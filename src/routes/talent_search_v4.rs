//! Recruiter search — one endpoint, replacing three.
//!
//! ## Why one and not four
//!
//! v1, v2 and v3 each added filters the previous one lacked, and each kept
//! its own SQL, its own row shape and its own pagination. Three endpoints
//! answering the same question differently is three places for a filter to be
//! subtly wrong, and a caller who cannot tell which one to use picks the
//! oldest. They are gone; this is what they were converging on.
//!
//! ## What it filters on
//!
//! Everything the three did — free text, domain, country, availability,
//! languages, badges, tags — plus what the platform has learned to record
//! since: the trade (one of the 127 orientations), a specific capability, a
//! craft-score tier, and whether somebody has a *proved* account on an
//! external platform.
//!
//! ## Two decisions worth stating
//!
//! **Keyset pagination, not offset.** A recruiter paginating while somebody's
//! score changes gets a skipped or repeated row with an offset, silently. The
//! cursor is `(score, id)`, which is the order rows are returned in.
//!
//! **Cached for fifteen minutes, keyed by the whole query.** Recruiters
//! re-run the same search across a session, and none of the inputs — scores,
//! orientations, capabilities — moves faster than the hourly sweep that
//! writes them.
//!
//! ## What somebody is, and what they have done
//!
//! The filters above describe a person: their trade, their skills, their
//! country. A recruiter also asks the other question — *what have they
//! finished* — and until now the endpoint could not answer it: contests won,
//! missions delivered and editorial featurings were all recorded and none of
//! them was reachable from a search.
//!
//! They are filters here, and sort keys, on the one endpoint. Not a second
//! endpoint per domain: a design track record and a security one are the same
//! four facts about different work, and the moment there are two queries the
//! filters start disagreeing — which is the whole reason v4 exists.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::services::SkillsService;

/// Recruiters re-run the same search across a session, and nothing it reads
/// moves faster than the hourly score sweep.
const CACHE_TTL_SECS: u64 = 15 * 60;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

pub fn talent_search_v4_routes() -> Router<AppState> {
    Router::new()
        .route("/talents/search", get(search))
        .route("/talents/{username}/card", get(talent_card))
}

/// Mirrors the CHECK on `users.looking_for`. Spelled out in the contract
/// rather than checked in the handler, so a client generated from the spec
/// cannot compile a search that will be refused.
#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LookingFor {
    Cdi,
    Cdd,
    Freelance,
    Internship,
    Contract,
}

impl LookingFor {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cdi => "cdi",
            Self::Cdd => "cdd",
            Self::Freelance => "freelance",
            Self::Internship => "internship",
            Self::Contract => "contract",
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Free text, over the profile search vector.
    #[param(max_length = 200)]
    pub q: Option<String>,
    /// One of the eleven domains.
    #[param(max_length = 30)]
    pub skill_domain: Option<String>,
    /// Trade slug. Follows a rename, so a bookmarked URL keeps working.
    #[param(max_length = 100)]
    pub orientation: Option<String>,
    /// `active` (default), `learning`, or `both`. Learning orientations are
    /// aspirations; including them by default would fill a recruiter's
    /// results with people who have proved nothing in that trade.
    #[serde(default = "default_mode")]
    #[param(pattern = r"^(active|learning|both)$")]
    pub mode: String,
    /// Only people for whom this is their primary trade.
    #[serde(default)]
    pub only_primary: bool,
    /// Family within a trade — `brand`, `motion`, `systems`, `llm-nlp`. It is
    /// the grouping reviewers are drawn from, which makes it the one grouping
    /// the platform actually maintains; a recruiter looking for "somebody in
    /// motion" is asking exactly this.
    #[param(max_length = 40)]
    pub family: Option<String>,
    /// CSV of skill slugs. Every one must be proved at `min_proficiency`.
    #[param(max_length = 500)]
    pub skills: Option<String>,
    #[serde(default = "default_min_proficiency")]
    #[param(minimum = 1, maximum = 5)]
    pub min_proficiency: i16,
    /// A platform capability, e.g. `code_reviewer:systems`.
    #[param(max_length = 60)]
    pub capability: Option<String>,
    /// Craft-score tier slug, e.g. `senior`. Matches that tier and above.
    #[param(max_length = 40)]
    pub min_tier: Option<String>,
    #[param(minimum = 0, maximum = 10000)]
    pub min_craft_score: Option<i32>,
    /// An external platform the person has *proved* they own — `github`,
    /// `gitlab`, `crates_io`. A claimed handle does not match: anybody can
    /// type anybody's.
    #[param(max_length = 30)]
    pub platform: Option<String>,
    #[param(pattern = r"^[A-Z]{2}$")]
    pub country_iso2: Option<String>,
    /// Two-letter code, matched at B2 or above.
    #[param(pattern = r"^[a-zA-Z]{2}$")]
    pub language_spoken: Option<String>,
    /// What kind of engagement the person is open to.
    #[param(inline)]
    pub looking_for: Option<LookingFor>,
    #[serde(default)]
    pub available_only: bool,
    /// Badge slug.
    #[param(max_length = 100)]
    pub badge: Option<String>,
    /// Tag slug.
    #[param(max_length = 100)]
    pub tag: Option<String>,
    /// Contests concluded and won outright. A podium is not a win: the
    /// distinction is the whole point of a ranking.
    #[param(minimum = 1)]
    pub min_contests_won: Option<i64>,
    /// Missions handed in and accepted. Cancelled and in-flight ones do not
    /// count — an unfinished mission says nothing about finishing.
    #[param(minimum = 1)]
    pub min_missions_delivered: Option<i64>,
    /// Only people featured within this many days. A featuring is editorial,
    /// and an old one says what somebody thought two years ago.
    #[param(minimum = 1, maximum = 3650)]
    pub featured_within_days: Option<i32>,
    /// `craft_score` (default), `contests_won`, `missions_delivered` or
    /// `recently_featured`. Always descending: nobody searches for the worst
    /// match first.
    #[serde(default = "default_sort")]
    #[param(pattern = r"^(craft_score|contests_won|missions_delivered|recently_featured)$")]
    pub sort: String,
    /// From the previous page's `next_cursor`: a score and a user id. The
    /// shape is in the contract because a cursor the handler cannot decode is
    /// refused rather than read as "from the beginning", and a caller has to
    /// be able to tell a malformed cursor from an empty result.
    #[param(pattern = r"^-?\d+\|[0-9a-fA-F-]{36}$", max_length = 100)]
    pub after: Option<String>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

fn default_mode() -> String {
    "active".into()
}
fn default_min_proficiency() -> i16 {
    1
}
fn default_limit() -> i64 {
    DEFAULT_LIMIT
}
fn default_sort() -> String {
    "craft_score".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Talent {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub title: String,
    pub skill_domain: String,
    pub country_iso2: Option<String>,
    /// In the searched domain when one was named, otherwise the best of any.
    /// Zero means never computed, which is not the same as measured at zero.
    pub craft_score: i32,
    pub craft_tier: Option<String>,
    pub scored_domain: Option<String>,
    /// The trades this person declares, active ones first.
    pub orientations: Vec<String>,
    /// The families those trades belong to, deduplicated. Somebody doing
    /// brand identity and illustration reads as `["brand", "illustration"]`.
    pub families: Vec<String>,
    /// Platforms whose ownership was proved. Claimed ones are absent: this is
    /// a recruiter surface, and an unproved handle is not evidence.
    pub verified_platforms: Vec<String>,
    pub available_for_work: bool,
    pub badge_count: i64,
    /// How many people have vouched for them. Its own field, never folded
    /// into the craft score: an endorsement is somebody's opinion, and it
    /// must not be able to masquerade as verified work.
    pub vouched_by_count: i64,
    /// Contests won outright, across every domain.
    pub contests_won: i64,
    /// Missions handed in and accepted.
    pub missions_delivered: i64,
    /// The Monday of the most recent week this person was featured, if ever.
    /// A date rather than a boolean: "featured" without a when is a claim
    /// that never expires.
    pub last_featured_on: Option<chrono::NaiveDate>,
    /// The value this page was ordered by, so the cursor is derivable from
    /// the last row without the caller knowing which sort was asked for.
    #[serde(skip)]
    pub sort_key: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub talents: Vec<Talent>,
    pub next_cursor: Option<String>,
    /// Echoed back so a cached answer is self-describing.
    pub filters_applied: Vec<String>,
}

/// Where a page stopped. `(sort_key, user_id)` — the order rows come back in,
/// so the next page resumes exactly where this one ended.
///
/// The key is whatever was sorted on, not always the craft score: a cursor
/// that always carried the score would skip and repeat rows the moment a
/// recruiter sorted by anything else, silently, which is the failure keyset
/// pagination exists to prevent.
///
/// It carries no sort name. Feeding a cursor from one sort into another
/// paginates nonsense either way, and encoding the name would only let the
/// endpoint pretend it had checked.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub key: i64,
    pub user_id: Uuid,
}

impl Cursor {
    pub fn encode(&self) -> String {
        format!("{}|{}", self.key, self.user_id)
    }

    pub fn decode(raw: &str) -> Option<Self> {
        let (key, user_id) = raw.split_once('|')?;
        Some(Cursor {
            key: key.parse().ok()?,
            user_id: user_id.parse().ok()?,
        })
    }
}

/// Search for people.
///
/// Public and unauthenticated, like the three it replaces: a profile that
/// somebody chose to make public is public, and putting a login in front of
/// the search would mean the platform's whole argument is only visible to
/// people who already believe it.
#[utoipa::path(
    get,
    path = "/api/talents/search",
    tag = "enterprise",
    params(SearchQuery),
    responses(
        (status = 200, description = "Matching talents", body = ApiResponse<SearchResponse>),
        (status = 400, description = "Invalid filter or cursor", body = crate::api_response::ErrorResponse),
    ),
    operation_id = "talentSearchV4Search",
)]
pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<ApiResponse<SearchResponse>>, AppError> {
    validate(&q)?;

    let cursor = match q.after.as_deref() {
        Some(raw) => Some(
            Cursor::decode(raw)
                .ok_or_else(|| AppError::Validation("that cursor is unusable".into()))?,
        ),
        None => None,
    };

    let cache_key = cache_key_for(&state, &q);
    let mut redis = state.redis.clone();
    if let Some(cached) =
        crate::services::cache::get_json::<SearchResponse>(&mut redis, &cache_key).await?
    {
        return Ok(Json(ApiResponse::new(cached)));
    }

    // An unknown trade answers nothing rather than everything. Silently
    // dropping the filter would show a recruiter a full page of people who do
    // not do the job they searched for.
    let orientation_id: Option<Uuid> = match &q.orientation {
        Some(slug) => {
            let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
                .bind(slug)
                .fetch_one(&state.db)
                .await?;
            Some(
                resolved
                    .ok_or_else(|| AppError::NotFound(format!("orientation '{slug}' not found")))?,
            )
        }
        None => None,
    };

    // A tier is a floor, so it becomes a score threshold — one comparison
    // rather than a join and a range condition on every row.
    let tier_floor: Option<i32> = match &q.min_tier {
        Some(slug) => {
            let floor: Option<i32> = sqlx::query_scalar(
                "SELECT min_score FROM craft_score_tiers
                  WHERE slug = $1 AND skill_domain = COALESCE($2, 'code')",
            )
            .bind(slug)
            .bind(q.skill_domain.as_deref())
            .fetch_optional(&state.db)
            .await?;
            Some(floor.ok_or_else(|| AppError::NotFound(format!("no tier '{slug}'")))?)
        }
        None => None,
    };

    let skills: Vec<String> = q
        .skills
        .as_deref()
        .map(|csv| {
            csv.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let score_floor = q.min_craft_score.max(tier_floor);

    let talents = sqlx::query_as::<_, Talent>(
        r#"
        SELECT * FROM (
        SELECT u.id AS user_id,
               u.username,
               u.display_name,
               u.title,
               u.skill_domain,
               u.country_iso2,
               COALESCE(best.score, 0) AS craft_score,
               best.tier_slug AS craft_tier,
               best.skill_domain AS scored_domain,
               COALESCE(orient.slugs, ARRAY[]::TEXT[]) AS orientations,
               COALESCE(orient.families, ARRAY[]::TEXT[]) AS families,
               COALESCE(plat.platforms, ARRAY[]::TEXT[]) AS verified_platforms,
               u.available_for_hire AS available_for_work,
               COALESCE(badges.n, 0) AS badge_count,
               COALESCE(vouches.n, 0) AS vouched_by_count,
               COALESCE(wins.n, 0) AS contests_won,
               COALESCE(delivered.n, 0) AS missions_delivered,
               feat.week_of AS last_featured_on,

               -- The value the page is ordered and paginated by. Computed
               -- once here rather than repeated in ORDER BY and in the cursor
               -- comparison, where the two could drift apart.
               CASE $21::TEXT
                   WHEN 'contests_won' THEN COALESCE(wins.n, 0)
                   WHEN 'missions_delivered' THEN COALESCE(delivered.n, 0)
                   -- Days since the epoch. Never featured sorts below anybody
                   -- who ever was, rather than alongside 1970.
                   WHEN 'recently_featured'
                       THEN COALESCE(feat.week_of - DATE '1970-01-01', -1)
                   ELSE COALESCE(best.score, 0)
               END::BIGINT AS sort_key

          FROM users u

          -- The score in the searched domain, or the best one when no domain
          -- was named. A recruiter searching without a domain wants the
          -- strongest profiles, not an arbitrary one of eleven.
          LEFT JOIN LATERAL (
              SELECT cs.score, cs.tier_slug, cs.skill_domain
                FROM craft_scores cs
               WHERE cs.user_id = u.id
                 AND ($2::TEXT IS NULL OR cs.skill_domain = $2)
               ORDER BY cs.score DESC
               LIMIT 1
          ) AS best ON TRUE

          LEFT JOIN LATERAL (
              SELECT array_agg(o.slug ORDER BY uo.is_primary DESC, o.slug) AS slugs,
                     array_agg(DISTINCT o.reviewer_group)
                         FILTER (WHERE o.reviewer_group IS NOT NULL) AS families
                FROM user_orientations uo
                JOIN orientations o ON o.id = uo.orientation_id
               WHERE uo.user_id = u.id AND uo.ended_at IS NULL
          ) AS orient ON TRUE

          -- Proved accounts only, from both places one lives: the forges and
          -- registries of `user_code_portfolios`, and the design and film
          -- portfolios of `external_signals`. A claimed handle is not
          -- evidence, and this is the surface where that matters most.
          LEFT JOIN LATERAL (
              -- Two tables, because 0415 renamed `user_code_portfolios` to
              -- `user_external_portfolios` without absorbing
              -- `external_signals`: registries and forges are in the first,
              -- the design platforms in the second. A recruiter filtering on
              -- `platform` means one thing by it, so both are read.
              SELECT array_agg(DISTINCT name) AS platforms FROM (
                  SELECT p.platform AS name
                    FROM user_external_portfolios p
                   WHERE p.user_id = u.id AND p.verified_at IS NOT NULL
                  UNION
                  SELECT es.provider
                    FROM external_signals es
                   WHERE es.user_id = u.id AND es.verified_at IS NOT NULL
              ) AS both_kinds
          ) AS plat ON TRUE

          LEFT JOIN LATERAL (
              SELECT count(*) AS n FROM user_badges ub
               WHERE ub.user_id = u.id AND ub.revoked_at IS NULL
          ) AS badges ON TRUE

          -- Live endorsements only: expired and broken ones are not
          -- somebody's current opinion. Counted separately from the score,
          -- never folded into it.
          LEFT JOIN LATERAL (
              SELECT count(*) AS n FROM vouchings v
               WHERE v.vouched_id = u.id
                 AND v.broken_at IS NULL
                 AND v.active_until > NOW()
          ) AS vouches ON TRUE

          -- Won outright, in a contest that finished. Second place is not a
          -- win, and a contest still running has no result to report.
          LEFT JOIN LATERAL (
              SELECT count(*) AS n
                FROM tournament_participants tp
                JOIN tournaments t ON t.id = tp.tournament_id
               WHERE tp.participant_type = 'user'
                 AND tp.participant_id = u.id
                 AND tp.rank = 1
                 AND t.status = 'concluded'
          ) AS wins ON TRUE

          -- Handed in and accepted. `cancelled` and everything before
          -- `delivered` say nothing about finishing.
          LEFT JOIN LATERAL (
              SELECT count(*) AS n
                FROM missions m
               WHERE m.assigned_user_id = u.id
                 AND m.status IN ('delivered', 'closed')
          ) AS delivered ON TRUE

          LEFT JOIN LATERAL (
              SELECT max(ft.week_of) AS week_of
                FROM featured_talents ft
               WHERE ft.user_id = u.id
          ) AS feat ON TRUE

         WHERE u.profile_active = TRUE
           AND u.is_banned = FALSE
           AND u.profile_hidden = FALSE

           AND ($1::TEXT IS NULL OR u.search_vector @@ plainto_tsquery('simple', $1))
           AND ($2::TEXT IS NULL OR u.skill_domain = $2
                OR EXISTS (SELECT 1 FROM craft_scores cs2
                            WHERE cs2.user_id = u.id AND cs2.skill_domain = $2))
           AND ($3::UUID IS NULL OR EXISTS (
                   SELECT 1 FROM user_orientations uo
                    WHERE uo.user_id = u.id
                      AND uo.orientation_id = $3
                      AND uo.ended_at IS NULL
                      AND ($4::TEXT = 'both' OR uo.mode = $4)
                      AND (NOT $5::BOOLEAN OR uo.is_primary)))

           -- Every named skill, not any of them: a recruiter asking for React
           -- and TypeScript wants somebody who has both.
           AND (cardinality($6::TEXT[]) = 0 OR NOT EXISTS (
                   SELECT 1 FROM unnest($6::TEXT[]) AS wanted(slug)
                    WHERE NOT EXISTS (
                        SELECT 1 FROM user_skills us
                          JOIN skill_nodes sn ON sn.id = us.skill_id
                         WHERE us.user_id = u.id
                           AND sn.slug = wanted.slug
                           AND us.proficiency_level >= $7)))

           AND ($8::TEXT IS NULL OR EXISTS (
                   SELECT 1 FROM user_capabilities uc
                    WHERE uc.user_id = u.id AND uc.capability = $8
                      AND uc.revoked_at IS NULL
                      AND (uc.expires_at IS NULL OR uc.expires_at > NOW())))

           AND ($9::INTEGER IS NULL OR COALESCE(best.score, 0) >= $9)

           -- Both tables, for the same reason the column above reads both.
           AND ($10::TEXT IS NULL
                OR EXISTS (SELECT 1 FROM user_external_portfolios p2
                            WHERE p2.user_id = u.id AND p2.platform = $10
                              AND p2.verified_at IS NOT NULL)
                OR EXISTS (SELECT 1 FROM external_signals es2
                            WHERE es2.user_id = u.id AND es2.provider = $10
                              AND es2.verified_at IS NOT NULL))

           AND ($11::TEXT IS NULL OR u.country_iso2 = $11)
           -- B2 and above. Below that somebody can read the language, not
           -- work in it, and a recruiter filtering on it means the second.
           AND ($12::TEXT IS NULL OR EXISTS (
                   SELECT 1 FROM user_languages ul
                    WHERE ul.user_id = u.id
                      AND lower(ul.language) = lower($12)
                      AND ul.proficiency IN ('B2', 'C1', 'C2', 'native')))
           AND ($13::TEXT IS NULL OR u.looking_for = $13)
           AND (NOT $14::BOOLEAN OR u.available_for_hire = TRUE)
           AND ($15::TEXT IS NULL OR EXISTS (
                   SELECT 1 FROM user_badges ub2
                     JOIN badges b ON b.id = ub2.badge_id
                    WHERE ub2.user_id = u.id AND b.slug = $15
                      AND ub2.revoked_at IS NULL))
           AND ($16::TEXT IS NULL OR EXISTS (
                   SELECT 1 FROM tag_map tm
                     JOIN tags t2 ON t2.id = tm.tag_id
                    WHERE tm.target_type = 'user' AND tm.target_id = u.id
                      AND t2.slug = $16))

           -- The family is read through the same active-orientation rule as
           -- the trade filter: somebody who has left a trade has left its
           -- family with it.
           AND ($17::TEXT IS NULL OR EXISTS (
                   SELECT 1 FROM user_orientations uo2
                     JOIN orientations o2 ON o2.id = uo2.orientation_id
                    WHERE uo2.user_id = u.id
                      AND uo2.ended_at IS NULL
                      AND o2.reviewer_group = $17))

           AND ($18::BIGINT IS NULL OR COALESCE(wins.n, 0) >= $18)
           AND ($19::BIGINT IS NULL OR COALESCE(delivered.n, 0) >= $19)
           AND ($20::INTEGER IS NULL
                OR feat.week_of >= (CURRENT_DATE - ($20::INTEGER * INTERVAL '1 day')))
        ) AS ranked

         WHERE ($22::BIGINT IS NULL
                OR (ranked.sort_key, ranked.user_id) < ($22::BIGINT, $23::UUID))

         ORDER BY ranked.sort_key DESC, ranked.user_id DESC
         LIMIT $24
        "#,
    )
    .bind(q.q.as_deref())
    .bind(q.skill_domain.as_deref())
    .bind(orientation_id)
    .bind(&q.mode)
    .bind(q.only_primary)
    .bind(&skills)
    .bind(q.min_proficiency)
    .bind(q.capability.as_deref())
    .bind(score_floor)
    .bind(q.platform.as_deref())
    .bind(q.country_iso2.as_deref())
    .bind(q.language_spoken.as_deref())
    .bind(q.looking_for.map(LookingFor::as_str))
    .bind(q.available_only)
    .bind(q.badge.as_deref())
    .bind(q.tag.as_deref())
    .bind(q.family.as_deref())
    .bind(q.min_contests_won)
    .bind(q.min_missions_delivered)
    .bind(q.featured_within_days)
    .bind(&q.sort)
    .bind(cursor.map(|c| c.key))
    .bind(cursor.map(|c| c.user_id))
    .bind(q.limit)
    .fetch_all(&state.db)
    .await?;

    // A cursor only on a full page: handing one back on a short page makes a
    // caller poll for a page that will always be empty.
    let next_cursor = (talents.len() as i64 == q.limit)
        .then(|| talents.last())
        .flatten()
        .map(|last| {
            Cursor {
                key: last.sort_key,
                user_id: last.user_id,
            }
            .encode()
        });

    let response = SearchResponse {
        talents,
        next_cursor,
        filters_applied: filters_applied(&q, &skills),
    };
    let _ =
        crate::services::cache::set_json(&mut redis, &cache_key, &response, CACHE_TTL_SECS).await;

    Ok(Json(ApiResponse::new(response)))
}

fn validate(q: &SearchQuery) -> Result<(), AppError> {
    crate::validators::check_max_len_opt(&q.q, "q", 200)?;
    crate::validators::check_max_len_opt(&q.skill_domain, "skill_domain", 30)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 100)?;
    crate::validators::check_max_len_opt(&q.skills, "skills", 500)?;
    crate::validators::check_max_len_opt(&q.capability, "capability", 60)?;
    crate::validators::check_max_len_opt(&q.min_tier, "min_tier", 40)?;
    crate::validators::check_max_len_opt(&q.platform, "platform", 30)?;
    crate::validators::check_max_len_opt(&q.badge, "badge", 100)?;
    crate::validators::check_max_len_opt(&q.tag, "tag", 100)?;
    crate::validators::check_max_len_opt(&q.family, "family", 40)?;
    crate::validators::check_max_len_opt(&q.after, "after", 100)?;

    if !matches!(q.mode.as_str(), "active" | "learning" | "both") {
        return Err(AppError::Validation(
            "mode must be active, learning or both".into(),
        ));
    }
    if !(1..=5).contains(&q.min_proficiency) {
        return Err(AppError::Validation(
            "min_proficiency must be between 1 and 5".into(),
        ));
    }
    if !(1..=MAX_LIMIT).contains(&q.limit) {
        return Err(AppError::Validation(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    if !matches!(
        q.sort.as_str(),
        "craft_score" | "contests_won" | "missions_delivered" | "recently_featured"
    ) {
        return Err(AppError::Validation(
            "sort must be craft_score, contests_won, missions_delivered or recently_featured"
                .into(),
        ));
    }
    // Floors, so zero is not a filter — asking for at least nothing is asking
    // for nothing, and a caller who sends it means they left the field blank.
    for (name, value) in [
        ("min_contests_won", q.min_contests_won),
        ("min_missions_delivered", q.min_missions_delivered),
    ] {
        if let Some(v) = value
            && v < 1
        {
            return Err(AppError::Validation(format!("{name} must be at least 1")));
        }
    }
    // Ten years. Beyond that the filter is not narrowing anything, and the
    // ceiling keeps a caller from writing a number that overflows the
    // interval it becomes.
    if let Some(days) = q.featured_within_days
        && !(1..=3650).contains(&days)
    {
        return Err(AppError::Validation(
            "featured_within_days must be between 1 and 3650".into(),
        ));
    }
    Ok(())
}

/// What actually narrowed the search, in the answer.
///
/// A recruiter who gets forty results wants to know which of their eight
/// filters the endpoint honoured — silently ignoring one is how somebody
/// concludes nobody matches when the filter was simply dropped.
fn filters_applied(q: &SearchQuery, skills: &[String]) -> Vec<String> {
    let mut applied = Vec::new();
    let mut note = |name: &str| applied.push(name.to_string());

    if q.q.is_some() {
        note("q");
    }
    if q.skill_domain.is_some() {
        note("skill_domain");
    }
    if q.orientation.is_some() {
        note("orientation");
        if q.mode != "active" {
            note("mode");
        }
        if q.only_primary {
            note("only_primary");
        }
    }
    if !skills.is_empty() {
        note("skills");
    }
    if q.capability.is_some() {
        note("capability");
    }
    if q.min_tier.is_some() {
        note("min_tier");
    }
    if q.min_craft_score.is_some() {
        note("min_craft_score");
    }
    if q.platform.is_some() {
        note("platform");
    }
    if q.country_iso2.is_some() {
        note("country_iso2");
    }
    if q.language_spoken.is_some() {
        note("language_spoken");
    }
    if q.looking_for.is_some() {
        note("looking_for");
    }
    if q.available_only {
        note("available_only");
    }
    if q.badge.is_some() {
        note("badge");
    }
    if q.tag.is_some() {
        note("tag");
    }
    if q.family.is_some() {
        note("family");
    }
    if q.min_contests_won.is_some() {
        note("min_contests_won");
    }
    if q.min_missions_delivered.is_some() {
        note("min_missions_delivered");
    }
    if q.featured_within_days.is_some() {
        note("featured_within_days");
    }
    // The sort is not a filter, but it changes which rows a page holds as
    // surely as one does, and a recruiter comparing two pages needs to know
    // it moved.
    if q.sort != "craft_score" {
        note(&format!("sort:{}", q.sort));
    }
    applied
}

/// Namespaced by database, like every other cached answer: two deployments
/// sharing a Redis must not serve each other's results.
fn cache_key_for(state: &AppState, q: &SearchQuery) -> String {
    let field = |value: &Option<String>| value.as_deref().unwrap_or("-").to_string();
    format!(
        "talents:v4:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        state.db.connect_options().get_database().unwrap_or("db"),
        field(&q.q),
        field(&q.skill_domain),
        field(&q.orientation),
        q.mode,
        q.only_primary,
        field(&q.skills),
        q.min_proficiency,
        field(&q.capability),
        field(&q.min_tier),
        q.min_craft_score.unwrap_or(-1),
        field(&q.platform),
        field(&q.country_iso2),
        field(&q.language_spoken),
        q.looking_for.map(LookingFor::as_str).unwrap_or("-"),
        q.available_only,
        field(&q.badge),
        field(&q.tag),
        field(&q.family),
        q.min_contests_won.unwrap_or(-1),
        q.min_missions_delivered.unwrap_or(-1),
        q.featured_within_days.unwrap_or(-1),
        q.sort,
        format_args!("{}:{}", field(&q.after), q.limit),
    )
}

// ═══════════════════════════════════════════════════════════════════
// The card
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentCardTopSkill {
    pub domain: String,
    pub sub_skill: String,
    pub fragments: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentCardResponse {
    pub username: String,
    pub display_name: String,
    pub skill_domain: String,
    pub title: String,
    pub country: Option<String>,
    pub member_since: String,
    pub top_skills: Vec<TalentCardTopSkill>,
    pub badge_count: i64,
    /// Every domain this person has a computed score in, best first.
    pub craft_scores: Vec<CardScore>,
}

/// Who the card is about, before anything is counted about them.
#[derive(sqlx::FromRow)]
struct CardIdentity {
    id: Uuid,
    username: String,
    display_name: String,
    skill_domain: String,
    title: String,
    country: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct CardScore {
    pub skill_domain: String,
    pub score: i32,
    pub tier_slug: Option<String>,
}

/// One person, enough to render a card. Public and server-side friendly.
#[utoipa::path(
    get,
    path = "/api/talents/{username}/card",
    tag = "enterprise",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "Talent card", body = ApiResponse<TalentCardResponse>),
        (status = 404, description = "No such talent", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn talent_card(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<TalentCardResponse>>, AppError> {
    let talent: Option<CardIdentity> = sqlx::query_as(
        "SELECT id, username, display_name, skill_domain, title, country, created_at
           FROM users
          WHERE username = $1 AND profile_active = TRUE
            AND is_banned = FALSE AND profile_hidden = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    let CardIdentity {
        id,
        username,
        display_name,
        skill_domain,
        title,
        country,
        created_at,
    } = talent.ok_or_else(|| AppError::NotFound("talent not found".into()))?;

    let top_skills = SkillsService::list_user_top_skills(&state.db, id, 3).await?;

    let badge_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_badges WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    // Every domain, not only code: a card that shows one score for somebody
    // who works across two says less than it knows.
    let craft_scores = sqlx::query_as::<_, CardScore>(
        "SELECT skill_domain, score, tier_slug FROM craft_scores
          WHERE user_id = $1 AND score > 0
          ORDER BY score DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(TalentCardResponse {
        username,
        display_name,
        skill_domain,
        title,
        country,
        member_since: created_at.to_rfc3339(),
        top_skills: top_skills
            .into_iter()
            .map(|(domain, sub_skill, fragments)| TalentCardTopSkill {
                domain,
                sub_skill,
                fragments,
            })
            .collect(),
        badge_count,
        craft_scores,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_survives_the_round_trip() {
        let cursor = Cursor {
            key: 1420,
            user_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        };
        let decoded = Cursor::decode(&cursor.encode()).expect("round trip");
        assert_eq!(decoded.key, cursor.key);
        assert_eq!(decoded.user_id, cursor.user_id);
    }

    #[test]
    fn a_key_of_zero_still_paginates() {
        // Everybody with no contest win shares a key, as does everybody
        // unscored, so the id is what separates them. A cursor that dropped
        // it would loop on the first page.
        let cursor = Cursor {
            key: 0,
            user_id: Uuid::nil(),
        };
        assert!(Cursor::decode(&cursor.encode()).is_some());
    }

    #[test]
    fn a_malformed_cursor_is_refused_rather_than_guessed() {
        assert!(Cursor::decode("").is_none());
        assert!(Cursor::decode("abc|11111111-2222-3333-4444-555555555555").is_none());
        assert!(Cursor::decode("100|nope").is_none());
        assert!(Cursor::decode("100").is_none());
    }

    fn a_query() -> SearchQuery {
        SearchQuery {
            q: None,
            skill_domain: None,
            orientation: None,
            family: None,
            min_contests_won: None,
            min_missions_delivered: None,
            featured_within_days: None,
            sort: default_sort(),
            mode: "active".into(),
            only_primary: false,
            skills: None,
            min_proficiency: 1,
            capability: None,
            min_tier: None,
            min_craft_score: None,
            platform: None,
            country_iso2: None,
            language_spoken: None,
            looking_for: None,
            available_only: false,
            badge: None,
            tag: None,
            after: None,
            limit: DEFAULT_LIMIT,
        }
    }

    #[test]
    fn an_empty_search_narrows_nothing_and_says_so() {
        assert!(filters_applied(&a_query(), &[]).is_empty());
    }

    #[test]
    fn every_filter_reports_itself() {
        // A recruiter who gets forty results needs to know which of their
        // filters was honoured; a silently dropped one reads as "nobody
        // matches".
        let mut q = a_query();
        q.orientation = Some("web-frontend-developer".into());
        q.only_primary = true;
        q.mode = "both".into();
        q.capability = Some("code_reviewer:web".into());
        q.platform = Some("github".into());
        q.available_only = true;

        let applied = filters_applied(&q, &["react".to_string()]);
        for expected in [
            "orientation",
            "mode",
            "only_primary",
            "skills",
            "capability",
            "platform",
            "available_only",
        ] {
            assert!(
                applied.contains(&expected.to_string()),
                "{expected} missing"
            );
        }
    }

    #[test]
    fn mode_is_only_reported_when_a_trade_was_asked_for() {
        // Without an orientation the mode filters nothing, and reporting it
        // would tell a recruiter their search was narrowed when it was not.
        let mut q = a_query();
        q.mode = "both".into();
        assert!(!filters_applied(&q, &[]).contains(&"mode".to_string()));
    }

    #[test]
    fn a_bad_mode_is_refused() {
        let mut q = a_query();
        q.mode = "aspirational".into();
        assert!(validate(&q).is_err());
    }

    #[test]
    fn a_limit_beyond_the_maximum_is_refused_rather_than_clamped() {
        // Clamping silently gives a caller fewer rows than they asked for and
        // no way to tell that from "there were only that many".
        let mut q = a_query();
        q.limit = 500;
        assert!(validate(&q).is_err());
        q.limit = 0;
        assert!(validate(&q).is_err());
    }
}
