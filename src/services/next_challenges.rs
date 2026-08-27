//! What to do next.
//!
//! ## Why this is not a design service
//!
//! The design backlog asked for `GET /users/me/design/next-suggestions`, with
//! a scoring function built from declared trades, families, format preference,
//! difficulty and deadline urgency. Every one of those exists for code, for AI
//! and for cybersecurity too — `reviewer_group` is a column on every
//! orientation, not a design idea — so a design-shaped twin would have been
//! copied four times and diverged three.
//!
//! The domain is a parameter. A design request scores design work because the
//! caller asked for design, not because the code knows what design is.
//!
//! ## Why two kinds of candidate in one list
//!
//! A challenge and a contest are the same question for the person reading:
//! *what should I spend this week on?* Returning them in separate lists makes
//! the client merge two rankings whose scores were never comparable, and it
//! makes "you have done three contests in a row" impossible to notice.
//!
//! ## Why the scores are small integers
//!
//! Nothing here is learned, and pretending otherwise would be dishonest — a
//! confidence of 0.87 implies a model that does not exist. These are a handful
//! of stated preferences added up, and the reason each point was awarded is
//! returned beside the total, so a reader can disagree with it.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How long a suggestion list stays fresh.
///
/// An hour. The inputs — declared trades, tier, what is open — move over days,
/// and a list that changed on every page load would stop reading as advice.
pub const CACHE_TTL_SECONDS: u64 = 60 * 60;

/// How many come back. Five: a list somebody reads, not a catalogue they
/// scroll past.
pub const SUGGESTION_COUNT: usize = 5;

/// Which format a piece of work has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// A brief claimed alone, reviewed round by round.
    Individual,
    /// A brief many people answer, ranked at the end.
    Contest,
}

impl Format {
    fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Contest => "contest",
        }
    }

    /// The kind of thing the suggestion points at, so a client can build the
    /// URL from the target's nature rather than inferring it from the format
    /// and the URL convention holding still. An individual brief is a slice; a
    /// contest is a tournament.
    fn target_kind(self) -> &'static str {
        match self {
            Self::Individual => "slice",
            Self::Contest => "tournament",
        }
    }
}

/// One suggestion, with the arithmetic that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// A slice id for `individual`, a tournament id for `contest`.
    pub id: Uuid,
    pub slug: Option<String>,
    pub title: String,
    pub format: Format,
    /// What the target is — `"slice"` or `"tournament"`. Derived from `format`
    /// and returned so a client never infers the target's nature from the URL
    /// convention (SKI-313): a third format, or a route that moves, would
    /// otherwise send a click somewhere wrong in silence. Owned rather than
    /// `&'static str` because a suggestion list is cached and read back
    /// (`cache::get_json`), which a borrowed field cannot deserialize into.
    pub target_kind: String,
    pub orientation_slug: Option<String>,
    /// The reviewer family the trade belongs to.
    pub family: Option<String>,
    pub difficulty: Option<i16>,
    pub estimated_hours: Option<i32>,
    /// When a contest stops taking entries.
    pub closes_at: Option<chrono::DateTime<chrono::Utc>>,
    pub score: i32,
    /// Why, one clause per point awarded. Returned rather than logged: a
    /// recommendation nobody can argue with is a recommendation nobody
    /// trusts.
    pub reasons: Vec<String>,
}

/// What the person has said about themselves, read once and applied to every
/// candidate.
struct Profile {
    orientations: Vec<String>,
    preferred_families: Vec<String>,
    challenge_preference: Option<String>,
    /// Where they sit on the domain's ladder, 1-based. `None` before a first
    /// score exists.
    tier_rank: Option<i32>,
    /// The format of the last thing they finished, which is the one not to
    /// suggest three times running.
    last_format: Option<Format>,
}

/// Points, all in one place so the weighting can be argued about without
/// reading the queries.
mod weight {
    /// The trade is one they declared. The strongest signal there is: it is
    /// the only one they chose deliberately and can change in a click.
    pub const DECLARED_TRADE: i32 = 3;
    /// The family is one they said interests them.
    pub const PREFERRED_FAMILY: i32 = 2;
    /// The format is the one they said they prefer.
    pub const PREFERRED_FORMAT: i32 = 2;
    /// The difficulty is within one step of where they sit.
    pub const DIFFICULTY_FITS: i32 = 1;
    /// Not the format they just did.
    pub const VARIETY: i32 = 1;
    /// A contest closing inside the week: worth surfacing now, because next
    /// week it is worth nothing.
    pub const CLOSING_SOON: i32 = 2;
}

/// Suggestions for one person in one domain.
///
/// Reads the database directly and is cached by the caller: the query set is
/// small but wide, and running it on every dashboard load would put a join
/// over four tables in the hot path of the page people open most.
pub async fn suggest(
    db: &PgPool,
    user_id: Uuid,
    domain: &str,
    limit: usize,
) -> Result<Vec<Suggestion>, AppError> {
    let profile = load_profile(db, user_id, domain).await?;

    let mut candidates = open_challenges(db, user_id, domain).await?;
    candidates.extend(open_contests(db, user_id, domain).await?);

    for candidate in &mut candidates {
        score(candidate, &profile);
    }

    // Highest first, and the closest deadline breaks a tie: between two equal
    // suggestions, the one that expires is the one worth reading first.
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| match (a.closes_at, b.closes_at) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
    });
    candidates.truncate(limit.clamp(1, 20));
    Ok(candidates)
}

fn score(candidate: &mut Suggestion, profile: &Profile) {
    let mut score = 0;
    let mut reasons = Vec::new();

    if let Some(slug) = &candidate.orientation_slug
        && profile.orientations.iter().any(|o| o == slug)
    {
        score += weight::DECLARED_TRADE;
        reasons.push(format!("dans un métier que tu as déclaré ({slug})"));
    }

    if let Some(family) = &candidate.family
        && profile.preferred_families.iter().any(|f| f == family)
    {
        score += weight::PREFERRED_FAMILY;
        reasons.push(format!("dans une famille qui t'intéresse ({family})"));
    }

    match profile.challenge_preference.as_deref() {
        Some("both") => {}
        Some(preferred) if preferred == candidate.format.as_str() => {
            score += weight::PREFERRED_FORMAT;
            reasons.push("dans le format que tu préfères".to_string());
        }
        _ => {}
    }

    // Difficulty is on 1-5 and the ladder has six tiers, so they are compared
    // as positions rather than as numbers: "about where you are" is the claim,
    // and one step either way is still about where you are.
    if let (Some(tier), Some(difficulty)) = (profile.tier_rank, candidate.difficulty)
        && (i32::from(difficulty) - tier).abs() <= 1
    {
        score += weight::DIFFICULTY_FITS;
        reasons.push("à peu près à ton niveau".to_string());
    }

    // Variety is scored against what they last finished rather than against
    // the rest of this list: the complaint the ticket describes — five
    // contests in a row — is about a habit, not about one page.
    if let Some(last) = profile.last_format
        && last != candidate.format
    {
        score += weight::VARIETY;
        reasons.push("change de format par rapport à ton dernier travail".to_string());
    }

    if let Some(closes_at) = candidate.closes_at {
        let hours = (closes_at - chrono::Utc::now()).num_hours();
        if (0..=24 * 7).contains(&hours) {
            score += weight::CLOSING_SOON;
            reasons.push(format!("ferme dans {} jours", (hours / 24).max(0) + 1));
        }
    }

    candidate.score = score;
    candidate.reasons = reasons;
}

async fn load_profile(db: &PgPool, user_id: Uuid, domain: &str) -> Result<Profile, AppError> {
    let orientations: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM user_orientations uo
           JOIN orientations o ON o.id = uo.orientation_id
          WHERE uo.user_id = $1 AND uo.ended_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let answers: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT answers FROM user_domain_profiles WHERE user_id = $1 AND domain = $2",
    )
    .bind(user_id)
    .bind(domain)
    .fetch_optional(db)
    .await?;
    let answers = answers.unwrap_or_else(|| serde_json::json!({}));

    let preferred_families = answers
        .get("preferred_families")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let challenge_preference = answers
        .get("challenge_preference")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // The tier's position on its own ladder, not its score: the ladders are
    // calibrated per domain and a raw score means nothing across two of them.
    let tier_rank: Option<i32> = sqlx::query_scalar(
        "SELECT t.sort_order FROM craft_scores cs
           JOIN craft_score_tiers t
                ON t.skill_domain = cs.skill_domain AND t.slug = cs.tier_slug
          WHERE cs.user_id = $1 AND cs.skill_domain = $2",
    )
    .bind(user_id)
    .bind(domain)
    .fetch_optional(db)
    .await?;

    // What they last finished. A contest entry and a validated slice are both
    // "finished something", and the more recent of the two is the habit.
    let last_slice: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT max(validated_at) FROM project_slices
          WHERE claimed_by_user_id = $1 AND validated_at IS NOT NULL",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .flatten();

    let last_contest: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT max(submitted_at) FROM tournament_submissions
          WHERE participant_type = 'user' AND participant_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .flatten();

    let last_format = match (last_slice, last_contest) {
        (Some(slice), Some(contest)) if contest > slice => Some(Format::Contest),
        (Some(_), _) => Some(Format::Individual),
        (None, Some(_)) => Some(Format::Contest),
        (None, None) => None,
    };

    Ok(Profile {
        orientations,
        preferred_families,
        challenge_preference,
        tier_rank,
        last_format,
    })
}

/// Open work in this domain that nobody has claimed.
///
/// Excludes anything the person has already worked on, whatever the outcome:
/// suggesting a challenge somebody abandoned is the fastest way to make a
/// dashboard look broken.
/// One open challenge, as the query returns it: id, title, trade slug,
/// reviewer family, difficulty, estimated hours.
type OpenChallengeRow = (
    Uuid,
    String,
    Option<String>,
    Option<String>,
    i16,
    Option<i32>,
);

async fn open_challenges(
    db: &PgPool,
    user_id: Uuid,
    domain: &str,
) -> Result<Vec<Suggestion>, AppError> {
    let rows: Vec<OpenChallengeRow> = sqlx::query_as(
        r#"
            SELECT s.id, s.title, o.slug, o.reviewer_group, s.difficulty, s.estimated_hours
              FROM project_slices s
              LEFT JOIN orientations o ON o.id = s.orientation_id
             WHERE s.status = 'open'
               AND s.claimed_by_user_id IS NULL
               AND s.claimed_by_team_id IS NULL
               AND s.primary_domain = $2
               AND NOT EXISTS (
                     SELECT 1 FROM slice_validation_decisions d
                      WHERE d.slice_id = s.id
                 )
               AND NOT EXISTS (
                     SELECT 1 FROM deliverables dv
                      WHERE dv.slice_id = s.id AND dv.user_id = $1
                 )
             ORDER BY s.created_at DESC
             LIMIT 100
            "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, orientation_slug, family, difficulty, estimated_hours)| Suggestion {
                id,
                slug: None,
                title,
                format: Format::Individual,
                target_kind: Format::Individual.target_kind().to_string(),
                orientation_slug,
                family,
                difficulty: Some(difficulty),
                estimated_hours,
                closes_at: None,
                score: 0,
                reasons: Vec::new(),
            },
        )
        .collect())
}

/// Contests somebody can still enter.
///
/// Domain-scoped ones plus those open to every domain: a cross-domain contest
/// is exactly the one that wants the widest field, and hiding it from a
/// domain dashboard would be the opposite of the intent.
async fn open_contests(
    db: &PgPool,
    user_id: Uuid,
    domain: &str,
) -> Result<Vec<Suggestion>, AppError> {
    let rows: Vec<(Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT t.id, t.slug, t.name, t.ends_at
          FROM tournaments t
         WHERE t.status IN ('registration', 'active')
           AND t.ends_at > NOW()
           AND (t.skill_domain = $2 OR t.skill_domain IS NULL)
           AND NOT EXISTS (
                 SELECT 1 FROM tournament_participants p
                  WHERE p.tournament_id = t.id
                    AND p.participant_type = 'user'
                    AND p.participant_id = $1
           )
         ORDER BY t.ends_at ASC
         LIMIT 50
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, slug, name, ends_at)| Suggestion {
            id,
            slug: Some(slug),
            title: name,
            format: Format::Contest,
            target_kind: Format::Contest.target_kind().to_string(),
            // A contest is addressed to a domain rather than to one trade, so
            // it earns the format and deadline points but never the trade
            // one. That is correct: it is a wider invitation.
            orientation_slug: None,
            family: None,
            difficulty: None,
            estimated_hours: None,
            closes_at: Some(ends_at),
            score: 0,
            reasons: Vec::new(),
        })
        .collect())
}

/// The Redis key. Per person and per domain, because both change the answer.
pub fn cache_key(user_id: Uuid, domain: &str) -> String {
    format!("next_challenges:{user_id}:{domain}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_candidate(format: Format) -> Suggestion {
        Suggestion {
            id: Uuid::nil(),
            slug: None,
            title: "Un brief".into(),
            format,
            target_kind: format.target_kind().to_string(),
            orientation_slug: Some("design-brand-identity".into()),
            family: Some("brand".into()),
            difficulty: Some(3),
            estimated_hours: Some(8),
            closes_at: None,
            score: 0,
            reasons: Vec::new(),
        }
    }

    fn a_profile() -> Profile {
        Profile {
            orientations: vec!["design-brand-identity".into()],
            preferred_families: vec!["brand".into()],
            challenge_preference: Some("individual".into()),
            tier_rank: Some(3),
            last_format: Some(Format::Contest),
        }
    }

    #[test]
    fn every_point_is_explained() {
        let mut candidate = a_candidate(Format::Individual);
        score(&mut candidate, &a_profile());

        // Trade, family, format, difficulty, variety.
        assert_eq!(candidate.score, 3 + 2 + 2 + 1 + 1);
        // A recommendation nobody can argue with is one nobody trusts, so
        // each point has to have said why.
        assert_eq!(candidate.reasons.len(), 5, "{:?}", candidate.reasons);
    }

    #[test]
    fn a_stranger_gets_a_score_of_zero_rather_than_nothing() {
        // Somebody who has declared nothing still has to be shown something:
        // an empty dashboard on day one is the worst possible first screen.
        let mut candidate = a_candidate(Format::Individual);
        let blank = Profile {
            orientations: Vec::new(),
            preferred_families: Vec::new(),
            challenge_preference: None,
            tier_rank: None,
            last_format: None,
        };
        score(&mut candidate, &blank);
        assert_eq!(candidate.score, 0);
        assert!(candidate.reasons.is_empty());
    }

    #[test]
    fn no_variety_point_for_the_format_just_done() {
        let mut candidate = a_candidate(Format::Contest);
        let mut profile = a_profile();
        profile.challenge_preference = Some("contest".into());
        score(&mut candidate, &profile);
        assert!(
            !candidate
                .reasons
                .iter()
                .any(|r| r.contains("change de format")),
            "{:?}",
            candidate.reasons
        );
    }

    #[test]
    fn both_earns_no_format_point_either_way() {
        // Somebody who answered "both" has expressed no preference, and
        // awarding the point to everything would just add a constant.
        for format in [Format::Individual, Format::Contest] {
            let mut candidate = a_candidate(format);
            let mut profile = a_profile();
            profile.challenge_preference = Some("both".into());
            score(&mut candidate, &profile);
            assert!(
                !candidate
                    .reasons
                    .iter()
                    .any(|r| r.contains("format que tu")),
                "{:?}",
                candidate.reasons
            );
        }
    }

    #[test]
    fn a_difficulty_two_steps_away_is_not_about_your_level() {
        let mut candidate = a_candidate(Format::Individual);
        candidate.difficulty = Some(5);
        score(&mut candidate, &a_profile());
        assert!(
            !candidate.reasons.iter().any(|r| r.contains("ton niveau")),
            "{:?}",
            candidate.reasons
        );
    }

    #[test]
    fn a_deadline_already_past_earns_nothing() {
        let mut candidate = a_candidate(Format::Contest);
        candidate.closes_at = Some(chrono::Utc::now() - chrono::Duration::hours(2));
        score(&mut candidate, &a_profile());
        assert!(
            !candidate.reasons.iter().any(|r| r.contains("ferme dans")),
            "{:?}",
            candidate.reasons
        );
    }
}
