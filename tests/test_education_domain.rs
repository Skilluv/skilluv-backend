//! The education domain: the catalogue, the gates, and the rules that exist
//! because the artefacts are about people who are not here.
//!
//! What is asserted is what would be expensive to discover later — a trade
//! nobody can review, a badge whose condition nothing implements, a cohort
//! report published with a learner in it, a curriculum adopted by its own
//! author.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

/// The three trades exist, are offered, and each belongs to a review family.
#[tokio::test]
async fn the_catalogue_holds_three_live_education_trades() {
    let app = TestApp::spawn().await;

    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'education' AND NOT is_archived
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        slugs,
        vec!["coding-teacher", "curriculum-designer", "technical-trainer"]
    );

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'education' AND NOT is_archived
            AND reviewer_group IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        ungrouped.is_empty(),
        "a trade with no review family is a trade nobody can be granted rights over: {ungrouped:?}"
    );
}

/// Mentoring stayed in `soft_skills`.
///
/// The distinction migration 0517 exists to draw: a mentor takes one person
/// and follows them, a trainer takes twenty and is answerable for whether
/// they arrived. Moving the mentoring nodes here would have emptied one
/// vocabulary to fill another.
#[tokio::test]
async fn the_mentoring_skills_did_not_move() {
    let app = TestApp::spawn().await;

    let domains: Vec<String> = sqlx::query_scalar(
        "SELECT domain FROM skill_nodes
          WHERE slug IN ('mentoring-junior', 'technical-1on1')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(domains.len(), 2);
    assert!(domains.iter().all(|d| d == "soft_skills"));
}

/// The domain is open, which is what a listing reads before offering it.
#[tokio::test]
async fn the_education_domain_is_active() {
    let app = TestApp::spawn().await;

    let active: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'education'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(active, "education has a catalogue and is not offered");
}

/// Every review capability the code builds at runtime can actually be granted.
#[tokio::test]
async fn every_education_review_capability_is_grantable() {
    let app = TestApp::spawn().await;
    let reviewer = user_id(&app, "edurev").await;

    let derived: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT primary_domain || '_reviewer:' || reviewer_group
           FROM orientations
          WHERE primary_domain = 'education' AND reviewer_group IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(derived.len(), 2, "two families, two capabilities");

    for capability in derived.into_iter().chain([
        "education_reviewer:all".into(),
        "challenge_validator:education".into(),
        "domain_curator:education".into(),
    ]) {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(reviewer)
        .bind(&capability)
        .execute(&app.db)
        .await
        .unwrap_or_else(|e| panic!("{capability} cannot be granted: {e}"));
    }
}

/// Every seeded badge names a condition the engine implements, and every
/// basis it rests on exists.
#[tokio::test]
async fn every_education_badge_can_actually_fire() {
    let app = TestApp::spawn().await;

    let rules: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT slug, conditions FROM badge_rules
          WHERE slug LIKE 'education-%' AND deprecated_at IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(rules.len(), 10, "nine counted plus one, per migration 0522");

    const DIMENSIONS: &[&str] = &["challenge_language", "orientation", "target_language"];

    for (slug, conditions) in rules {
        if conditions.get("manual").and_then(|m| m.as_bool()) == Some(true) {
            continue;
        }

        if let Some(dimension) = conditions.get("distinct_over").and_then(|d| d.as_str()) {
            assert!(
                DIMENSIONS.contains(&dimension),
                "{slug} counts distinct '{dimension}', which nothing implements"
            );
        }

        if let Some(types) = conditions.get("proof_types").and_then(|t| t.as_array()) {
            for proof in types {
                let proof = proof.as_str().unwrap_or_default();
                assert!(
                    skilluv_backend::services::badge_engine::PROOF_TYPES.contains(&proof),
                    "{slug} rests on the proof type '{proof}', which nothing counts"
                );
            }
        }

        if let Some(basis) = conditions.get("attestation_basis").and_then(|b| b.as_str()) {
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM attestation_bases WHERE basis = $1)",
            )
            .bind(basis)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert!(known, "{slug} rests on basis '{basis}', which is not seeded");
        }
    }
}

/// Every craft-score term the formula names is one the profile can measure.
#[tokio::test]
async fn every_education_craft_score_term_is_measured() {
    let app = TestApp::spawn().await;

    let terms: Vec<String> = sqlx::query_scalar(
        "SELECT term FROM craft_score_weights
          WHERE skill_domain = 'education' AND is_active
          ORDER BY term",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    // The arms of the `match` in `education_profile::compute`.
    const MEASURED: &[&str] = &[
        "assessment_frameworks_published",
        "attestations_education",
        "cohorts_delivered",
        "curriculum_adoptions",
        "featured_times",
        "learners_reached",
        "missions_completed",
        "orientations_distinct",
        "review_grid_average",
        "workshops_delivered",
        "years_active",
    ];

    for term in &terms {
        assert!(
            MEASURED.contains(&term.as_str()),
            "the formula names '{term}', which nothing knows how to count"
        );
    }
    assert_eq!(terms.len(), MEASURED.len(), "every measured term is weighted");
}

/// Every attestation basis this domain seeds is one a generator can reach.
///
/// The failure this catches is a basis that reads well and is issued by
/// nothing: it appears in the catalogue, in the craft-score query and in a
/// badge condition, and no artefact ever produces it.
#[tokio::test]
async fn every_education_basis_has_a_way_of_being_issued() {
    let app = TestApp::spawn().await;

    let bases: Vec<String> = sqlx::query_scalar(
        "SELECT basis FROM attestation_bases WHERE skill_domain = 'education' ORDER BY basis",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    // `featured_educator` is editorial and issued by hand, like every other
    // `featured_*`. The rest come out of `education_attestations`.
    const REACHABLE: &[&str] = &[
        "education_assessment_framework_published",
        "education_cohort_delivered",
        "education_curriculum_authored",
        "education_workshop_delivered",
        "featured_educator",
    ];

    assert_eq!(bases, REACHABLE);
}

/// A profile answers for somebody with nothing, and says so honestly.
#[tokio::test]
async fn an_empty_education_profile_is_an_apprentice_and_not_a_500() {
    let app = TestApp::spawn().await;
    app.register_user("edunewbie").await;

    let response = app.get("/api/users/edunewbie/education-profile").await;
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["craft_score"], 0);
    assert_eq!(data["tier"], "apprentice");
    assert!(data["cohorts"].as_array().unwrap().is_empty());
}

/// A delivery that reports on learners says how many.
#[tokio::test]
async fn a_delivered_course_says_how_many_it_was_for() {
    let app = TestApp::spawn().await;
    let owner = user_id(&app, "eduowner").await;
    let project = seed_project(&app, "edu-count", owner).await;

    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             education_subtype)
         VALUES ($1, 'education_artifact', 'Cohorte', 'x', 'education', 3, 'course_delivered')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a delivered course with no headcount was accepted"
    );

    // A curriculum is a design document and reports on nobody.
    let accepted = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             education_subtype)
         VALUES ($1, 'education_artifact', 'Programme', 'x', 'education', 3,
                 'curriculum_document')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(accepted.is_ok(), "a curriculum was refused");
}

/// A testimonial cannot be stored without consent.
///
/// Enforced by the schema rather than by an endpoint, because this is the row
/// most likely to be read by something that publishes.
#[tokio::test]
async fn a_testimonial_without_consent_cannot_be_stored() {
    let app = TestApp::spawn().await;
    let teacher = user_id(&app, "eduteacher1").await;
    let learner = user_id(&app, "edulearner1").await;
    let cohort = seed_cohort(&app, teacher, &[learner]).await;

    let refused = sqlx::query(
        "INSERT INTO education_learner_outcomes
            (cohort_id, learner_user_id, testimonial_md)
         VALUES ($1, $2, 'It changed everything.')",
    )
    .bind(cohort)
    .bind(learner)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a testimonial was stored with nobody consenting to it"
    );

    let accepted = sqlx::query(
        "INSERT INTO education_learner_outcomes
            (cohort_id, learner_user_id, testimonial_md, testimonial_consent_at)
         VALUES ($1, $2, 'It changed everything.', NOW())",
    )
    .bind(cohort)
    .bind(learner)
    .execute(&app.db)
    .await;

    assert!(accepted.is_ok());
}

/// A cohort report is not attested until the learner data is declared clear,
/// the cohort is concluded, and enough learners finished.
#[tokio::test]
async fn a_cohort_attestation_waits_for_all_three_gates() {
    let app = TestApp::spawn().await;
    let teacher = user_id(&app, "eduteacher2").await;
    let learners = [
        user_id(&app, "edul1").await,
        user_id(&app, "edul2").await,
        user_id(&app, "edul3").await,
    ];
    let cohort = seed_cohort(&app, teacher, &learners).await;
    let slice = seed_course_delivered(&app, teacher, cohort).await;

    // Nothing yet: no declaration, not concluded, no outcomes.
    assert!(issued_for(&app, slice).await.is_empty());

    // Declared, still not concluded.
    sqlx::query(
        "UPDATE project_slices
            SET education_learner_data_cleared_at = NOW(),
                education_learner_data_cleared_by = $2
          WHERE id = $1",
    )
    .bind(slice)
    .bind(teacher)
    .execute(&app.db)
    .await
    .unwrap();
    assert!(issued_for(&app, slice).await.is_empty());

    // Concluded, but nobody recorded an outcome. A cohort with no outcome
    // rows has measured nothing, whatever its completion would have been.
    sqlx::query("UPDATE cohorts SET concluded_at = NOW() WHERE id = $1")
        .bind(cohort)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(issued_for(&app, slice).await.is_empty());

    // One of three finished. Below the threshold.
    for (i, learner) in learners.iter().enumerate() {
        sqlx::query(
            "INSERT INTO education_learner_outcomes (cohort_id, learner_user_id, completed)
             VALUES ($1, $2, $3)",
        )
        .bind(cohort)
        .bind(learner)
        .bind(i == 0)
        .execute(&app.db)
        .await
        .unwrap();
    }
    assert!(issued_for(&app, slice).await.is_empty());

    // All three finished.
    sqlx::query("UPDATE education_learner_outcomes SET completed = TRUE WHERE cohort_id = $1")
        .bind(cohort)
        .execute(&app.db)
        .await
        .unwrap();

    let bases = issued_for(&app, slice).await;
    assert_eq!(bases, vec!["education_cohort_delivered"]);

    // Re-running issues nothing twice.
    assert!(issued_for(&app, slice).await.is_empty());
}

/// Somebody who did not lead the cohort cannot attest it.
#[tokio::test]
async fn a_cohort_is_attested_by_the_person_who_taught_it() {
    let app = TestApp::spawn().await;
    let teacher = user_id(&app, "eduteacher3").await;
    let impostor = user_id(&app, "eduimpostor").await;
    let learner = user_id(&app, "edul4").await;

    let cohort = seed_cohort(&app, teacher, &[learner]).await;
    // The report is delivered by somebody who did not lead it.
    let slice = seed_course_delivered(&app, impostor, cohort).await;

    sqlx::query(
        "UPDATE project_slices
            SET education_learner_data_cleared_at = NOW(),
                education_learner_data_cleared_by = $2
          WHERE id = $1",
    )
    .bind(slice)
    .bind(impostor)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE cohorts SET concluded_at = NOW() WHERE id = $1")
        .bind(cohort)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO education_learner_outcomes (cohort_id, learner_user_id, completed)
         VALUES ($1, $2, TRUE)",
    )
    .bind(cohort)
    .bind(learner)
    .execute(&app.db)
    .await
    .unwrap();

    let issued =
        skilluv_backend::services::education_attestations::issue_for_slice(&app.db, slice)
            .await
            .unwrap();
    assert!(
        issued.is_empty(),
        "somebody attested a cohort they did not lead"
    );
}

/// A curriculum is attested when somebody else has run it, and never by its
/// author.
#[tokio::test]
async fn a_curriculum_is_attested_by_being_adopted() {
    let app = TestApp::spawn().await;
    let author = user_id(&app, "educurauthor").await;
    let adopter = user_id(&app, "educuradopter").await;
    let slice = seed_curriculum(&app, author).await;

    // Published, run by nobody.
    let issued =
        skilluv_backend::services::education_attestations::issue_for_slice(&app.db, slice)
            .await
            .unwrap();
    assert!(issued.is_empty(), "a curriculum nobody ran was attested");

    // The author cannot adopt their own.
    let self_adoption = sqlx::query(
        "INSERT INTO education_curriculum_adoptions (curriculum_slice_id, adopter_user_id)
         VALUES ($1, $2)",
    )
    .bind(slice)
    .bind(author)
    .execute(&app.db)
    .await;
    assert!(
        self_adoption.is_err(),
        "the author adopted their own curriculum"
    );

    sqlx::query(
        "INSERT INTO education_curriculum_adoptions (curriculum_slice_id, adopter_user_id)
         VALUES ($1, $2)",
    )
    .bind(slice)
    .bind(adopter)
    .execute(&app.db)
    .await
    .unwrap();

    let issued =
        skilluv_backend::services::education_attestations::issue_for_slice(&app.db, slice)
            .await
            .unwrap();
    assert_eq!(issued, vec!["education_curriculum_authored"]);
}

/// Outcomes are readable by the teacher and by the learner they are about,
/// and by nobody else.
#[tokio::test]
async fn an_outcome_row_is_not_readable_by_a_stranger() {
    let app = TestApp::spawn().await;
    let teacher = user_id(&app, "eduteacher4").await;
    let learner = user_id(&app, "edul5").await;
    let cohort = seed_cohort(&app, teacher, &[learner]).await;

    sqlx::query(
        "INSERT INTO education_learner_outcomes (cohort_id, learner_user_id, completed)
         VALUES ($1, $2, TRUE)",
    )
    .bind(cohort)
    .bind(learner)
    .execute(&app.db)
    .await
    .unwrap();

    // The session belongs to the last account registered, which is the
    // learner: they see their own row.
    let mine = app
        .get(&format!("/api/education/cohorts/{cohort}/outcomes"))
        .await;
    assert_eq!(mine.status(), 200);
    let body: serde_json::Value = mine.json().await.unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // Somebody with no connection to the cohort gets a refusal rather than an
    // empty list: the difference would tell them the cohort exists.
    app.register_user("edustranger").await;
    let theirs = app
        .get(&format!("/api/education/cohorts/{cohort}/outcomes"))
        .await;
    assert_eq!(theirs.status(), 403);
}

/// Concluding a cohort says whether it supports the claim its report will
/// make.
#[tokio::test]
async fn concluding_a_cohort_says_whether_it_meets_the_threshold() {
    let app = TestApp::spawn().await;
    let learner = user_id(&app, "edul6").await;
    let teacher = user_id(&app, "eduteacher5").await;
    let cohort = seed_cohort(&app, teacher, &[learner]).await;

    // The session is the teacher's: they registered last.
    let response = app
        .post(&format!("/api/education/cohorts/{cohort}/conclude"), &json!({}))
        .await;
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["data"]["concluded"], true);
    assert_eq!(
        body["data"]["meets_attestation_threshold"], false,
        "a cohort with no recorded outcomes must not read as attestable"
    );

    // Concluding twice is refused: it already happened.
    let again = app
        .post(&format!("/api/education/cohorts/{cohort}/conclude"), &json!({}))
        .await;
    assert_eq!(again.status(), 403);
}


/// A education attestation counts towards the platform rank like any other.
///
/// Ticket F-07 asked for a check that it does. It does because the rank
/// counts attestations without looking at what they rest on — which is the
/// right design and the kind of thing that gets broken by somebody adding a
/// domain filter for a reason that seemed good at the time.
#[tokio::test]
async fn the_rank_counts_education_attestations_like_any_other() {
    let app = TestApp::spawn().await;
    let person = user_id(&app, "eduranker").await;

    let before = skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, person)
        .await
        .unwrap();
    assert_eq!(before.1, "apprenti");

    // Eleven verified deliverables and one attestation is `artisan`, whatever
    // domain either of them came from.
    for _ in 0..11 {
        common::delivered_in(&app, person, "education", "teaching").await;
    }

    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code, basis,
             linked_deliverable_ids)
         SELECT $1, 'artefact', 'x', 'y', 'RANKTEST02', 'education_workshop_delivered',
                ARRAY[(SELECT id FROM deliverables WHERE user_id = $1 LIMIT 1)]",
    )
    .bind(person)
    .execute(&app.db)
    .await
    .unwrap();

    let after = skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, person)
        .await
        .unwrap();
    assert_eq!(
        after.1, "artisan",
        "a education attestation did not count towards the rank"
    );
}
/// Whatever the generators issue for one slice, as it stands right now.
async fn issued_for(app: &TestApp, slice: Uuid) -> Vec<String> {
    skilluv_backend::services::education_attestations::issue_for_slice(&app.db, slice)
        .await
        .expect("the education generators")
}

// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .expect("registered user")
}

async fn seed_project(app: &TestApp, slug: &str, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, 'Projet', 'x', 'user', $2) RETURNING id",
    )
    .bind(format!("{slug}-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .expect("project")
}

/// A taught cohort with its members.
async fn seed_cohort(app: &TestApp, teacher: Uuid, learners: &[Uuid]) -> Uuid {
    let cohort: Uuid = sqlx::query_scalar(
        "INSERT INTO cohorts
            (slug, name, description, starts_at, ends_at, led_by_user_id, created_by)
         VALUES ($1, 'Cohorte', 'x', NOW() - INTERVAL '8 weeks', NOW(), $2, $2)
         RETURNING id",
    )
    .bind(format!("cohort-{}", Uuid::new_v4()))
    .bind(teacher)
    .fetch_one(&app.db)
    .await
    .expect("cohort");

    for learner in learners {
        sqlx::query("INSERT INTO cohort_members (cohort_id, user_id) VALUES ($1, $2)")
            .bind(cohort)
            .bind(learner)
            .execute(&app.db)
            .await
            .expect("cohort member");
    }
    cohort
}

/// A verified delivered-course artefact reporting on one cohort.
async fn seed_course_delivered(app: &TestApp, author: Uuid, cohort: Uuid) -> Uuid {
    let project = seed_project(app, "edu-delivery", author).await;

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, education_subtype, education_learners_count, education_cohort_id,
             published_artifact_url, orientation_id)
         VALUES ($1, 'education_artifact', 'Cohorte livrée', 'x', 'education', 4,
                 'validated', 'course_delivered', 3, $2,
                 'https://example.test/cohort',
                 (SELECT id FROM orientations WHERE slug = 'technical-trainer'))
         RETURNING id",
    )
    .bind(project)
    .bind(cohort)
    .fetch_one(&app.db)
    .await
    .expect("course slice");

    verified_deliverable(app, slice, author).await;
    slice
}

/// A verified curriculum artefact.
async fn seed_curriculum(app: &TestApp, author: Uuid) -> Uuid {
    let project = seed_project(app, "edu-curriculum", author).await;

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, education_subtype, published_artifact_url, orientation_id)
         VALUES ($1, 'education_artifact', 'Programme', 'x', 'education', 4,
                 'validated', 'curriculum_document', 'https://example.test/curriculum',
                 (SELECT id FROM orientations WHERE slug = 'curriculum-designer'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .expect("curriculum slice");

    verified_deliverable(app, slice, author).await;
    slice
}

async fn verified_deliverable(app: &TestApp, slice: Uuid, author: Uuid) {
    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, public)
         VALUES ($1, $2, 'other', 'https://example.test/x', 'human_review',
                 'verified', NOW(), TRUE)",
    )
    .bind(slice)
    .bind(author)
    .execute(&app.db)
    .await
    .expect("verified deliverable");
}
