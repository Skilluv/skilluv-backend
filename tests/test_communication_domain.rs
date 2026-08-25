//! The communication domain: the catalogue, the rights, and the rules the
//! schema enforces rather than trusts.
//!
//! What is asserted here is what would be expensive to discover later — a
//! trade nobody can review, a badge whose condition nothing implements, a
//! translation attested by the person who wrote it.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

/// The five trades exist, are offered, and each belongs to a review family.
#[tokio::test]
async fn the_catalogue_holds_five_live_communication_trades() {
    let app = TestApp::spawn().await;

    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'communication' AND NOT is_archived
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        slugs,
        vec![
            "content-creator-tech",
            "developer-advocate",
            "research-writer-tech",
            "tech-writer",
            "technical-translator",
        ]
    );

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'communication' AND NOT is_archived
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

/// `tech-writer` moved out of `soft_skills` rather than being copied.
///
/// The failure this catches is the one migrations 0209 and 0402 both refused:
/// two rows for one trade give two answers to whether somebody holds it, and
/// both get read.
#[tokio::test]
async fn the_legacy_tech_writer_moved_and_was_not_duplicated() {
    let app = TestApp::spawn().await;

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT slug, primary_domain FROM orientations WHERE slug = 'tech-writer'")
            .fetch_all(&app.db)
            .await
            .unwrap();

    assert_eq!(rows.len(), 1, "one trade, one row");
    assert_eq!(rows[0].1, "communication");

    // The other legacy candidate stayed where it was: a maintainer's job is
    // triage, review and releases, and filing it here would put it under a
    // review family that cannot judge it.
    let maintainer: String = sqlx::query_scalar(
        "SELECT primary_domain FROM orientations WHERE slug = 'open-source-maintainer'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(maintainer, "soft_skills");
}

/// The domain is open, which is what a listing reads before offering it.
#[tokio::test]
async fn the_communication_domain_is_active() {
    let app = TestApp::spawn().await;

    let active: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'communication'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(active, "communication has a catalogue and is not offered");
}

/// Every review capability the code builds at runtime can actually be granted.
///
/// The failure this catches is the one migration 0305 documented: the
/// capability name is assembled from an orientation row, so a missing value
/// does not fail to compile — it produces a grant the database refuses.
#[tokio::test]
async fn every_communication_review_capability_is_grantable() {
    let app = TestApp::spawn().await;
    let reviewer = user_id(&app, "commrev").await;

    let derived: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT primary_domain || '_reviewer:' || reviewer_group
           FROM orientations
          WHERE primary_domain = 'communication' AND reviewer_group IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(derived.len(), 4, "four families, four capabilities");

    for capability in derived.into_iter().chain([
        "communication_reviewer:all".into(),
        "challenge_validator:communication".into(),
        "domain_curator:communication".into(),
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

/// Every seeded badge names a condition the engine implements.
///
/// A rule naming a proof type or a dimension nothing counts is a badge that
/// silently never fires: the row exists, the description reads well, and
/// nobody ever gets it.
#[tokio::test]
async fn every_communication_badge_can_actually_fire() {
    let app = TestApp::spawn().await;

    let rules: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT slug, conditions FROM badge_rules
          WHERE slug LIKE 'communication-%' AND deprecated_at IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        rules.len(),
        12,
        "eleven counted plus one, per migration 0505"
    );

    // The dimensions `count_distinct_dimension` knows. `target_language` was
    // added for `communication-polyglot`; seeding the rule without the
    // implementation is exactly the failure this asserts against.
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

        // A rule resting on an attestation basis has to name one that exists,
        // or it counts nothing forever.
        if let Some(basis) = conditions.get("attestation_basis").and_then(|b| b.as_str()) {
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM attestation_bases WHERE basis = $1)",
            )
            .bind(basis)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert!(
                known,
                "{slug} rests on basis '{basis}', which is not seeded"
            );
        }
    }
}

/// Every craft-score term the formula names is one the profile can measure.
///
/// A weight row naming a term nothing counts is skipped and logged rather than
/// guessed at, which is correct behaviour and an invisible undercount. The
/// list is short enough to assert.
#[tokio::test]
async fn every_communication_craft_score_term_is_measured() {
    let app = TestApp::spawn().await;

    let terms: Vec<String> = sqlx::query_scalar(
        "SELECT term FROM craft_score_weights
          WHERE skill_domain = 'communication' AND is_active
          ORDER BY term",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    // The arms of the `match` in `communication_profile::compute`.
    const MEASURED: &[&str] = &[
        "attestations_communication",
        "audience_reach",
        "content_published",
        "docs_contributions",
        "featured_times",
        "missions_completed",
        "orientations_distinct",
        "research_published",
        "review_grid_average",
        "talks_delivered",
        "target_languages_distinct",
        "translations_validated",
        "years_active",
    ];

    for term in &terms {
        assert!(
            MEASURED.contains(&term.as_str()),
            "the formula names '{term}', which nothing knows how to count"
        );
    }
    assert_eq!(
        terms.len(),
        MEASURED.len(),
        "every measured term is weighted"
    );
}

/// A profile answers for somebody with nothing, and says so honestly.
#[tokio::test]
async fn an_empty_communication_profile_is_an_apprentice_and_not_a_500() {
    let app = TestApp::spawn().await;
    app.register_user("commnewbie").await;

    let response = app.get("/api/users/commnewbie/communication-profile").await;
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["craft_score"], 0);
    assert_eq!(data["tier"], "apprentice");
    assert!(
        data["breakdown"].as_array().unwrap().is_empty(),
        "nothing counted means nothing in the breakdown, not a zero for every term"
    );
}

/// The schema refuses a translation that does not say which way it went.
///
/// Without the pair there is no language to match a reviewer against, and the
/// polyglot badge would count a tag nobody can check.
#[tokio::test]
async fn a_translation_has_to_name_its_languages() {
    let app = TestApp::spawn().await;
    let owner = user_id(&app, "translangowner").await;
    let project = seed_project(&app, "trans-langs", owner).await;

    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             communication_subtype)
         VALUES ($1, 'communication_artifact', 'Traduction', 'x', 'communication', 3,
                 'translation')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a translation with no source and no target was accepted"
    );

    // And the reverse: languages on something that is not a translation would
    // make the polyglot badge countable from prose.
    let also_refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             communication_subtype, published_artifact_url,
             communication_target_languages)
         VALUES ($1, 'communication_artifact', 'Article', 'x', 'communication', 3,
                 'blog_post', 'https://example.test/a', ARRAY['fr'])",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(
        also_refused.is_err(),
        "a blog post was allowed to claim a target language"
    );
}

/// What claims to be published has to say where.
#[tokio::test]
async fn a_published_piece_has_to_name_an_address() {
    let app = TestApp::spawn().await;
    let owner = user_id(&app, "pubaddrowner").await;
    let project = seed_project(&app, "pub-address", owner).await;

    for subtype in [
        "blog_post",
        "video_content",
        "devrel_talk",
        "research_paper",
    ] {
        let refused = sqlx::query(
            "INSERT INTO project_slices
                (project_id, slice_type, title, description, primary_domain, difficulty,
                 communication_subtype)
             VALUES ($1, 'communication_artifact', 'Pièce', 'x', 'communication', 3, $2)",
        )
        .bind(project)
        .bind(subtype)
        .execute(&app.db)
        .await;

        assert!(
            refused.is_err(),
            "a {subtype} with nothing to open was accepted"
        );
    }

    // A documentation change lives in the pull request that carried it, and
    // demanding a second address would mean inventing one.
    let accepted = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             communication_subtype)
         VALUES ($1, 'communication_artifact', 'Docs', 'x', 'communication', 3,
                 'documentation')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(accepted.is_ok(), "a documentation change was refused");
}

/// A translation is not validated by the person who translated it.
#[tokio::test]
async fn a_translator_cannot_validate_their_own_translation() {
    let app = TestApp::spawn().await;
    let translator = user_id(&app, "selftrans").await;
    let slice = seed_translation(&app, translator, "fr").await;

    // Rights and the declared language, so the only thing left to refuse is
    // that it is their own work.
    grant(&app, translator, "communication_reviewer:translation").await;
    sqlx::query("INSERT INTO user_review_languages (user_id, language) VALUES ($1, 'fr')")
        .bind(translator)
        .execute(&app.db)
        .await
        .unwrap();

    let outcome = skilluv_backend::services::communication_attestations::validate_translation(
        &app.db, translator, slice, "fr", "",
    )
    .await;

    assert!(
        outcome.is_err(),
        "the translator validated their own translation"
    );
}

/// A reviewer who has not declared the language cannot sign for it.
#[tokio::test]
async fn a_reviewer_signs_only_in_a_language_they_declared() {
    let app = TestApp::spawn().await;
    let author = user_id(&app, "transauthor").await;
    let reviewer = user_id(&app, "transreviewer").await;

    let slice = seed_translation(&app, author, "sw").await;
    grant(&app, reviewer, "communication_reviewer:translation").await;

    let undeclared = skilluv_backend::services::communication_attestations::validate_translation(
        &app.db, reviewer, slice, "sw", "",
    )
    .await;
    assert!(
        undeclared.is_err(),
        "somebody signed for a language they never claimed to read"
    );

    sqlx::query(
        "INSERT INTO user_review_languages (user_id, language, proficiency)
         VALUES ($1, 'sw', 'native')",
    )
    .bind(reviewer)
    .execute(&app.db)
    .await
    .unwrap();

    // And a language the artefact does not target is still refused: a review
    // in the wrong language attests something that did not happen.
    let wrong_language =
        skilluv_backend::services::communication_attestations::validate_translation(
            &app.db, reviewer, slice, "fr", "",
        )
        .await;
    assert!(wrong_language.is_err());

    let issued = skilluv_backend::services::communication_attestations::validate_translation(
        &app.db,
        reviewer,
        slice,
        "sw",
        "Relu, terminologie cohérente.",
    )
    .await
    .expect("a declared reviewer signing in the right language");

    assert!(issued.is_some(), "nothing was attested");

    // The review is kept, so the claim can be traced to a person rather than
    // to an attestation that appeared.
    let signed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM translation_reviews
          WHERE slice_id = $1 AND reviewer_user_id = $2 AND language = 'sw'",
    )
    .bind(slice)
    .bind(reviewer)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(signed, 1);

    // Re-running issues nothing twice.
    let again = skilluv_backend::services::communication_attestations::validate_translation(
        &app.db, reviewer, slice, "sw", "",
    )
    .await
    .unwrap();
    assert!(again.is_none(), "a second pass issued a second attestation");
}

/// The declared language endpoint refuses what is not a tag, and accepts a
/// language nobody has heard of.
#[tokio::test]
async fn declaring_a_review_language_is_open_but_not_free_text() {
    let app = TestApp::spawn().await;
    app.register_user("langdeclarer").await;

    let refused = app
        .post(
            "/api/communication/review-languages",
            &json!({ "language": "french please" }),
        )
        .await;
    assert_eq!(refused.status(), 400);

    // A language with two thousand speakers and no ISO tooling is still a
    // language, and a closed list would be a statement that it is not.
    let accepted = app
        .post(
            "/api/communication/review-languages",
            &json!({ "language": "dyu", "proficiency": "native" }),
        )
        .await;
    assert_eq!(accepted.status(), 200);

    let listed = app.get("/api/communication/review-languages").await;
    let body: serde_json::Value = listed.json().await.unwrap();
    assert_eq!(body["data"][0]["language"], "dyu");
    assert_eq!(body["data"][0]["proficiency"], "native");
}

/// The guides answer in the locale they exist in rather than disappearing.
///
/// This asserted that a French reader got English rows, which was true while
/// the French translations did not exist and was never the thing worth
/// asserting. Migration 0535 wrote them, so the French reader now gets French
/// — and the fallback is checked with a locale that genuinely has no rows,
/// which is what the test was always about.
#[tokio::test]
async fn a_guide_reaches_a_reader_in_every_locale() {
    let app = TestApp::spawn().await;

    let french: serde_json::Value = app
        .get_with_header(
            "/api/guides?domain=communication&kind=onboarding",
            "accept-language",
            "fr-FR,fr;q=0.9",
        )
        .await
        .json()
        .await
        .unwrap();
    let guides = french["data"].as_array().unwrap();
    assert_eq!(
        guides.len(),
        4,
        "one onboarding guide per review family, whatever locale the reader asked for"
    );
    assert!(
        guides.iter().all(|g| g["locale"] == "fr"),
        "French exists for every one of these since 0535: {guides:?}"
    );

    // Arabic has none. The list must still hold four rather than look empty,
    // because a half-translated catalogue that answers nothing reads as a
    // domain with no guides at all.
    let arabic: serde_json::Value = app
        .get_with_header(
            "/api/guides?domain=communication&kind=onboarding",
            "accept-language",
            "ar",
        )
        .await
        .json()
        .await
        .unwrap();
    let fallback = arabic["data"].as_array().unwrap();
    assert_eq!(fallback.len(), 4);
    assert!(fallback.iter().all(|g| g["locale"] == "en"));
}

/// The opportunities board is public to read and curated to write.
#[tokio::test]
async fn only_a_curator_puts_an_opportunity_on_the_board() {
    let app = TestApp::spawn().await;
    app.register_user("notacurator").await;

    let body = json!({
        "slug": "someconf-2027-cfp",
        "kind": "conference_cfp",
        "skill_domain": "communication",
        "title": "SomeConf 2027 — call for papers",
        "organisation": "SomeConf",
        "url": "https://example.test/cfp",
        "is_remote": true,
    });

    let refused = app.post("/api/opportunities", &body).await;
    assert_eq!(refused.status(), 403);

    let curator: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'notacurator'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    grant(&app, curator, "domain_curator:communication").await;

    let accepted = app.post("/api/opportunities", &body).await;
    assert_eq!(accepted.status(), 200);

    // Public, because an opportunity only the people already here can read is
    // an opportunity that reaches nobody new.
    let listed = app.get("/api/opportunities?domain=communication").await;
    assert_eq!(listed.status(), 200);
    let listing: serde_json::Value = listed.json().await.unwrap();
    assert_eq!(listing["data"][0]["slug"], "someconf-2027-cfp");

    // Taking it down says why, and does not delete it.
    let withdrawn = app
        .delete_with_body(
            &format!(
                "/api/opportunities/{}",
                listing["data"][0]["id"].as_str().unwrap()
            ),
            &json!({ "reason": "the deadline passed" }),
        )
        .await;
    assert_eq!(withdrawn.status(), 200);

    let after = app.get("/api/opportunities?domain=communication").await;
    let after_body: serde_json::Value = after.json().await.unwrap();
    assert!(after_body["data"].as_array().unwrap().is_empty());

    let still_there: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_opportunities WHERE withdrawn_at IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        still_there, 1,
        "a withdrawn opportunity is kept, not deleted"
    );
}

/// A communication attestation counts towards the platform rank like any other.
///
/// Ticket F-07 asked for a check that it does. It does because the rank
/// counts attestations without looking at what they rest on — which is the
/// right design and the kind of thing that gets broken by somebody adding a
/// domain filter for a reason that seemed good at the time.
#[tokio::test]
async fn the_rank_counts_communication_attestations_like_any_other() {
    let app = TestApp::spawn().await;
    let person = user_id(&app, "commranker").await;

    let before = skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, person)
        .await
        .unwrap();
    assert_eq!(before.1, "apprenti");

    // Eleven verified deliverables and one attestation is `artisan`, whatever
    // domain either of them came from.
    for _ in 0..11 {
        common::delivered_in(&app, person, "communication", "documentation").await;
    }

    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code, basis,
             linked_deliverable_ids)
         SELECT $1, 'artefact', 'x', 'y', 'RANKTEST01', 'communication_docs_contribution',
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
        "a communication attestation did not count towards the rank"
    );
}
// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

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

/// A registered account, by username, for a fixture that needs an owner.
async fn user_id(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .expect("registered user")
}

/// A verified translation delivered by `author`, targeting one language.
async fn seed_translation(app: &TestApp, author: Uuid, language: &str) -> Uuid {
    let project = seed_project(app, "translation", author).await;

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, communication_subtype, communication_source_language,
             communication_target_languages, pr_url,
             orientation_id)
         VALUES ($1, 'communication_artifact', 'Traduction', 'x', 'communication', 3,
                 'validated', 'translation', 'en', ARRAY[$2], 'https://example.test/pr/1',
                 (SELECT id FROM orientations WHERE slug = 'technical-translator'))
         RETURNING id",
    )
    .bind(project)
    .bind(language)
    .fetch_one(&app.db)
    .await
    .expect("translation slice");

    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, public)
         VALUES ($1, $2, 'other', 'https://example.test/pr/1', 'human_review',
                 'verified', NOW(), TRUE)",
    )
    .bind(slice)
    .bind(author)
    .execute(&app.db)
    .await
    .expect("verified deliverable");

    slice
}

async fn grant(app: &TestApp, user_id: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test')
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap_or_else(|e| panic!("{capability}: {e}"));
}
