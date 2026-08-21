//! What to do first, given what somebody said in the wizard.
//!
//! ## Rules, not a model
//!
//! Written as explicit combinations because the reasoning has to be
//! defensible to the person receiving it: "you said beginner, web and five
//! hours a week, so here is the web guide and the easiest open issues in
//! TypeScript". A score nobody can explain would be a worse version of the
//! same answer.
//!
//! ## Deliberately short
//!
//! A first month with nine items is a first month nobody starts. Three steps,
//! four when an answer earns a fourth.
//!
//! ## Where the code rules went
//!
//! They were `services::code_onboarding`, which also owned its own storage on
//! eight columns of `users`. The storage is now `user_domain_profiles` like
//! every other domain's, and these rules moved here unchanged — the words a
//! senior developer reads are the words they read before.

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Recommendation {
    /// Said in one sentence, in the second person. What the person reads
    /// first.
    pub headline: String,
    /// Why this and not something else. The reasoning, so it can be argued
    /// with.
    pub because: String,
    /// Guides to open, by slug.
    pub guides: Vec<String>,
    /// Where to look for work: a ready-made query against the domain's feed.
    pub feed_query: String,
    /// What to aim at in the first month.
    pub next_steps: Vec<String>,
}

/// One answer, as a string.
fn answer(answers: &Value, key: &str) -> Option<String> {
    answers.get(key)?.as_str().map(str::to_string)
}

/// One answer, as a list. A wrong shape reads as absent rather than
/// panicking: the object is written by a wizard and read here, and a
/// mismatched version of the two must degrade to "no preference".
fn list(answers: &Value, key: &str) -> Vec<String> {
    answers
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// What to do first.
///
/// Every domain gets an answer. An unrecognised one gets the general advice
/// rather than nothing: somebody who has just filled in six questions and is
/// shown a blank page concludes the wizard was pointless, which is worse than
/// generic advice.
pub fn recommend(domain: &str, answers: &Value) -> Recommendation {
    match domain {
        "code" => code(answers),
        "design" => design(answers),
        _ => general(domain, answers),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Code
// ═══════════════════════════════════════════════════════════════════

fn code(answers: &Value) -> Recommendation {
    let family = list(answers, "preferred_families")
        .first()
        .cloned()
        .unwrap_or_else(|| "web".into());
    let language = list(answers, "main_tools").first().cloned();
    let level = answer(answers, "level").unwrap_or_default();
    let goal = answer(answers, "goal").unwrap_or_default();

    // A language narrows the feed usefully; a family does not, because the
    // feed filters on trades and a family is eight of them.
    let feed_query = match &language {
        Some(language) => format!("/api/code/first-issues?language={language}&max_difficulty=3"),
        None => "/api/code/first-issues?max_difficulty=3".to_string(),
    };

    let experienced = matches!(level.as_str(), "senior" | "staff");

    let (headline, because, mut next_steps) = match (experienced, goal.as_str()) {
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

    if answer(answers, "weekly_hours").as_deref() == Some("under_5") {
        next_steps.push(
            "Moins de cinq heures par semaine : vise une seule chose à la fois, et une petite. \
             Une contribution finie vaut mieux que trois commencées."
                .to_string(),
        );
    }
    if answer(answers, "challenge_preference").as_deref() == Some("short_hackathons") {
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
        feed_query,
        next_steps,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Design
// ═══════════════════════════════════════════════════════════════════

fn design(answers: &Value) -> Recommendation {
    let trade = list(answers, "preferred_families").first().cloned();
    let level = answer(answers, "level").unwrap_or_default();
    let goal = answer(answers, "goal").unwrap_or_default();
    let preference = answer(answers, "challenge_preference").unwrap_or_default();

    // The trade narrows the feed here, unlike code: a design trade *is* the
    // filter the catalogue is organised by.
    let feed_query = match &trade {
        Some(trade) => format!("/api/users/me/next-challenges?orientation={trade}"),
        None => "/api/users/me/next-challenges".to_string(),
    };

    let experienced = matches!(level.as_str(), "senior" | "researcher");

    let (headline, because, mut next_steps) = match (experienced, goal.as_str()) {
        (true, "paid_missions") => (
            "Va aux missions, et garde un livrable public en parallèle.".to_string(),
            "Ton problème n'est pas d'apprendre le métier, c'est qu'ici personne ne peut encore \
             vérifier que tu le fais. Une mission paie ; un livrable validé se montre."
                .to_string(),
            vec![
                "Ouvre le tableau des missions et filtre sur ton métier.".to_string(),
                "Déclare tes portfolios existants : un lien confirmé compte dans la recherche \
                 des recruteurs, un lien affirmé ne compte pas."
                    .to_string(),
                "Prends un livrable en parallèle : c'est lui qui produit l'attestation."
                    .to_string(),
            ],
        ),
        (true, _) => (
            "Prends une critique, pas un exercice.".to_string(),
            "Les rounds d'itération sont faits pour apprendre un processus que tu connais. Ce \
             qui te distinguera est un travail que peu de gens peuvent tenir, et le regard de \
             quelqu'un d'aussi expérimenté que toi."
                .to_string(),
            vec![
                "Choisis un brief exigeant plutôt qu'un exercice cadré.".to_string(),
                "Candidate à la relecture dans ta famille : c'est la capability qui ouvre le \
                 plus de portes ici."
                    .to_string(),
                "Propose un brief : la banque de briefs manque de sujets écrits par des gens \
                 qui ont livré."
                    .to_string(),
            ],
        ),
        (false, "portfolio") => (
            "Un livrable validé avant la fin du mois.".to_string(),
            "C'est le chemin le plus court vers une pièce que tu peux montrer avec la trace de \
             sa validation — ce qu'un Behance ne donne pas."
                .to_string(),
            vec![
                "Prends un défi individuel de ton métier, le plus petit.".to_string(),
                "Rends les sources, pas seulement l'image : un livrable qu'on ne peut pas \
                 rouvrir n'est pas livré."
                    .to_string(),
                "Attends la critique et itère : deux ou trois rounds sont le cas normal, pas \
                 un échec."
                    .to_string(),
            ],
        ),
        (false, _) => (
            "Trente jours, un livrable, jusqu'au bout de la critique.".to_string(),
            "Ce qui compte au début n'est pas le volume, c'est d'avoir traversé une fois le \
             cycle complet — y compris le round où on te dit « pas encore »."
                .to_string(),
            vec![
                "Déclare un métier : sans lui, le catalogue ne peut rien te proposer.".to_string(),
                "Prends le défi le plus facile de ce métier.".to_string(),
                "Lis la grille de critique avant de rendre : elle dit sur quoi tu seras lu."
                    .to_string(),
            ],
        ),
    };

    if answer(answers, "weekly_hours").as_deref() == Some("lt3") {
        next_steps.push(
            "Moins de trois heures par semaine : une seule chose à la fois, et une petite. Un \
             livrable fini vaut mieux que trois commencés."
                .to_string(),
        );
    }
    match preference.as_str() {
        "contest" => next_steps.push(
            "Tu préfères les concours : surveille les briefs ouverts, ils ont une date de fin \
             et c'est ce qui fait avancer."
                .to_string(),
        ),
        "individual" => next_steps.push(
            "Tu préfères travailler seul : les défis individuels n'ont pas de date de fin, \
             donne-t'en une."
                .to_string(),
        ),
        _ => {}
    }

    let mut guides = vec!["toolkit-design".to_string()];
    if let Some(trade) = trade {
        guides.insert(0, format!("onboarding-{trade}"));
    }

    Recommendation {
        headline,
        because,
        guides,
        feed_query,
        next_steps,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Everything else
// ═══════════════════════════════════════════════════════════════════

/// The advice that holds whatever the trade is.
///
/// Four domains have a wizard and no rules of their own yet. They get this
/// rather than an empty answer: somebody who has just filled in six questions
/// and is shown a blank page concludes the wizard was pointless.
fn general(domain: &str, answers: &Value) -> Recommendation {
    let scarce = matches!(
        answer(answers, "weekly_hours").as_deref(),
        Some("lt3") | Some("under_5")
    );

    let mut next_steps = vec![
        "Déclare un métier : sans lui, le catalogue ne peut rien te proposer.".to_string(),
        "Prends le défi le plus facile de ce métier et rends-le en entier.".to_string(),
        "Lis la critique jusqu'au bout, même quand elle dit « pas encore ».".to_string(),
    ];
    if scarce {
        next_steps.push(
            "Peu de temps par semaine : une seule chose à la fois, et une petite.".to_string(),
        );
    }

    Recommendation {
        headline: "Trente jours, un livrable vérifiable.".to_string(),
        because: "Une seule traversée complète du cycle vaut mieux que trois débuts : c'est \
                  elle qui laisse une trace qu'un inconnu peut vérifier."
            .to_string(),
        guides: vec![format!("toolkit-{domain}")],
        feed_query: "/api/users/me/next-challenges".to_string(),
        next_steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_senior_after_paid_work_is_not_sent_to_the_beginner_feed() {
        let out = recommend(
            "code",
            &json!({"level": "staff", "goal": "find_paid_work", "main_tools": ["rust"]}),
        );
        assert!(out.headline.contains("missions"), "{}", out.headline);
        assert!(out.feed_query.contains("language=rust"));
    }

    #[test]
    fn a_beginner_is_sent_somewhere_they_can_start() {
        let out = recommend("code", &json!({"level": "beginner", "goal": "learn"}));
        assert!(out.next_steps.len() >= 3);
        assert!(out.feed_query.contains("max_difficulty=3"));
    }

    #[test]
    fn scarce_time_earns_an_extra_sentence_in_both_vocabularies() {
        // Code says `under_5`, design says `lt3`. The same answer to the same
        // question, and both have to be heard.
        let code = recommend(
            "code",
            &json!({"level": "junior", "weekly_hours": "under_5"}),
        );
        let design = recommend(
            "design",
            &json!({"level": "apprentissage", "weekly_hours": "lt3"}),
        );
        assert!(
            code.next_steps
                .iter()
                .any(|s| s.contains("une seule chose"))
        );
        assert!(
            design
                .next_steps
                .iter()
                .any(|s| s.contains("une seule chose"))
        );
    }

    #[test]
    fn a_design_trade_narrows_the_feed_and_names_a_guide() {
        let out = recommend(
            "design",
            &json!({"level": "debutant", "preferred_families": ["design-brand-identity"]}),
        );
        assert!(
            out.feed_query.contains("design-brand-identity"),
            "{}",
            out.feed_query
        );
        assert_eq!(out.guides[0], "onboarding-design-brand-identity");
    }

    #[test]
    fn a_domain_with_no_rules_still_gets_an_answer() {
        // A blank page after six questions reads as "that was pointless".
        let out = recommend("ops", &json!({"level": "debutant"}));
        assert!(!out.headline.is_empty());
        assert!(!out.next_steps.is_empty());
    }

    #[test]
    fn an_answer_of_the_wrong_shape_degrades_to_no_preference() {
        // The object is written by a wizard and read here; a mismatched pair
        // of versions must not panic.
        let out = recommend(
            "design",
            &json!({"preferred_families": "design-web", "level": 7}),
        );
        assert!(!out.feed_query.contains("design-web"));
        assert!(!out.headline.is_empty());
    }
}
