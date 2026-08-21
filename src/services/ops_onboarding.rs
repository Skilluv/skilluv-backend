//! Six questions, asked once, and what follows from the answers.
//!
//! ## The one that is not about skill
//!
//! On-call. Somebody with a night job, a shared room or an unreliable
//! connection cannot hold a pager, and none of that says anything about how
//! good they are at the work. Asking up front means two things the platform
//! could not otherwise do: never offering an on-call mission to somebody for
//! whom it would be a trap, and never treating "has never been on call" as
//! "not ready".
//!
//! ## Why the recommendation is computed and not stored
//!
//! It is a function of the answers, and the answers are the columns. Storing
//! the advice would mean a row that disagrees with the wizard the day the
//! advice is improved.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

pub const LEVELS: &[&str] = &["beginner", "junior", "engineer", "senior", "principal"];
pub const WEEKLY_HOURS: &[&str] = &["under_3", "3_to_10", "over_10", "fulltime"];
pub const OBJECTIVES: &[&str] = &[
    "learn",
    "build_portfolio",
    "find_paid_work",
    "become_mentor",
    "start_own_practice",
];
pub const ONCALL_EXPERIENCE: &[&str] = &["never", "occasional", "regular", "always_on"];
pub const CLOUD_PLATFORMS: &[&str] = &["aws", "gcp", "azure", "on_prem", "multi", "none"];

/// Two, not three. Somebody claiming five trades has claimed the domain, and
/// a playlist covering everything covers nothing.
pub const MAX_TRADES: usize = 2;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct WizardAnswers {
    pub level: String,
    /// Trade slugs — `sre`, `cloud-architect`, and so on. Two at most.
    pub trades: Vec<String>,
    pub cloud_experience: Vec<String>,
    pub weekly_hours: String,
    pub objective: String,
    pub oncall_experience: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Recommendation {
    pub headline: String,
    /// Why this and not something else. Advice without a reason is an
    /// instruction, and people do not follow instructions from a form.
    pub because: String,
    /// Guide slugs to open first.
    pub guides: Vec<String>,
    /// Where to practise, given what the answers say about budget and access.
    pub practise_at: String,
    pub next_steps: Vec<String>,
    /// True when on-call work should not be offered yet. Not a judgement:
    /// the platform simply will not put a pager in front of somebody who has
    /// never held one until they have said they want it.
    pub oncall_ready: bool,
}

pub fn validate(answers: &WizardAnswers) -> Result<(), AppError> {
    if !LEVELS.contains(&answers.level.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not a level — expected one of: {}",
            answers.level,
            LEVELS.join(", ")
        )));
    }
    if !WEEKLY_HOURS.contains(&answers.weekly_hours.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not a weekly-hours band",
            answers.weekly_hours
        )));
    }
    if !OBJECTIVES.contains(&answers.objective.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not an objective",
            answers.objective
        )));
    }
    if !ONCALL_EXPERIENCE.contains(&answers.oncall_experience.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not an on-call answer",
            answers.oncall_experience
        )));
    }
    if answers.trades.is_empty() {
        return Err(AppError::Validation(
            "pick at least one trade — the guides and the playlist follow from it".into(),
        ));
    }
    if answers.trades.len() > MAX_TRADES {
        return Err(AppError::Validation(format!(
            "{MAX_TRADES} trades at most: picking more means picking none"
        )));
    }
    for platform in &answers.cloud_experience {
        if !CLOUD_PLATFORMS.contains(&platform.as_str()) {
            return Err(AppError::Validation(format!(
                "'{platform}' is not a platform we ask about"
            )));
        }
    }
    Ok(())
}

/// What to do first, given the answers.
pub fn recommend(answers: &WizardAnswers) -> Recommendation {
    // `recommend` is public and `validate` guarantees at least one trade,
    // but a caller that skipped validation must still get a guide rather
    // than an empty list that reads as "there is nothing for you".
    let guides: Vec<String> = if answers.trades.is_empty() {
        vec!["ops-onboarding-devops".to_string()]
    } else {
        answers
            .trades
            .iter()
            .map(|t| format!("ops-onboarding-{}", short_name(t)))
            .collect()
    };

    // Somebody with no cloud account, or only on-premises experience, needs
    // to be told where to practise before anything else. The whole domain is
    // out of reach otherwise, and that is a budget problem rather than a
    // skill one.
    let has_cloud = answers
        .cloud_experience
        .iter()
        .any(|p| matches!(p.as_str(), "aws" | "gcp" | "azure" | "multi"));

    let practise_at = if has_cloud {
        "Ton propre compte, avec une alerte de budget posée avant la première \
         ressource. Le palier gratuit d'Oracle reste utile pour ce qui doit \
         tourner en continu."
            .to_string()
    } else {
        "Commence en local : k3s et Docker suffisent pour la moitié du \
         domaine. Pour ce qui demande un nuage, le palier permanent d'Oracle \
         donne plusieurs cœurs ARM, et Cloudflare Workers ne demande aucune \
         carte bancaire."
            .to_string()
    };

    let experienced = matches!(answers.level.as_str(), "senior" | "principal");
    let scarce_time = answers.weekly_hours == "under_3";
    let oncall_ready = !matches!(answers.oncall_experience.as_str(), "never");

    let (headline, because, mut next_steps) = match (experienced, answers.objective.as_str()) {
        (true, "find_paid_work") => (
            "Va aux missions, et publie un artefact en parallèle.".to_string(),
            "Ce qui te manque ici n'est pas la compétence : c'est la trace publique. \
             Un module que quelqu'un d'autre réutilise vaut plus qu'un CV, et il \
             se produit en quelques soirées quand on sait déjà faire."
                .to_string(),
            vec![
                "Ouvre le tableau des missions et filtre sur ton métier.".to_string(),
                "Publie un module, un chart ou un tableau de bord réutilisable : c'est \
                 lui qui produira l'attestation qu'une entreprise peut vérifier."
                    .to_string(),
                "Déclare tes certifications : elles comptent, moins qu'un artefact, \
                 mais elles comptent."
                    .to_string(),
            ],
        ),
        (true, _) => (
            "Prends la relecture au sérieux.".to_string(),
            "Avec ton expérience, le chemin le plus court vers un poids réel ici est \
             de relire le travail des autres. La capability de relecture ops ouvre \
             plus de portes que n'importe quel artefact supplémentaire."
                .to_string(),
            vec![
                "Demande la capability de relecture de ta famille.".to_string(),
                "Écris un post-mortem d'un incident que tu as vraiment conduit.".to_string(),
                "Choisis un dépôt d'infrastructure du catalogue et contribue à sa \
                 documentation : c'est ce qui manque le plus et ce que personne ne fait."
                    .to_string(),
            ],
        ),
        (false, "learn") => (
            "Un cluster à toi, cette semaine.".to_string(),
            "Personne n'apprend ce métier sur la production de quelqu'un d'autre, et \
             tout ce qui compte s'apprend en local d'abord. La première panne que tu \
             provoques exprès t'apprendra plus qu'un mois de lecture."
                .to_string(),
            vec![
                "Installe k3s ou kind, et déploie quelque chose de simple.".to_string(),
                "Casse-le exprès, et note ce que tu as regardé pour comprendre.".to_string(),
                "Lis le guide de ton métier en entier avant d'ouvrir un défi.".to_string(),
            ],
        ),
        (false, _) => (
            "Un artefact que quelqu'un d'autre peut lancer.".to_string(),
            "C'est le seul objectif court qui produise une trace vérifiable dans ce \
             domaine. Un pipeline, un module ou un runbook — pas un tutoriel suivi, \
             une chose qui tourne."
                .to_string(),
            vec![
                "Lis le guide de ton métier en entier.".to_string(),
                "Prends le défi le plus facile de ton métier et va jusqu'au README.".to_string(),
                "Fais-le relire : un artefact sans relecture n'apprend rien.".to_string(),
            ],
        ),
    };

    if scarce_time {
        next_steps.push(
            "Moins de trois heures par semaine : vise un runbook ou un tableau de bord \
             plutôt qu'une migration. Un petit artefact fini vaut mieux qu'un gros \
             abandonné, et il est relisible."
                .to_string(),
        );
    }

    if !oncall_ready {
        next_steps.push(
            "Tu n'as jamais été d'astreinte : aucune mission d'astreinte ne te sera \
             proposée tant que tu ne l'auras pas demandé. Ce n'est pas un jugement — \
             être joignable est une contrainte de vie, pas une compétence."
                .to_string(),
        );
    }

    Recommendation {
        headline,
        because,
        guides,
        practise_at,
        next_steps,
        oncall_ready,
    }
}

/// The guide slug suffix for a trade.
///
/// The guides are named after the trade in short form, because
/// `ops-onboarding-database-administrator` reads badly in a URL and the
/// mapping is small enough to be explicit rather than derived.
fn short_name(trade: &str) -> &str {
    match trade {
        "devops-engineer" => "devops",
        "sre" => "sre",
        "cloud-architect" => "cloud",
        "platform-engineer" => "platform",
        "kubernetes-specialist" => "kubernetes",
        "observability-engineer" => "observability",
        "incident-commander" => "incident",
        "database-administrator" => "database",
        other => other,
    }
}

pub async fn complete(
    db: &PgPool,
    user_id: Uuid,
    answers: &WizardAnswers,
) -> Result<Recommendation, AppError> {
    validate(answers)?;

    // The trades must be real ones: a typo would point somebody at a guide
    // that does not exist and quietly recommend nothing.
    let known: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'ops' AND NOT is_archived",
    )
    .fetch_all(db)
    .await?;

    for trade in &answers.trades {
        if !known.contains(trade) {
            return Err(AppError::Validation(format!(
                "'{trade}' is not an ops trade — expected one of: {}",
                known.join(", ")
            )));
        }
    }

    // One row per person per domain (migration 0306). Replaces the whole
    // answer object rather than merging: the wizard sends every question it
    // asked, and merging would keep an answer the person has just cleared.
    let stored = serde_json::json!({
        "level": answers.level,
        "trades": answers.trades,
        "cloud_experience": answers.cloud_experience,
        "weekly_hours": answers.weekly_hours,
        "objective": answers.objective,
        "oncall_experience": answers.oncall_experience,
    });

    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
        VALUES ($1, 'ops', $2, NOW())
        ON CONFLICT (user_id, domain) DO UPDATE
            SET answers      = EXCLUDED.answers,
                completed_at = NOW(),
                -- Answering is un-skipping.
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
pub async fn skip(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO user_domain_profiles (user_id, domain, skipped_at)
        VALUES ($1, 'ops', NOW())
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

    fn answers() -> WizardAnswers {
        WizardAnswers {
            level: "junior".into(),
            trades: vec!["sre".into()],
            cloud_experience: vec!["none".into()],
            weekly_hours: "3_to_10".into(),
            objective: "build_portfolio".into(),
            oncall_experience: "never".into(),
        }
    }

    #[test]
    fn three_trades_are_refused() {
        let mut a = answers();
        a.trades = vec![
            "sre".into(),
            "cloud-architect".into(),
            "devops-engineer".into(),
        ];
        assert!(validate(&a).is_err(), "picking three means picking none");
    }

    #[test]
    fn no_trade_at_all_is_refused() {
        let mut a = answers();
        a.trades = vec![];
        assert!(validate(&a).is_err());
    }

    #[test]
    fn somebody_with_no_cloud_is_told_where_to_practise_free() {
        let rec = recommend(&answers());
        assert!(
            rec.practise_at.contains("Oracle") || rec.practise_at.contains("local"),
            "the whole domain is out of reach without this answer: {}",
            rec.practise_at
        );
    }

    #[test]
    fn never_having_been_on_call_is_not_a_deficiency_but_it_is_recorded() {
        let rec = recommend(&answers());
        assert!(!rec.oncall_ready);
        assert!(
            rec.next_steps.iter().any(|s| s.contains("astreinte")),
            "the person is told, in their own words, that nothing will be sprung on them"
        );

        let mut experienced = answers();
        experienced.oncall_experience = "regular".into();
        assert!(recommend(&experienced).oncall_ready);
    }

    #[test]
    fn scarce_time_changes_the_advice() {
        let mut a = answers();
        a.weekly_hours = "under_3".into();
        let rec = recommend(&a);
        assert!(
            rec.next_steps.iter().any(|s| s.contains("runbook")),
            "a small finished artefact beats a large abandoned one"
        );
    }

    #[test]
    fn the_guide_slugs_match_the_ones_that_exist() {
        // Migration 0247 seeds exactly these eight. A trade whose slug does
        // not map here would send somebody to a 404 on their first click.
        for trade in [
            "devops-engineer",
            "sre",
            "cloud-architect",
            "platform-engineer",
            "kubernetes-specialist",
            "observability-engineer",
            "incident-commander",
            "database-administrator",
        ] {
            let mut a = answers();
            a.trades = vec![trade.into()];
            let rec = recommend(&a);
            assert_eq!(rec.guides.len(), 1);
            assert!(
                rec.guides[0].starts_with("ops-onboarding-"),
                "{trade} maps to {}",
                rec.guides[0]
            );
        }
    }
}
