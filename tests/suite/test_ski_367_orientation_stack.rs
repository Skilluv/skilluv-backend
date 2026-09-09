//! SKI-367 - a trade says which tools it works with, as data.
//!
//! The signup card wants to draw tool logos. The information existed only in
//! `description` prose ("React, Vue ou Svelte, TypeScript, CSS moderne"), and
//! parsing that would have produced a card showing three right logos and a
//! fourth invented, with nothing to signal the error.

use crate::common::TestApp;

/// The stack arrives with the orientation, in the same call.
///
/// A second request per trade would undo the work done on
/// `/orientation-counts`, which exists so the signup flow can show eleven
/// domains without eleven round trips.
#[tokio::test]
async fn the_stack_arrives_with_the_orientation() {
    let app = TestApp::spawn().await;

    let body: serde_json::Value = app
        .get("/api/orientations?domain=code&limit=200")
        .await
        .json()
        .await
        .unwrap();

    let orientations = body["data"]["orientations"]
        .as_array()
        .expect("a catalogue");
    let frontend = orientations
        .iter()
        .find(|o| o["slug"] == "web-frontend-developer")
        .expect("the frontend trade is seeded");

    let stack: Vec<&str> = frontend["stack"]
        .as_array()
        .expect("stack is present on every row, empty or not")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert_eq!(
        stack,
        vec!["react", "vue", "svelte", "typescript", "css"],
        "the identifiers its own description names, in reading order"
    );

    // Every row carries the field, so a client never has to branch on its
    // absence - only on its emptiness.
    assert!(
        orientations.iter().all(|o| o["stack"].is_array()),
        "stack is absent from at least one row"
    );
}

/// An empty stack means "not recorded yet", and that is a legitimate answer.
///
/// Most trades are not filled: only those whose description names their tools
/// outright were, because an invented logo is worse than a missing one on the
/// screen where somebody chooses their trade.
#[tokio::test]
async fn a_trade_with_nothing_recorded_says_so_with_an_empty_stack() {
    let app = TestApp::spawn().await;

    let body: serde_json::Value = app
        .get("/api/orientations?domain=code&limit=200")
        .await
        .json()
        .await
        .unwrap();

    let orientations = body["data"]["orientations"].as_array().unwrap();
    let empty = orientations
        .iter()
        .filter(|o| o["stack"].as_array().unwrap().is_empty())
        .count();

    assert!(
        empty > 0,
        "the backfill was deliberately partial; if everything is filled, \
         somebody derived stacks from prose and that is what this forbids"
    );
}

/// Every identifier written is one the registry knows.
///
/// This is the property that makes the set verifiable rather than a
/// convention. It is enforced in the database by a trigger, because a foreign
/// key cannot reach inside an array; this asserts the enforcement is actually
/// attached and that the seeded data satisfies it.
#[tokio::test]
async fn no_orientation_names_a_tool_the_registry_does_not_hold() {
    let app = TestApp::spawn().await;

    let unknown: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT t
           FROM orientations o, unnest(o.stack) AS t
          WHERE NOT EXISTS (SELECT 1 FROM tools WHERE tools.id = t)
          ORDER BY t",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        unknown.is_empty(),
        "identifiers with no entry in `tools`: {unknown:?}"
    );
}

/// The trigger refuses a typo rather than letting it reach the signup screen.
#[tokio::test]
async fn an_unknown_identifier_is_refused_at_write_time() {
    let app = TestApp::spawn().await;

    let refused = sqlx::query(
        "UPDATE orientations SET stack = ARRAY['reakt'] WHERE slug = 'web-frontend-developer'",
    )
    .execute(&app.db)
    .await;

    let err = refused.expect_err("a misspelt tool must not be writable");
    let message = err.to_string();
    assert!(
        message.contains("reakt"),
        "the refusal has to name what it refused: {message}"
    );
}

/// A tool cannot be listed twice.
///
/// Not a correctness problem so much as a rendering one: the card would draw
/// the same logo twice and look broken, and nothing downstream would explain
/// why.
#[tokio::test]
async fn a_stack_cannot_repeat_a_tool() {
    let app = TestApp::spawn().await;

    let refused = sqlx::query(
        "UPDATE orientations SET stack = ARRAY['react','react'] \
         WHERE slug = 'web-frontend-developer'",
    )
    .execute(&app.db)
    .await;

    assert!(refused.is_err(), "a repeated tool must not be writable");
}

/// Identifiers are stable, which is the whole reason a client may map them.
///
/// A rename breaks every consumer silently, so the ones the frontend is being
/// asked to draw are pinned here. Changing a display name is free; changing an
/// id has to fail loudly, in CI, and not on somebody's signup screen.
#[tokio::test]
async fn the_identifiers_a_client_maps_to_logos_are_pinned() {
    let app = TestApp::spawn().await;

    for (id, display) in [
        ("react", "React"),
        ("typescript", "TypeScript"),
        ("postgresql", "PostgreSQL"),
        ("nodejs", "Node.js"),
        ("jetpack-compose", "Jetpack Compose"),
        ("tla-plus", "TLA+"),
    ] {
        let found: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM tools WHERE id = $1")
                .bind(id)
                .fetch_optional(&app.db)
                .await
                .unwrap();
        assert_eq!(
            found.as_deref(),
            Some(display),
            "`{id}` is an identifier clients map to a logo; it may not be renamed"
        );
    }
}

/// The backfill stops being code-only.
///
/// 0619 built the registry as a general thing and filled it for one domain, so
/// `/api/orientations` answered a developer with their languages and everybody
/// else with an empty array. Migration 0625 applies 0619's own rule to the
/// other eleven catalogues: a stack is written only where the orientation's
/// description names the tool outright.
///
/// Seven orientations qualified, which is few and is the point. This pins them
/// by slug, because the value of the rule is that it produced a short list.
#[tokio::test]
async fn the_stack_reaches_past_the_code_catalogue() {
    let app = TestApp::spawn().await;

    let expected: [(&str, &[&str]); 7] = [
        ("design-motion-ui", &["lottie", "rive"]),
        ("design-motion-3d", &["cinema-4d", "blender"]),
        ("audio-music-implementer", &["fmod", "wwise"]),
        ("game-engine-programmer", &["godot", "bevy"]),
        ("game-animator-3d", &["unity", "unreal-engine"]),
        ("game-vfx-artist", &["godot"]),
        ("kubernetes-specialist", &["kubernetes"]),
    ];

    for (slug, tools) in expected {
        let stack: Vec<String> =
            sqlx::query_scalar("SELECT stack FROM orientations WHERE slug = $1")
                .bind(slug)
                .fetch_one(&app.db)
                .await
                .unwrap_or_else(|e| panic!("{slug} is not in the catalogue: {e}"));
        assert_eq!(
            stack, tools,
            "{slug} does not carry the tools its own description names"
        );
    }
}

/// Nothing was written for a trade whose description names no tool.
///
/// `quality` mentions Playwright and `ops` mentions Terraform and Prometheus,
/// but only in the migrations' comments about artefact kinds. Reading those as
/// stacks would be the invention the rule exists to refuse, and the refusal is
/// worth a test because it is the tempting half of the work.
#[tokio::test]
async fn a_tool_named_only_in_a_comment_is_not_a_stack() {
    let app = TestApp::spawn().await;

    let leaked: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o, unnest(o.stack) AS t
          WHERE t IN ('playwright', 'terraform', 'prometheus', 'pulumi', 'helm')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        leaked.is_empty(),
        "these stacks were derived from a migration comment, not from a trade's \
         own description: {leaked:?}"
    );
}
