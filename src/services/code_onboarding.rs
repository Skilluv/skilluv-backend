//! The code onboarding wizard, and what it recommends.
//!
//! Seven questions. The answers are worth nothing on their own — the point is
//! the first month that follows them, and with thirty-three trades the
//! platform cannot guess it.
//!
//! ## The recommendation is rules, not a model
//!
//! Written as explicit combinations because the reasoning has to be
//! defensible to the person receiving it: "you said beginner, web and five
//! hours a week, so here is the web guide and the easiest open issues in
//! TypeScript". A score nobody can explain would be a worse version of the
//! same answer.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const LEVELS: &[&str] = &["beginner", "junior", "mid", "senior", "staff"];
pub const WEEKLY_HOURS: &[&str] = &["under_5", "5_to_15", "15_to_40", "fulltime"];
pub const OBJECTIVES: &[&str] = &[
    "learn",
    "build_portfolio",
    "find_paid_work",
    "contribute_upstream",
    "publish_library",
    "become_mentor",
    "ship_own_product",
];
pub const CHALLENGE_PREFERENCES: &[&str] = &[
    "upstream_contributions",
    "solo_shipped_apps",
    "published_libraries",
    "long_team_projects",
    "short_hackathons",
];

/// At most three of each. Somebody who selects everything has told us
/// nothing while believing they answered.
pub const MAX_SELECTIONS: usize = 3;

#[derive(Debug, Clone, Deserialize)]
pub struct WizardAnswers {
    pub level: String,
    /// Reviewer groups — the same eight families the guides use.
    pub preferred_families: Vec<String>,
    pub weekly_hours: String,
    pub objective: String,
    pub main_languages: Vec<String>,
    pub challenge_preference: String,
    /// Optional: triggers a portfolio import if given.
    #[serde(default)]
    pub github_username: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    /// Said in one sentence, in the second person. What the person reads
    /// first.
    pub headline: String,
    /// Why this and not something else. The reasoning, so it can be argued
    /// with.
    pub because: String,
    /// Guides to open, by slug.
    pub guides: Vec<String>,
    /// Where to look for work: a ready-made query against the first-issues
    /// feed.
    pub first_issues_query: String,
    /// What to aim at in the first month.
    pub next_steps: Vec<String>,
}

/// Check the answers against what the columns allow, with messages somebody
/// can act on rather than a constraint name.
pub fn validate(answers: &WizardAnswers) -> Result<(), AppError> {
    if !LEVELS.contains(&answers.level.as_str()) {
        return Err(AppError::Validation(format!(
            "level must be one of: {}",
            LEVELS.join(", ")
        )));
    }
    if !WEEKLY_HOURS.contains(&answers.weekly_hours.as_str()) {
        return Err(AppError::Validation(format!(
            "weekly_hours must be one of: {}",
            WEEKLY_HOURS.join(", ")
        )));
    }
    if !OBJECTIVES.contains(&answers.objective.as_str()) {
        return Err(AppError::Validation(format!(
            "objective must be one of: {}",
            OBJECTIVES.join(", ")
        )));
    }
    if !CHALLENGE_PREFERENCES.contains(&answers.challenge_preference.as_str()) {
        return Err(AppError::Validation(format!(
            "challenge_preference must be one of: {}",
            CHALLENGE_PREFERENCES.join(", ")
        )));
    }
    if answers.preferred_families.is_empty() {
        return Err(AppError::Validation(
            "pick at least one family — the whole point of the wizard is the narrowing".into(),
        ));
    }
    if answers.preferred_families.len() > MAX_SELECTIONS {
        return Err(AppError::Validation(format!(
            "pick at most {MAX_SELECTIONS} families"
        )));
    }
    if answers.main_languages.len() > MAX_SELECTIONS {
        return Err(AppError::Validation(format!(
            "pick at most {MAX_SELECTIONS} languages"
        )));
    }
    Ok(())
}

/// What to do first, given what somebody said.
///
/// Deliberately opinionated and deliberately short. A first month with nine
/// items is a first month nobody starts.
pub fn recommend(answers: &WizardAnswers) -> Recommendation {
    let family = answers
        .preferred_families
        .first()
        .cloned()
        .unwrap_or_else(|| "web".into());
    let language = answers.main_languages.first().cloned();

    // A language narrows the feed usefully; a family does not, because the
    // feed filters on trades and a family is eight of them.
    let query = match &language {
        Some(language) => format!("/api/code/first-issues?language={language}&max_difficulty=3"),
        None => "/api/code/first-issues?max_difficulty=3".to_string(),
    };

    // The three answers that actually change the advice: how experienced,
    // how much time, and what for. The rest tunes it.
    let experienced = matches!(answers.level.as_str(), "senior" | "staff");
    let scarce_time = answers.weekly_hours == "under_5";

    let (headline, because, next_steps) = match (experienced, answers.objective.as_str()) {
        (true, "find_paid_work") => (
            "Va directement aux missions, et prends une contribution en parallèle.".to_string(),
            "Avec ton expérience, le fil des premières issues te fera perdre du temps. Ce qui \
             te manque sur Skilluv n'est pas la compétence, c'est la trace publique."
                .to_string(),
            vec![
                "Ouvre le tableau des missions et filtre sur ton langage.".to_string(),
                "Prends une contribution upstream en parallèle : c'est elle qui produira \
                 l'attestation qu'une entreprise peut vérifier."
                    .to_string(),
                "Connecte ton compte GitHub pour importer ce qui existe déjà.".to_string(),
            ],
        ),
        (true, _) => (
            "Choisis un projet difficile, pas un projet facile.".to_string(),
            "Le fil des premières issues est fait pour apprendre le processus, que tu connais \
             déjà. Ce qui te distinguera est une contribution que peu de gens peuvent faire."
                .to_string(),
            vec![
                "Prends une issue marquée `help wanted` plutôt que `good first issue`.".to_string(),
                "Vise un dépôt du catalogue partenaire : les relations comptent autant que le \
                 code."
                    .to_string(),
                "Envisage de relire le travail des autres — c'est la capability qui ouvre le \
                 plus de portes ici."
                    .to_string(),
            ],
        ),
        (false, "contribute_upstream") | (false, "build_portfolio") => (
            "Une contribution fusionnée avant la fin du mois.".to_string(),
            "C'est l'objectif le plus court qui produise une trace vérifiable, et il est \
             atteignable même en quelques heures par semaine."
                .to_string(),
            vec![
                "Lis le guide de ta famille en entier avant d'écrire une ligne.".to_string(),
                "Prends une issue du fil, la plus facile.".to_string(),
                "Ouvre la pull request même si tu doutes : la revue est un cours particulier \
                 gratuit."
                    .to_string(),
            ],
        ),
        (false, "publish_library") => (
            "Publie quelque chose de petit, tôt.".to_string(),
            "Une bibliothèque publiée, même minuscule, apprend la distribution — qui est la \
             partie que personne n'apprend en écrivant du code."
                .to_string(),
            vec![
                "Automatise une chose que tu fais à la main.".to_string(),
                "Publie-la sur le registre de ton langage, avec un README utilisable.".to_string(),
                "Trouve un utilisateur qui n'est pas toi.".to_string(),
            ],
        ),
        (false, _) => (
            "Trente jours, une contribution.".to_string(),
            "Tu débutes : ce qui compte n'est pas le volume mais d'avoir traversé une fois le \
             cycle complet, revue comprise."
                .to_string(),
            vec![
                "Ouvre le guide de ta famille.".to_string(),
                "Fais tourner les tests d'un projet du catalogue en local.".to_string(),
                "Prends une issue étiquetée pour les débutants.".to_string(),
            ],
        ),
    };

    let mut next_steps = next_steps;
    if scarce_time {
        next_steps.push(
            "Moins de cinq heures par semaine : vise une seule chose à la fois, et une petite. \
             Une contribution finie vaut mieux que trois commencées."
                .to_string(),
        );
    }
    if answers.challenge_preference == "short_hackathons" {
        next_steps.push(
            "Tu préfères les formats courts : surveille les hackathons et le code golf \
             hebdomadaire."
                .to_string(),
        );
    }

    Recommendation {
        headline,
        because,
        guides: vec![format!("onboarding-{family}"), "toolkit-code".to_string()],
        first_issues_query: query,
        next_steps,
    }
}

/// Store the answers and hand back the recommendation.
pub async fn complete(
    db: &PgPool,
    user_id: Uuid,
    answers: &WizardAnswers,
) -> Result<Recommendation, AppError> {
    validate(answers)?;

    // The families must be real reviewer groups: a typo would send somebody
    // to a guide that does not exist and quietly recommend nothing.
    let known: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE reviewer_group IS NOT NULL AND primary_domain = 'code'",
    )
    .fetch_all(db)
    .await?;
    for family in &answers.preferred_families {
        if !known.contains(family) {
            return Err(AppError::Validation(format!(
                "'{family}' is not a code family — expected one of: {}",
                known.join(", ")
            )));
        }
    }

    // One row per person per domain (migration 0306). Replaces the whole
    // answer object rather than merging: the wizard sends every question it
    // asked, and merging would keep an answer the person has just cleared.
    let stored = serde_json::json!({
        "level": answers.level,
        "preferred_families": answers.preferred_families,
        "weekly_hours": answers.weekly_hours,
        "objective": answers.objective,
        "main_languages": answers.main_languages,
        "challenge_preference": answers.challenge_preference,
    });

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
        VALUES ($1, 'code', $2, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE
            SET answers      = EXCLUDED.answers,
                completed_at = NOW(),
                -- Answering is un-skipping. Somebody who said "stop asking"
                -- and then answered has changed their mind, and leaving the
                -- old timestamp would keep the profile reading as skipped.
                skipped_at   = NULL
        "#,
    )
    .bind(user_id)
    .bind(&stored)
    .execute(db)
    .await?;

    Ok(recommend(answers))
}

/// Stop asking.
///
/// Recorded separately from "answered nothing": without this the wizard would
/// reappear forever for exactly the people who least wanted it.
pub async fn skip(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, skipped_at)
        VALUES ($1, 'code', NOW())
        ON CONFLICT (user_id, domain) DO UPDATE SET skipped_at = NOW()
        "#,
    )
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(level: &str, objective: &str, hours: &str) -> WizardAnswers {
        WizardAnswers {
            level: level.into(),
            preferred_families: vec!["web".into()],
            weekly_hours: hours.into(),
            objective: objective.into(),
            main_languages: vec!["typescript".into()],
            challenge_preference: "upstream_contributions".into(),
            github_username: None,
        }
    }

    #[test]
    fn selecting_everything_is_refused() {
        let mut a = answers("beginner", "learn", "5_to_15");
        a.preferred_families = vec![
            "web".into(),
            "mobile".into(),
            "systems".into(),
            "data".into(),
        ];
        assert!(validate(&a).is_err());
    }

    #[test]
    fn selecting_nothing_is_refused_too() {
        let mut a = answers("beginner", "learn", "5_to_15");
        a.preferred_families = vec![];
        assert!(validate(&a).is_err());
    }

    #[test]
    fn a_senior_is_not_sent_to_the_beginner_feed() {
        let senior = recommend(&answers("senior", "find_paid_work", "15_to_40"));
        let beginner = recommend(&answers("beginner", "learn", "15_to_40"));
        assert_ne!(senior.headline, beginner.headline);
        assert!(
            senior.next_steps.iter().any(|s| s.contains("missions")),
            "somebody experienced looking for paid work should be shown the missions"
        );
    }

    #[test]
    fn every_recommendation_says_why() {
        for level in LEVELS {
            for objective in OBJECTIVES {
                let r = recommend(&answers(level, objective, "5_to_15"));
                assert!(
                    !r.because.is_empty(),
                    "{level}/{objective} explains nothing"
                );
                assert!(!r.next_steps.is_empty());
                assert!(r.guides.len() >= 2);
            }
        }
    }

    #[test]
    fn scarce_time_changes_the_advice() {
        let scarce = recommend(&answers("junior", "learn", "under_5"));
        let plenty = recommend(&answers("junior", "learn", "fulltime"));
        assert!(scarce.next_steps.len() > plenty.next_steps.len());
    }

    #[test]
    fn the_guide_named_matches_the_family_chosen() {
        let mut a = answers("junior", "learn", "5_to_15");
        a.preferred_families = vec!["systems".into()];
        assert!(
            recommend(&a)
                .guides
                .contains(&"onboarding-systems".to_string())
        );
    }
}
