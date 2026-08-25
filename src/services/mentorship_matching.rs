//! Matching a mentee with a mentor, in whichever domain they work in.
//!
//! Generic mentorship matching asks "who is available and knows this area".
//! That is not enough: thirty-three code trades and ten AI ones mean "knows
//! this area" can be true and useless — a kernel engineer is not the right
//! person to review somebody's first React component, an MLOps engineer is
//! not the right person to read an alignment experiment, and both pairs would
//! find that out an hour into a paid session.
//!
//! ## One module, two domains
//!
//! Everything that decides the ordering is domain-agnostic: shared families,
//! shared tools, the score gap, the timezone, the load. What differs is a
//! handful of strings — which domain to score, which answer key holds the
//! tools, how many mentees is too many — so they are a [`DomainRules`] value
//! rather than a second copy of four hundred lines that would drift within a
//! month.
//!
//! ## What the score is made of, and why each part is there
//!
//! * **Family.** The strongest signal, and the one that makes a session
//!   useful at all.
//! * **Language.** A bonus rather than a requirement: a good mentor in a
//!   neighbouring language beats a mediocre one in the same.
//! * **Distance.** A mentor should be well ahead, not adjacent — somebody a
//!   hundred points above you has nothing to teach yet — and not so far ahead
//!   that the conversation has no common ground.
//! * **Timezone.** Three hours is roughly the widest window where two people
//!   can find an hour that is not the middle of somebody's night.
//! * **Load.** A mentor already carrying five people is not available,
//!   whatever their calendar says.
//!
//! Every part is returned with the match, because a mentee who can see why
//! somebody was suggested can tell us it was wrong.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How far ahead a mentor should be. Below this the conversation is between
/// peers, which is valuable and is not mentorship.
pub const MIN_SCORE_GAP: i32 = 500;

/// Beyond three hours, "let us find an hour" means somebody's midnight.
pub const MAX_TIMEZONE_GAP_HOURS: i32 = 3;

/// What differs between domains, and nothing else does.
#[derive(Debug, Clone, Copy)]
pub struct DomainRules {
    /// The `skill_domain` whose craft score and profile answers are read.
    pub domain: &'static str,
    /// The key in `user_domain_profiles.answers` holding what somebody works
    /// with — languages for code, frameworks for AI. Both are arrays, because
    /// nobody uses exactly one.
    pub tools_key: &'static str,
    /// The key holding what they picked to work on. `preferred_families` for
    /// code and AI, whose wizards ask for review families; `trades` for ops,
    /// whose wizard asks for the job. Same role, different word, and using
    /// one word for both would make one of the two wizards read wrong.
    pub families_key: &'static str,
    /// Past this, a mentor has nothing left to give the next person.
    pub max_active_mentees: i64,
    /// What the tools are called when the reasoning is written out. "Partage
    /// tes langages" reads wrong to somebody who answered PyTorch.
    pub tools_label: &'static str,
    /// Whether the wizard stores trade slugs where the mentor side stores
    /// reviewer families. Design's wizard asks which trades interest you —
    /// `design-brand-identity` — while a mentor is known by the family behind
    /// it, `brand`. Where this is set the answers are resolved through
    /// `orientations.reviewer_group` before anything is compared; where it is
    /// not, the two sides already speak the same words.
    pub families_are_trade_slugs: bool,
}

/// Five, because this is the most populated domain and a lower cap would
/// leave most people unmatched.
pub const CODE: DomainRules = DomainRules {
    domain: "code",
    tools_key: "main_tools",
    families_key: "preferred_families",
    max_active_mentees: 5,
    tools_label: "langages",
    families_are_trade_slugs: false,
};

/// Three, because the domain is smaller and a session is longer: reading
/// somebody's training run is not reviewing a pull request.
pub const AI: DomainRules = DomainRules {
    domain: "ai",
    tools_key: "main_frameworks",
    families_key: "preferred_families",
    max_active_mentees: 3,
    tools_label: "outils",
    families_are_trade_slugs: false,
};

/// Four, lower than code's five: an ops mentee often arrives with a system
/// that is on fire, and a mentor carrying four of those is carrying four
/// emergencies rather than four conversations.
pub const OPS: DomainRules = DomainRules {
    domain: "ops",
    tools_key: "cloud_experience",
    families_key: "trades",
    max_active_mentees: 4,
    tools_label: "plateformes",
    families_are_trade_slugs: false,
};

/// Three, like AI and for a related reason: the domain is small and the
/// session is long. Listening to somebody's mix and saying something useful
/// about it is not a fifteen-minute pass over a diff.
pub const AUDIO: DomainRules = DomainRules {
    domain: "audio",
    tools_key: "main_daws",
    families_key: "preferred_families",
    max_active_mentees: 3,
    tools_label: "stations",
    families_are_trade_slugs: false,
};

/// Three, like AI. A design session is a critique over an artefact, which is
/// slower and more attentive than reading a diff; a designer carrying five is
/// carrying them badly.
///
/// `main_tool` is the one answer here that is a string rather than an array —
/// the wizard asks which tool you work in, singular — and the query below
/// reads either shape.
pub const DESIGN: DomainRules = DomainRules {
    domain: "design",
    tools_key: "main_tool",
    families_key: "preferred_families",
    max_active_mentees: 3,
    tools_label: "outils",
    families_are_trade_slugs: true,
};

/// Four, which is the code number rather than the design one, and for the
/// reason that separates them: a quality session is usually reading a report
/// or a suite and saying what is missing, which is closer to reading a diff
/// than to critiquing an artefact.
///
/// `quality_tools` is open text, so the overlap it produces is a bonus and
/// never a filter — somebody who lists "axe" and somebody who lists "axe
/// DevTools" are the same answer, and a filter would separate them.
pub const QUALITY: DomainRules = DomainRules {
    domain: "quality",
    tools_key: "quality_tools",
    families_key: "preferred_families",
    max_active_mentees: 4,
    tools_label: "outils",
    families_are_trade_slugs: false,
};

/// Three, like design and for the same reason: a leadership session is
/// reading somebody's document and saying what is missing from it, which is
/// slower and more attentive than reading a diff. A mentor carrying five is
/// carrying them badly.
pub const LEADERSHIP: DomainRules = DomainRules {
    domain: "leadership",
    tools_key: "leadership_tools",
    families_key: "preferred_families",
    max_active_mentees: 3,
    tools_label: "outils",
    families_are_trade_slugs: false,
};

/// Four. A communication mentee usually arrives with a draft rather than an
/// emergency, and reading a draft properly is an hour — slower than a diff,
/// faster than listening to a mix twice.
pub const COMMUNICATION: DomainRules = DomainRules {
    domain: "communication",
    tools_key: "main_formats",
    families_key: "preferred_families",
    max_active_mentees: 4,
    tools_label: "formats",
    families_are_trade_slugs: false,
};

/// Three. An education session is a conversation about somebody else's
/// learners — what went wrong in a cohort, why a lesson lost the room — and it
/// does not compress. Three is what a working trainer can carry.
pub const EDUCATION: DomainRules = DomainRules {
    domain: "education",
    tools_key: "main_settings",
    families_key: "preferred_families",
    max_active_mentees: 3,
    tools_label: "cadres",
    families_are_trade_slugs: false,
};

/// The rules for a domain named at runtime.
///
/// The wizard validates its answers against the same distinction the matcher
/// reads them with, so a family the wizard accepts is one the matcher can use.
/// Two lists would drift, and the drift is silent: an answer stored and never
/// matched looks like an empty platform.
pub fn rules_for(domain: &str) -> Option<DomainRules> {
    match domain {
        "code" => Some(CODE),
        "ai" => Some(AI),
        "ops" => Some(OPS),
        "audio" => Some(AUDIO),
        "design" => Some(DESIGN),
        "quality" => Some(QUALITY),
        "leadership" => Some(LEADERSHIP),
        "communication" => Some(COMMUNICATION),
        "education" => Some(EDUCATION),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Match {
    pub mentor_user_id: Uuid,
    pub username: String,
    pub headline: String,
    pub craft_score: i32,
    pub score: i32,
    /// Which families you have in common.
    pub shared_families: Vec<String>,
    /// Languages for code, frameworks for AI.
    pub shared_tools: Vec<String>,
    pub timezone_gap_hours: Option<i32>,
    pub active_mentees: i64,
    /// The reasoning, in sentences. A mentee who can read why somebody was
    /// suggested can tell us it was wrong.
    pub because: Vec<String>,
}

/// What the person looking for a mentor said about themselves.
#[derive(sqlx::FromRow)]
struct Mentee {
    craft_score: i32,
    preferred_families: Vec<String>,
    tools: Vec<String>,
    timezone: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    user_id: Uuid,
    username: String,
    headline: String,
    craft_score: i32,
    families: Vec<String>,
    tools: Vec<String>,
    timezone: Option<String>,
    active_mentees: i64,
}

/// Hours east of UTC, from an offset string like `+02:00` or `-05:00`.
///
/// Deliberately narrow: this reads what the onboarding stores, and anything
/// else — an IANA name, a city — answers `None` rather than a guess. A wrong
/// offset would suggest mentors in the wrong half of the planet, which is
/// worse than suggesting nobody.
pub fn utc_offset_hours(timezone: &str) -> Option<i32> {
    let trimmed = timezone.trim();
    let (sign, rest) = match trimmed.strip_prefix('+') {
        Some(rest) => (1, rest),
        None => (-1, trimmed.strip_prefix('-')?),
    };
    let hours: i32 = rest.split(':').next()?.parse().ok()?;
    (0..=14).contains(&hours).then_some(sign * hours)
}

/// How far apart two offsets are, in hours.
pub fn timezone_gap(a: Option<&str>, b: Option<&str>) -> Option<i32> {
    let a = utc_offset_hours(a?)?;
    let b = utc_offset_hours(b?)?;
    Some((a - b).abs())
}

/// Score one candidate against one mentee.
///
/// Pure and tested: this is where a mistake is silent, because a wrong
/// ordering still looks like a list of plausible people.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate(
    shared_families: usize,
    shared_tools: usize,
    score_gap: i32,
    timezone_gap: Option<i32>,
    active_mentees: i64,
    max_active_mentees: i64,
) -> i32 {
    // Nothing in common in the family is not a match at any price.
    if shared_families == 0 {
        return 0;
    }
    // Somebody adjacent to you has nothing to teach yet.
    if score_gap < MIN_SCORE_GAP {
        return 0;
    }
    // Already carrying as many as they can.
    if active_mentees >= max_active_mentees {
        return 0;
    }

    let mut score = 100 * shared_families as i32;
    score += 40 * shared_tools as i32;

    // The gap helps, with diminishing returns: three thousand points ahead is
    // not six times better than five hundred, and past a point the
    // conversation loses its common ground.
    score += ((score_gap as f64 / 100.0).sqrt() * 30.0).round() as i32;

    match timezone_gap {
        Some(gap) if gap <= MAX_TIMEZONE_GAP_HOURS => score += 60 - 15 * gap,
        // Further than that, still possible and much harder to schedule.
        Some(_) => score -= 40,
        // Unknown. Neither rewarded nor punished — most people have not
        // filled it in, and punishing them would hide good mentors.
        None => {}
    }

    // Room left. A mentor with nobody has more attention to give.
    score += 10 * (max_active_mentees - active_mentees) as i32;

    score.max(0)
}

/// Mentors worth suggesting to this person, best first.
pub async fn matches_for(
    db: &PgPool,
    rules: DomainRules,
    mentee_id: Uuid,
    limit: i64,
) -> Result<Vec<Match>, AppError> {
    // A mentee with no computed score reads as zero here, which is the right
    // answer: the gap to a mentor is then the mentor's whole score, and
    // somebody with nothing proved has the most to learn.
    let mentee: Option<Mentee> = sqlx::query_as(
        // The answers live in `user_domain_profiles` since migration 0306.
        // COALESCE to an empty array rather than NULL: somebody who never
        // answered the wizard has no families, which the check below turns
        // into a message telling them to answer it.
        // `answer_texts` reads an answer that may be an array of strings or a
        // single string: design's wizard asks for one tool, the others ask
        // for several, and a `jsonb_array_elements_text` over a bare string
        // errors rather than returning nothing.
        "SELECT COALESCE(cs.score, 0) AS craft_score,
                answer_texts(p.answers, $4) AS preferred_families,
                answer_texts(p.answers, $3) AS tools,
                u.timezone
           FROM users u
           LEFT JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $2
           LEFT JOIN user_domain_profiles p
                  ON p.user_id = u.id AND p.domain = $2
          WHERE u.id = $1",
    )
    .bind(mentee_id)
    .bind(rules.domain)
    .bind(rules.tools_key)
    .bind(rules.families_key)
    .fetch_optional(db)
    .await?;

    let Mentee {
        craft_score: mentee_score,
        preferred_families: mentee_families,
        tools: mentee_tools,
        timezone: mentee_timezone,
    } = mentee.ok_or_else(|| AppError::NotFound("user not found".into()))?;

    if mentee_families.is_empty() {
        return Err(AppError::Validation(format!(
            "réponds d'abord au questionnaire {} : sans famille, il n'y a rien sur quoi faire correspondre",
            rules.domain
        )));
    }

    // A mentee who named trades is compared on the families behind them: the
    // mentor side is keyed by family, and `design-brand-identity` matches
    // `brand` nowhere without this step. Resolved in SQL rather than from a
    // table in Rust, so a twenty-seventh trade needs no code change.
    let mentee_families = if rules.families_are_trade_slugs {
        let resolved: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT reviewer_group FROM orientations
              WHERE slug = ANY($1) AND reviewer_group IS NOT NULL",
        )
        .bind(&mentee_families)
        .fetch_all(db)
        .await?;
        if resolved.is_empty() {
            return Err(AppError::Validation(format!(
                "réponds d'abord au questionnaire {} : sans famille, il n'y a rien sur quoi faire correspondre",
                rules.domain
            )));
        }
        resolved
    } else {
        mentee_families
    };

    let candidates = sqlx::query_as::<_, Candidate>(
        r#"
        SELECT u.id AS user_id,
               u.username,
               m.headline,
               cs.score AS craft_score,
               -- Proven, not declared, and with no fallback.
               --
               -- The families of the trades this person has verified work in.
               -- Not the ones they told the wizard interest them: a mentor who
               -- said they were interested in motion and never delivered any is
               -- not a motion mentor, and an hour with them teaches that the
               -- expensive way, to somebody who paid for the hour.
               --
               -- This was briefly softened to fall back on the declared answer
               -- for anybody with no verified deliverable at all, on the
               -- reasoning that requiring proof empties the list on a young
               -- platform. That reasoning was wrong, and design's own test says
               -- why: the person it lets through is one with a *high* score who
               -- declares a family they have never worked in, which is exactly
               -- the case the rule exists for. The list being short is the
               -- honest answer when few people have delivered.
               --
               -- The mentee side stays declared, deliberately: somebody looking
               -- for a mentor is looking towards where they want to go, not
               -- where they have already been.
               ARRAY(
                   SELECT DISTINCT o.reviewer_group
                     FROM deliverables d
                     JOIN project_slices ps ON ps.id = d.slice_id
                     JOIN orientations o ON o.id = ps.orientation_id
                    WHERE d.user_id = u.id
                      AND d.verification_status = 'verified'
                      AND d.revoked_at IS NULL
                      AND o.primary_domain = $3
                      AND o.reviewer_group IS NOT NULL
               ) AS families,
               -- What the mentor works in: their declared tools, and the
               -- expertise they wrote on their mentor profile. Both, because
               -- somebody who filled in one rarely filled in the other.
               ARRAY(
                   SELECT DISTINCT lower(l)
                     FROM unnest(answer_texts(p.answers, $4) || m.expertise_areas) AS l
               ) AS tools,
               u.timezone,
               (SELECT count(DISTINCT s.mentee_user_id)
                  FROM mentorship_sessions s
                 WHERE s.mentor_user_id = u.id
                   -- Booked and not yet over. `completed` is deliberately
                   -- absent: a session that happened is history, not a
                   -- mentee somebody is still carrying.
                   AND s.status IN ('paid', 'confirmed')
                   AND s.scheduled_at > NOW() - INTERVAL '60 days')
                   AS active_mentees
          FROM mentor_profiles m
          JOIN users u ON u.id = m.user_id
          -- An inner join: a mentor with no computed score has proved
          -- nothing in this domain, and suggesting them would be suggesting
          -- somebody on the strength of having a mentor profile.
          JOIN craft_scores cs ON cs.user_id = u.id AND cs.skill_domain = $3
          LEFT JOIN user_domain_profiles p
                 ON p.user_id = u.id AND p.domain = $3
         WHERE m.active = TRUE
           AND u.is_banned = FALSE
           AND u.id <> $1
           AND cs.score >= $2
        "#,
    )
    .bind(mentee_id)
    .bind(mentee_score + MIN_SCORE_GAP)
    .bind(rules.domain)
    .bind(rules.tools_key)
    .fetch_all(db)
    .await?;

    let mentee_tools: Vec<String> = mentee_tools.iter().map(|l| l.to_lowercase()).collect();

    let mut matches: Vec<Match> = candidates
        .into_iter()
        .map(|c| {
            let shared_families: Vec<String> = c
                .families
                .iter()
                .filter(|f| mentee_families.contains(f))
                .cloned()
                .collect();
            let shared_tools: Vec<String> = c
                .tools
                .iter()
                .filter(|l| mentee_tools.contains(l))
                .cloned()
                .collect();
            let gap = timezone_gap(mentee_timezone.as_deref(), c.timezone.as_deref());
            let score_gap = c.craft_score - mentee_score;

            let score = score_candidate(
                shared_families.len(),
                shared_tools.len(),
                score_gap,
                gap,
                c.active_mentees,
                rules.max_active_mentees,
            );

            let mut because = Vec::new();
            if !shared_families.is_empty() {
                because.push(format!(
                    "Travaille dans {} — la même famille que toi.",
                    shared_families.join(", ")
                ));
            }
            if !shared_tools.is_empty() {
                because.push(format!(
                    "Partage tes {} : {}.",
                    rules.tools_label,
                    shared_tools.join(", ")
                ));
            }
            because.push(format!(
                "{score_gap} points de craft score d'avance : assez pour t'apprendre quelque \
                 chose, pas assez pour parler une autre langue."
            ));
            match gap {
                Some(0) => because.push("Même fuseau horaire.".into()),
                Some(g) if g <= MAX_TIMEZONE_GAP_HOURS => {
                    because.push(format!("{g} h de décalage : trouvable."))
                }
                Some(g) => because.push(format!("{g} h de décalage : à organiser.")),
                None => {}
            }
            if c.active_mentees == 0 {
                because.push("N'accompagne personne en ce moment.".into());
            }

            Match {
                mentor_user_id: c.user_id,
                username: c.username,
                headline: c.headline,
                craft_score: c.craft_score,
                score,
                shared_families,
                shared_tools,
                timezone_gap_hours: gap,
                active_mentees: c.active_mentees,
                because,
            }
        })
        // A zero is a refusal, not a weak suggestion: showing a kernel
        // engineer to somebody learning React wastes an hour of both.
        .filter(|m| m.score > 0)
        .collect();

    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.active_mentees.cmp(&b.active_mentees))
    });
    matches.truncate(limit.clamp(1, 50) as usize);
    Ok(matches)
}

/// Whether somebody looks stuck enough that suggesting a mentor is worth it.
///
/// Three pieces of work handed in for review in this domain and none of them
/// validated. Not a judgement about the person — it is the shape of somebody
/// repeating a mistake nobody has named for them yet, which is the one thing
/// a mentor fixes faster than another attempt does.
///
/// Returned rather than pushed. Telling somebody "you seem to be struggling"
/// unprompted lands badly however it is worded; this way the suggestion sits
/// beside their work because they opened the page.
///
/// The slice types are read from `slice_types` rather than named here, so a
/// domain that gains a surface gains it in this signal too. Types with no
/// domain — a repository issue, a piece of documentation — belong to every
/// domain and are deliberately excluded: failing to get a doc fix merged says
/// nothing about somebody's design.
pub async fn could_use_a_mentor(
    db: &PgPool,
    domain: &str,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let (handed_in, validated): (i64, i64) = sqlx::query_as(
        r#"
        SELECT count(DISTINCT d.slice_id) FILTER (WHERE TRUE),
               count(DISTINCT d.slice_id) FILTER (WHERE s.status = 'validated')
          FROM slice_validation_decisions d
          JOIN project_slices s ON s.id = d.slice_id
          JOIN slice_types t ON t.slug = s.slice_type
         WHERE s.claimed_by_user_id = $1
           AND t.skill_domain = $2
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_one(db)
    .await?;

    Ok(handed_in >= 3 && validated == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_is_read_and_anything_else_is_not_guessed() {
        assert_eq!(utc_offset_hours("+01:00"), Some(1));
        assert_eq!(utc_offset_hours("-05:00"), Some(-5));
        assert_eq!(utc_offset_hours("+00:00"), Some(0));
        // A wrong offset suggests mentors on the wrong half of the planet,
        // which is worse than suggesting nobody.
        assert_eq!(utc_offset_hours("Africa/Porto-Novo"), None);
        assert_eq!(utc_offset_hours("CET"), None);
        assert_eq!(utc_offset_hours("+99:00"), None);
    }

    #[test]
    fn a_gap_needs_both_ends() {
        assert_eq!(timezone_gap(Some("+01:00"), Some("-02:00")), Some(3));
        assert_eq!(timezone_gap(Some("+01:00"), None), None);
        assert_eq!(timezone_gap(None, None), None);
    }

    #[test]
    fn no_shared_family_is_not_a_match_at_any_price() {
        assert_eq!(
            score_candidate(0, 3, 5000, Some(0), 0, CODE.max_active_mentees),
            0
        );
    }

    #[test]
    fn somebody_adjacent_has_nothing_to_teach_yet() {
        assert_eq!(
            score_candidate(1, 1, 100, Some(0), 0, CODE.max_active_mentees),
            0
        );
        assert!(score_candidate(1, 1, MIN_SCORE_GAP, Some(0), 0, CODE.max_active_mentees) > 0);
    }

    #[test]
    fn a_full_mentor_is_not_available_whatever_their_calendar_says() {
        assert_eq!(
            score_candidate(
                2,
                2,
                2000,
                Some(0),
                CODE.max_active_mentees,
                CODE.max_active_mentees
            ),
            0
        );
        assert!(
            score_candidate(
                2,
                2,
                2000,
                Some(0),
                CODE.max_active_mentees - 1,
                CODE.max_active_mentees
            ) > 0
        );
    }

    #[test]
    fn a_shared_language_helps_and_does_not_decide() {
        let same_language_one_family =
            score_candidate(1, 1, 1000, Some(0), 0, CODE.max_active_mentees);
        let two_families_no_language =
            score_candidate(2, 0, 1000, Some(0), 0, CODE.max_active_mentees);
        assert!(
            two_families_no_language > same_language_one_family,
            "a good mentor in a neighbouring language beats a mediocre one in the same"
        );
    }

    #[test]
    fn the_gap_has_diminishing_returns() {
        let modest = score_candidate(1, 0, 500, None, 0, CODE.max_active_mentees);
        let large = score_candidate(1, 0, 5000, None, 0, CODE.max_active_mentees);
        let enormous = score_candidate(1, 0, 50_000, None, 0, CODE.max_active_mentees);
        assert!(modest < large && large < enormous);
        // Ten times the gap is not ten times the score.
        assert!(large - modest < (modest - 100) * 10);
    }

    #[test]
    fn an_unknown_timezone_is_neither_rewarded_nor_punished() {
        let unknown = score_candidate(1, 1, 1000, None, 0, CODE.max_active_mentees);
        let close = score_candidate(1, 1, 1000, Some(1), 0, CODE.max_active_mentees);
        let far = score_candidate(1, 1, 1000, Some(9), 0, CODE.max_active_mentees);
        assert!(close > unknown, "a known close timezone should win");
        assert!(unknown > far, "a known far timezone should lose");
    }

    #[test]
    fn the_two_domains_differ_only_where_they_should() {
        // If a third domain is added and this fails, the thing to change is
        // the constant — not to copy the module.
        assert_ne!(CODE.domain, AI.domain);
        assert_ne!(CODE.tools_key, AI.tools_key);
        // AI carries fewer mentees per mentor: reading somebody's training
        // run is not reviewing a pull request. Ops fewer still, because an
        // ops mentee often arrives with a system that is on fire.
        let (ai, code, ops) = (
            AI.max_active_mentees,
            CODE.max_active_mentees,
            OPS.max_active_mentees,
        );
        assert!(ai < code);
        assert!(ops < code);
    }

    #[test]
    fn a_full_ai_mentor_is_full_earlier_than_a_code_one() {
        let load = AI.max_active_mentees;
        assert_eq!(
            score_candidate(2, 2, 2000, Some(0), load, AI.max_active_mentees),
            0
        );
        assert!(score_candidate(2, 2, 2000, Some(0), load, CODE.max_active_mentees) > 0);
    }
}
