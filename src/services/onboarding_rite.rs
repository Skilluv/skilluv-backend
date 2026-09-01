//! What Bonjour Skilluv asks for, per domain.
//!
//! One rite, twelve gestures. The name, the badge and the moment are single;
//! the shape belongs to the trade. Until SKI-362 there was one shape — fork a
//! GitHub starter and open a pull request — and the first thing the platform
//! asked of a designer, a sound engineer or a teacher was to open a GitHub
//! account.
//!
//! This is the declaration the API serves and the front renders: for a domain,
//! what to do, what is handed in, and who reads it. The brief a person
//! actually works from is the `is_domain_rite` template of migration 0607;
//! this says what surface to give them for it.
//!
//! It is a table in Rust rather than rows in Postgres because the mechanism
//! differs per entry, not just the wording: `Fork` calls GitHub and waits on a
//! webhook, `Submission` writes a `challenge_submissions` row. A column cannot
//! hold that difference, and a row per domain would be a configuration that
//! looks editable and is not.

/// How the platform receives the gesture.
///
/// Deliberately two, not twelve. A screen, a playtest verdict, a finding and
/// twenty seconds of sound differ entirely in what they *are*, and not at all
/// in what the backend does with them: an artifact arrives against a template
/// and a person reads it. Multiplying the mechanism by the wording is how a
/// platform ends up with eleven half-built ingestion paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiteForm {
    /// A `skilluv-community/starter-*` forked on the caller's GitHub account,
    /// a pull request on their own fork, a webhook. Requires a connected
    /// GitHub account — and is the only form that does.
    Fork,
    /// An artifact submitted against the domain's rite template, which lands
    /// in the human review queue (SKI-361). No GitHub account, no repository.
    Submission,
}

/// One domain's first gesture.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Rite {
    pub domain: &'static str,
    pub form: RiteForm,
    /// One line: what the person does. The full brief is the template's.
    pub gesture: &'static str,
    /// What is handed in — what the front builds an input for.
    pub expected_artifact: &'static str,
    /// The loop this trade *continues* into once the rite is passed — the
    /// design critique, the playtest verdicts, the disclosure programme.
    ///
    /// Not where the rite itself is read. Every rite, all twelve, is read in
    /// the generic human review queue (`review_tasks` + `reviews`), and this
    /// field used to be called `review_loop`, which said otherwise. It named a
    /// routing that does not exist and would have had a front announcing
    /// "three design reviewers are looking at this" when one task was sitting
    /// in one queue.
    ///
    /// It stays because it is worth telling somebody where their trade goes
    /// next. It is documentation, and now it is named like documentation.
    pub continues_in: &'static str,
}

/// The twelve. Order is `SKILL_DOMAINS`' order, and
/// `every_domain_has_exactly_one_rite` keeps it that way.
pub const RITES: &[Rite] = &[
    Rite {
        domain: "code",
        form: RiteForm::Fork,
        gesture: "Fork a Skilluv starter, introduce yourself in HELLO.md, and open the pull request.",
        expected_artifact: "A pull request from `main` to `showcase` on your own fork, touching HELLO.md.",
        continues_in: "github_webhook",
    },
    Rite {
        domain: "design",
        form: RiteForm::Submission,
        gesture: "Design one screen against the entry brief of your trade, and say what each choice serves.",
        expected_artifact: "One screen, uploaded, with two or three sentences of rationale.",
        continues_in: "design_critique",
    },
    Rite {
        domain: "game",
        form: RiteForm::Submission,
        gesture: "Play one published slice start to finish and return a playtest verdict.",
        expected_artifact: "A written verdict: what it taught without telling, where you stuck, the first change.",
        continues_in: "playtest_verdicts",
    },
    Rite {
        domain: "security",
        form: RiteForm::Submission,
        gesture: "Read the published scope, test only inside it, and report one finding.",
        expected_artifact: "One reproducible finding: what you did, what happened, why it matters.",
        continues_in: "disclosure_programme",
    },
    Rite {
        domain: "ops",
        form: RiteForm::Submission,
        gesture: "Read one SLO of the Skilluv ops ground and propose one improvement with its cost.",
        expected_artifact: "A written proposal naming the SLO, what it misses, and the trade-off.",
        continues_in: "ops_ground",
    },
    Rite {
        domain: "ai",
        form: RiteForm::Submission,
        gesture: "Take the first workspace step of an entry mission, and show what you checked.",
        expected_artifact: "The step, plus what you ran, what came back, and what you rejected.",
        continues_in: "missions",
    },
    Rite {
        domain: "soft_skills",
        form: RiteForm::Submission,
        gesture: "Review one public deliverable: what holds, what to change, and why.",
        expected_artifact: "A review naming what to keep before what to change, with an order.",
        continues_in: "review_queue",
    },
    Rite {
        domain: "audio",
        form: RiteForm::Submission,
        gesture: "Make a twenty-second signature and declare every source.",
        expected_artifact: "Twenty seconds of audio, with a source list including licences.",
        continues_in: "audio_castings",
    },
    Rite {
        domain: "quality",
        form: RiteForm::Submission,
        gesture: "File one defect report on the Skilluv canvas that needs no follow-up question.",
        expected_artifact: "Steps, expected, actual, where — and how sure you are.",
        continues_in: "defect_reports",
    },
    Rite {
        domain: "leadership",
        form: RiteForm::Submission,
        gesture: "Write a retro on a public Skilluv incident, and propose one change with an owner.",
        expected_artifact: "A retro naming causes rather than people, ending on a measurable change.",
        continues_in: "retros",
    },
    Rite {
        domain: "communication",
        form: RiteForm::Submission,
        gesture: "Translate one paragraph of a guide, and defend the choices that are not literal.",
        expected_artifact: "The translation, plus notes on the two or three departures.",
        continues_in: "translation_reviews",
    },
    Rite {
        domain: "education",
        form: RiteForm::Submission,
        gesture: "Explain one skill node in three beats, to somebody who does not have it yet.",
        expected_artifact: "Problem solved, smallest example, first mistake — in that order.",
        continues_in: "cohorts",
    },
];

/// The rite of a domain, or `None` for a string that is not one.
///
/// Callers pass a domain that has already been through
/// `validators::check_skill_domain`; `None` here therefore means the tables
/// have drifted, which the unit test below makes impossible to ship.
pub fn for_domain(domain: &str) -> Option<&'static Rite> {
    RITES.iter().find(|r| r.domain == domain)
}

/// Whether this domain's gesture needs a connected GitHub account.
///
/// The one question the start endpoint used to answer with "yes, always".
pub fn requires_github(domain: &str) -> bool {
    matches!(
        for_domain(domain).map(|r| r.form),
        Some(RiteForm::Fork) | None
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validators::SKILL_DOMAINS;

    /// A domain with no rite is a domain whose new accounts land on an error,
    /// which is exactly the state SKI-360 and SKI-362 were filed about. The
    /// loop is over the constant, never over a list written here, so the
    /// thirteenth domain fails this the day it is added rather than the day
    /// somebody signs up for it.
    #[test]
    fn every_domain_has_exactly_one_rite() {
        let declared: Vec<&str> = RITES.iter().map(|r| r.domain).collect();
        assert_eq!(
            declared, SKILL_DOMAINS,
            "the rite catalogue and validators::SKILL_DOMAINS have drifted"
        );
    }

    /// Only `code` forks. The whole ticket is that the other eleven must not.
    #[test]
    fn only_the_code_rite_needs_github() {
        for rite in RITES {
            assert_eq!(
                rite.form == RiteForm::Fork,
                rite.domain == "code",
                "{} has the wrong form",
                rite.domain
            );
            assert_eq!(requires_github(rite.domain), rite.domain == "code");
        }
    }

    /// Every field is a sentence somebody reads, so none of them may be blank.
    #[test]
    fn no_rite_is_half_written() {
        for rite in RITES {
            assert!(!rite.gesture.is_empty(), "{}: no gesture", rite.domain);
            assert!(
                !rite.expected_artifact.is_empty(),
                "{}: nothing to hand in",
                rite.domain
            );
            assert!(
                !rite.continues_in.is_empty(),
                "{}: the trade goes nowhere after the rite",
                rite.domain
            );
        }
    }
}
