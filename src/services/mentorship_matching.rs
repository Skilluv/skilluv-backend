//! Deciding whether two people are worth an hour of each other's time.
//!
//! ## Why this is not in one domain's module
//!
//! `code_mentorship` scored a candidate on five things: family, shared
//! vocabulary, distance in craft score, timezone gap, and how many people the
//! mentor is already carrying. The design backlog asked for the same five,
//! and cyber will ask for them next.
//!
//! Copied, they would diverge — and a divergence here is silent. A wrong
//! ordering still looks like a list of plausible people, so nobody would
//! notice for months that one domain had stopped counting the timezone.
//!
//! What genuinely differs between domains is **where a person's families come
//! from**, and that stays in each domain's module. Code reads columns on
//! `users`; design reads the onboarding answers and, for mentors, the trades
//! they have actually been validated in. Those are different questions with
//! different right answers, and pretending otherwise would have been the
//! wrong generalisation.

use serde::Serialize;
use uuid::Uuid;

/// The thresholds a domain sets. Everything else about the score is shared.
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    /// How far ahead a mentor should be. Below this the conversation is
    /// between peers, which is valuable and is not mentorship.
    pub min_score_gap: i32,
    /// Beyond this, "let us find an hour" means somebody's midnight.
    pub max_timezone_gap_hours: i32,
    /// Past this many people, the next one gets what is left over.
    pub max_active_mentees: i64,
}

impl Weights {
    /// What a domain uses unless it says otherwise.
    ///
    /// Three hours is roughly the widest window where two people can find an
    /// hour that is not the middle of somebody's night — and it is the one
    /// number that should not vary by domain, because the planet does not.
    pub const fn default_for_domain() -> Self {
        Self {
            min_score_gap: 500,
            max_timezone_gap_hours: 3,
            max_active_mentees: 3,
        }
    }
}

/// One suggestion, with the reasoning attached.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Match {
    pub mentor_user_id: Uuid,
    pub username: String,
    pub headline: String,
    /// The mentor's craft score in the domain being matched on.
    pub craft_score: i32,
    pub score: i32,
    /// Which families you have in common.
    pub shared_families: Vec<String>,
    /// Shared vocabulary — languages for code, tools for design.
    pub shared_vocabulary: Vec<String>,
    pub timezone_gap_hours: Option<i32>,
    pub active_mentees: i64,
    /// The reasoning, in sentences. Somebody who can read why a mentor was
    /// suggested can tell us it was wrong.
    pub because: Vec<String>,
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
/// Pure and tested, because this is where a mistake is silent: a wrong
/// ordering still looks like a list of plausible people.
///
/// Zero means "not a match", not "a weak match". Showing a type designer to
/// somebody learning motion wastes an hour of both.
pub fn score_candidate(
    shared_families: usize,
    shared_vocabulary: usize,
    score_gap: i32,
    timezone_gap: Option<i32>,
    active_mentees: i64,
    weights: Weights,
) -> i32 {
    // Nothing in common in the family is not a match at any price.
    if shared_families == 0 {
        return 0;
    }
    // Somebody adjacent to you has nothing to teach yet.
    if score_gap < weights.min_score_gap {
        return 0;
    }
    // Already carrying as many as they can.
    if active_mentees >= weights.max_active_mentees {
        return 0;
    }

    let mut score = 100 * shared_families as i32;
    score += 40 * shared_vocabulary as i32;

    // The gap helps, with diminishing returns: three thousand points ahead is
    // not six times better than five hundred, and past a point the
    // conversation loses its common ground.
    score += ((score_gap as f64 / 100.0).sqrt() * 30.0).round() as i32;

    match timezone_gap {
        Some(gap) if gap <= weights.max_timezone_gap_hours => score += 60 - 15 * gap,
        // Further than that, still possible and much harder to schedule.
        Some(_) => score -= 40,
        // Unknown. Neither rewarded nor punished — most people have not
        // filled it in, and punishing them would hide good mentors.
        None => {}
    }

    // Room left. A mentor with nobody has more attention to give.
    score += 10 * (weights.max_active_mentees - active_mentees) as i32;

    score.max(0)
}

/// The sentences that go with a score.
///
/// Built here so every domain says the same things in the same order — a
/// mentee comparing two suggestions should not have to work out that "même
/// fuseau" and "0 h de décalage" mean the same thing.
pub fn reasons(
    shared_families: &[String],
    shared_vocabulary: &[String],
    vocabulary_noun: &str,
    score_gap: i32,
    timezone_gap: Option<i32>,
    active_mentees: i64,
    weights: Weights,
) -> Vec<String> {
    let mut because = Vec::new();

    if !shared_families.is_empty() {
        because.push(format!(
            "Travaille dans {} — la même famille que toi.",
            shared_families.join(", ")
        ));
    }
    if !shared_vocabulary.is_empty() {
        because.push(format!(
            "Partage {vocabulary_noun} : {}.",
            shared_vocabulary.join(", ")
        ));
    }
    because.push(format!(
        "{score_gap} points de craft score d'avance : assez pour t'apprendre quelque \
         chose, pas assez pour parler une autre langue."
    ));
    match timezone_gap {
        Some(0) => because.push("Même fuseau horaire.".into()),
        Some(g) if g <= weights.max_timezone_gap_hours => {
            because.push(format!("{g} h de décalage : trouvable."))
        }
        Some(g) => because.push(format!("{g} h de décalage : à organiser.")),
        None => {}
    }
    if active_mentees == 0 {
        because.push("N'accompagne personne en ce moment.".into());
    }

    because
}

/// Best first, and between two equals the one carrying fewer people.
pub fn rank(matches: &mut Vec<Match>, limit: i64) {
    matches.retain(|m| m.score > 0);
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.active_mentees.cmp(&b.active_mentees))
    });
    matches.truncate(limit.clamp(1, 50) as usize);
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Weights = Weights::default_for_domain();

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
    fn the_three_refusals_are_absolute() {
        // No shared family.
        assert_eq!(score_candidate(0, 5, 5000, Some(0), 0, W), 0);
        // Adjacent rather than ahead.
        assert_eq!(score_candidate(2, 5, 499, Some(0), 0, W), 0);
        // Already full.
        assert_eq!(
            score_candidate(2, 5, 5000, Some(0), W.max_active_mentees, W),
            0
        );
    }

    #[test]
    fn an_unknown_timezone_is_neither_rewarded_nor_punished() {
        // Most people have not filled it in, and punishing them would hide
        // good mentors.
        let unknown = score_candidate(1, 0, 1000, None, 0, W);
        let far = score_candidate(1, 0, 1000, Some(9), 0, W);
        let near = score_candidate(1, 0, 1000, Some(0), 0, W);
        assert!(far < unknown, "{far} < {unknown}");
        assert!(unknown < near, "{unknown} < {near}");
    }

    #[test]
    fn the_gap_helps_with_diminishing_returns() {
        // Compared over equal widths, which is what "diminishing" means: the
        // same five hundred points are worth less the further ahead the
        // mentor already is. An earlier version of this test compared 500 to
        // 3000 against 3000 to 30000 and failed, because the second interval
        // is ten times wider — the returns diminish per point, not per pair.
        let at = |gap| score_candidate(1, 0, gap, Some(0), 0, W);
        let early = at(1_000) - at(500);
        let late = at(3_000) - at(2_500);
        assert!(early > 0 && late > 0, "{early} {late}");
        assert!(late < early, "the same 500 points should be worth less: {late} < {early}");
    }

    #[test]
    fn a_mentor_with_room_is_preferred_to_an_identical_one_without() {
        let free = score_candidate(1, 1, 1000, Some(1), 0, W);
        let busy = score_candidate(1, 1, 1000, Some(1), W.max_active_mentees - 1, W);
        assert!(free > busy);
    }

    #[test]
    fn every_domain_says_the_same_things_in_the_same_order() {
        // A mentee comparing two suggestions should not have to work out that
        // "même fuseau" and "0 h de décalage" mean the same thing.
        let families = vec!["brand".to_string()];
        let vocabulary = vec!["figma".to_string()];
        let said = reasons(&families, &vocabulary, "tes outils", 900, Some(0), 0, W);
        assert!(said[0].contains("brand"));
        assert!(said[1].contains("figma"));
        assert!(said[2].contains("900"));
        assert!(said[3].contains("Même fuseau"));
        assert!(said[4].contains("N'accompagne personne"));
    }

    #[test]
    fn a_zero_never_reaches_a_ranking() {
        let mut matches = vec![
            Match {
                mentor_user_id: Uuid::nil(),
                username: "refusé".into(),
                headline: String::new(),
                craft_score: 0,
                score: 0,
                shared_families: vec![],
                shared_vocabulary: vec![],
                timezone_gap_hours: None,
                active_mentees: 0,
                because: vec![],
            },
            Match {
                mentor_user_id: Uuid::nil(),
                username: "retenu".into(),
                headline: String::new(),
                craft_score: 900,
                score: 120,
                shared_families: vec!["brand".into()],
                shared_vocabulary: vec![],
                timezone_gap_hours: None,
                active_mentees: 1,
                because: vec![],
            },
        ];
        rank(&mut matches, 10);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].username, "retenu");
    }
}
