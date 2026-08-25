//! Every documented read endpoint answers something, to everybody.
//!
//! `GET /users/{username}/ops-profile` returned 500 to every request it had
//! ever received: one figure in its query was cast to `INT` while the struct
//! read every column as `i64`, so sqlx refused to decode the row. It survived
//! because exactly one test reached it, incidentally, on its way to checking
//! something else — a wholly dead endpoint and one awkward test look identical
//! from outside.
//!
//! ## Why the list is no longer written down
//!
//! This file used to carry a hand-maintained list of a hundred and
//! ninety-eight paths: the GETs somebody had noticed nothing else calls. An
//! audit had found 404 of 922 registered routes that no test touched, and a
//! list was the affordable answer at the time.
//!
//! It had the failure mode it was written to catch. A route added and not
//! added to the list is invisible again, and nothing says so.
//!
//! So the list is derived from `ApiDoc`, the OpenAPI document the platform
//! publishes. Every documented GET is called, with its path parameters filled
//! from the types the document itself declares. Four hundred and six paths
//! rather than a hundred and ninety-eight, and the next one is covered the day
//! it is documented rather than the day somebody remembers.
//!
//! That moves the drift somewhere better rather than removing it: a route in
//! the router and absent from `openapi.rs` is still uncovered. It is a
//! narrower and louder gap — an undocumented public endpoint is its own
//! problem, whatever this file does — and
//! `an_undocumented_route_is_not_a_hidden_one` is what reports it.
//!
//! ## What is asserted
//!
//! The *shape* of the answer, not its content — so these stay true as the
//! endpoints grow, and fail only for the reason worth failing on.
//! Unauthenticated is 401, forbidden is 403, absent is 404, and an integration
//! this deployment does not have is 503. A 500 to a well-formed request is
//! none of those: it is what a query that has stopped decoding produces.
//!
//! The three doors matter, because an endpoint can decode fine on the path
//! that refuses you and fail on the one that reads rows. The unauthenticated
//! pass alone would not have caught the ops profile, and it did not catch
//! `/users/me/performance` either — that answered 500 to every signed-in
//! caller because an absent AI worker was being reported as a server fault.

mod common;
use common::TestApp;

use skilluv_backend::openapi::ApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::path::ParameterIn;

/// An id nothing owns, for the path parameters that are UUIDs.
///
/// A handler naming a column that does not exist fails when the query runs,
/// not when a row matches, so a stranger's id still reaches the failure. 404
/// is the right answer here and 500 is not: that is what
/// `/api/enterprises/me/agency-clients` was doing, and it took an audit rather
/// than a user to notice.
const NO_SUCH_ID: &str = "00000000-0000-4000-8000-000000000000";

/// The same, for the ones that are slugs, usernames or free strings.
const NO_SUCH_SLUG: &str = "nothing-by-this-name";

/// Every documented GET, with its path parameters filled in.
///
/// The placeholder comes from the parameter's own schema, so a route that
/// declares a UUID gets a UUID and one that declares a slug gets a slug.
/// Guessing from the parameter *name* would work until the first `{id}` that
/// is a slug, and `/admin/missions/{slug}` and `/admin/users/{id}` show the
/// two conventions already coexist.
fn documented_read_endpoints() -> Vec<String> {
    let doc = ApiDoc::openapi();
    let mut paths = Vec::new();

    for (path, item) in doc.paths.paths.iter() {
        let Some(operation) = item.get.as_ref() else {
            continue;
        };

        let mut concrete = path.clone();
        if let Some(parameters) = operation.parameters.as_ref() {
            for parameter in parameters {
                if !matches!(parameter.parameter_in, ParameterIn::Path) {
                    continue;
                }
                let schema = parameter
                    .schema
                    .as_ref()
                    .map(|s| serde_json::to_string(s).unwrap_or_default())
                    .unwrap_or_default();

                let placeholder = if schema.contains("\"uuid\"") {
                    NO_SUCH_ID
                } else if schema.contains("\"integer\"") {
                    "1"
                } else {
                    NO_SUCH_SLUG
                };
                concrete = concrete.replace(&format!("{{{}}}", parameter.name), placeholder);
            }
        }

        // A segment the document declares nothing for. Left out of the sweep
        // rather than called with a brace in the URL, and reported by
        // `every_path_parameter_is_described` so it is fixed rather than
        // silently skipped.
        if !concrete.contains('{') {
            paths.push(concrete);
        }
    }

    paths.sort();
    paths
}

/// Documented GETs whose path segments the document does not describe.
fn paths_with_undescribed_segments() -> Vec<String> {
    ApiDoc::openapi()
        .paths
        .paths
        .iter()
        .filter(|(path, item)| item.get.is_some() && path.contains('{'))
        .filter(|(path, item)| {
            let operation = item.get.as_ref().expect("filtered on being present");
            let described: Vec<&str> = operation
                .parameters
                .as_ref()
                .map(|params| {
                    params
                        .iter()
                        .filter(|p| matches!(p.parameter_in, ParameterIn::Path))
                        .map(|p| p.name.as_str())
                        .collect()
                })
                .unwrap_or_default();

            // Every `{segment}` in the path has to have a parameter of that
            // name. One missing is enough.
            path.split('{')
                .skip(1)
                .filter_map(|rest| rest.split('}').next())
                .any(|segment| !described.contains(&segment))
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// A segment in the path and nothing describing it.
///
/// The sweep cannot fill those, so the path drops out of the coverage without
/// anybody noticing — which is the failure this whole file exists to stop. It
/// is also a real defect in the published document: a client generator reading
/// it produces a method with no argument for the segment.
#[tokio::test]
async fn every_path_parameter_is_described() {
    let undescribed = paths_with_undescribed_segments();
    assert!(
        undescribed.is_empty(),
        "these paths take a segment the OpenAPI document does not declare, so the \
         sweep cannot call them and no generated client can either: {undescribed:?}"
    );
}

/// Registered outside the API surface on purpose, and documented nowhere
/// because they are not part of it.
///
/// A Prometheus scrape endpoint, an RFC 9116 contact file, a PWA manifest and
/// a liveness probe. Each is a contract with something other than a client of
/// this API, and putting them in the OpenAPI document would say they are part
/// of it.
const NOT_PART_OF_THE_API: &[&str] = &[
    "/.well-known/security.txt",
    "/health/live",
    "/manifest.webmanifest",
    "/metrics",
    "/security.txt",
];

/// GET routes that exist and are not in the OpenAPI document.
///
/// Inherited, counted, allowed to shrink and never to grow. Each one is a
/// public endpoint nothing sweeps, no generated client knows about, and no
/// front-end developer can discover without reading `src/routes` — which is
/// the condition that let `/users/me/performance` answer 500 to every
/// signed-in caller for as long as it did.
///
/// The assertion below is two-way on purpose. A new undocumented route fails
/// it, and so does an entry here that has since been documented — so the list
/// cannot quietly stop describing the truth, in either direction.
///
/// Closing one is mechanical: a `#[utoipa::path]` on the handler and a line in
/// `src/openapi.rs`. They are not closed here because sixty-two of those in one
/// commit is a diff nobody reads, and this list is what makes doing them in
/// batches possible.
const UNDOCUMENTED_YET: &[&str] = &[
    "/admin/design/briefs",
    "/admin/domains/{domain}/overview",
    "/admin/domains/{domain}/reviewers",
    "/admin/feature-flags",
    "/admin/featured/{domain}/{week}/card",
    "/admin/plagiarism",
    "/admin/slices",
    "/admin/validators/collusion-matrix",
    "/admin/validators/stats",
    "/attestations/verify/{code}/card.png",
    "/beginner/verifications/mine",
    "/beginner/verifications/queue",
    "/code/languages/top",
    "/cohorts",
    "/cohorts/{id}",
    "/cohorts/{id}/members",
    "/contests/plagiarism/{id}",
    "/design/briefs/mine",
    "/design/cloud/connections",
    "/design/cloud/inspect",
    "/design/cloud/{provider}/start",
    "/design/mentors/for-me",
    "/design/slices/{id}/auto-checks",
    "/design/slices/{id}/compare",
    "/design/slices/{id}/versions/{round}",
    "/design/uploads/{id}/download-url",
    "/design/uploads/{id}/parts",
    "/enterprise/invoices/{id}/pdf",
    "/enterprise/invoices/{id}/preview",
    "/featured/{domain}",
    "/featured/{domain}/recent",
    "/maintainer-digest/confirm/{token}",
    "/maintainer-digest/unsubscribe/{token}",
    "/me/validation/queue",
    "/me/validator-applications",
    "/missions/{slug}/ratings",
    "/moderation/external-signals",
    "/peer-matching/proposals",
    "/projects/{slug}/active-skilluvers",
    "/series",
    "/series/{slug}",
    "/series/{slug}/standings",
    "/talent-offers",
    "/users/me/assistant-interactions",
    "/users/me/assistant-quota",
    "/users/me/bookmarks",
    "/users/me/bookmarks/folders",
    "/users/me/cohorts",
    "/users/me/external-signals",
    "/users/me/goals",
    "/users/me/next-challenges",
    "/users/me/notes",
    "/users/me/peer-matches",
    "/users/me/peer-matching/enrollments",
    "/users/me/talent-offers",
    "/users/me/vouchings",
    "/users/{user_id}/external-signals",
    "/users/{user_id}/vouchings",
    "/users/{username}/code-languages",
    "/users/{username}/mission-standing",
    "/verify/{hash}",
    "/verify/{hash}/pdf",
];

/// Every GET the router registers is in the published document.
///
/// The gap the derived sweep moved rather than closed. A route that exists and
/// is undocumented is invisible to this file, to the OpenAPI consumers and to
/// the front end — and adding one is a single line in a `Router`, with nothing
/// asking for the `#[utoipa::path]` that should come with it.
///
/// Read from the source rather than from the router, because `axum::Router`
/// cannot be enumerated. That is a blunt instrument and it is the only one
/// available; it stays narrow by only ever reading `.route("…", …get(…)…)`
/// literals, which is how every route in this codebase is written.
#[tokio::test]
async fn an_undocumented_route_is_not_a_hidden_one() {
    let documented: std::collections::HashSet<String> = ApiDoc::openapi()
        .paths
        .paths
        .iter()
        .filter(|(_, item)| item.get.is_some())
        .map(|(path, _)| path.clone())
        .collect();

    let mut undocumented: Vec<(String, String)> = Vec::new();

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    for entry in std::fs::read_dir(&dir).expect("src/routes is readable") {
        let file = entry.expect("a directory entry").path();
        if file.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&file).expect("a readable module");

        for line in source.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix(".route(\"") else {
                continue;
            };
            let Some(route) = rest.split('"').next() else {
                continue;
            };
            // Only the GETs. A `.route(path, post(...))` line carries no
            // `get(`, and a combined `get(x).post(y)` does.
            if !line.contains("get(") {
                continue;
            }
            // Three nesting styles coexist. Most routers are nested under
            // `/api`, so their literal is a suffix. `well_known_routes` and
            // `metrics_routes` are merged at the root and write the prefix
            // out, so their literal is already absolute. Both are accepted,
            // which is why this compares two candidates rather than one.
            let nested = format!("/api{route}");
            if !documented.contains(route) && !documented.contains(&nested) {
                undocumented.push((
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    route.to_string(),
                ));
            }
        }
    }

    undocumented.sort();
    undocumented.dedup();

    let known: std::collections::HashSet<&str> = UNDOCUMENTED_YET
        .iter()
        .chain(NOT_PART_OF_THE_API)
        .copied()
        .collect();

    let fresh: Vec<String> = undocumented
        .iter()
        .filter(|(_, route)| !known.contains(route.as_str()))
        .map(|(file, route)| format!("{file}: {route}"))
        .collect();

    assert!(
        fresh.is_empty(),
        "{} GET route(s) were added without an OpenAPI entry, so nothing sweeps them \
         and no client knows they exist — add the `#[utoipa::path]` and register the \
         handler in `src/openapi.rs`:\n{}",
        fresh.len(),
        fresh.join("\n")
    );

    // The other direction. An entry that has since been documented, or whose
    // route no longer exists, has to leave the list — otherwise the list stops
    // being a count of anything and starts being decoration.
    let still_undocumented: std::collections::HashSet<&str> =
        undocumented.iter().map(|(_, r)| r.as_str()).collect();
    let stale: Vec<&str> = UNDOCUMENTED_YET
        .iter()
        .filter(|r| !still_undocumented.contains(*r))
        .copied()
        .collect();

    assert!(
        stale.is_empty(),
        "{} route(s) in UNDOCUMENTED_YET are documented now, or gone. Remove them — \
         a debt list that overstates itself stops being read:\n{}",
        stale.len(),
        stale.join("\n")
    );
}

/// The three doors, because an endpoint can decode fine on the path that
/// refuses you and fail on the one that reads rows.
async fn every_endpoint_answers(app: &TestApp, who: &str) {
    let paths = documented_read_endpoints();

    // A floor rather than an exact count: the number grows with the platform,
    // and an assertion on the exact figure would be edited without being read.
    // A collapse to nothing is the failure worth catching — an empty sweep
    // passes silently, which is the shape of bug this file is about.
    assert!(
        paths.len() > 300,
        "the derived sweep collapsed to {} paths — something stopped the OpenAPI \
         document being built",
        paths.len()
    );

    let mut dead = Vec::new();

    for path in &paths {
        let resp = app.get(path).await;
        let status = resp.status().as_u16();
        // 503 is allowed and 500 is not, which is the whole distinction: an
        // integration this deployment does not have is unavailable, not
        // broken. Stripe is absent here and in CI, and four handlers used to
        // call that an internal error — telling a caller the server failed
        // when the honest answer is that payments were never configured. The
        // AI worker is absent for the same reason, and four more handlers
        // were saying the same thing about it.
        if status >= 500 && status != 503 {
            dead.push(format!(
                "{path} -> {status}: {}",
                resp.text().await.unwrap_or_default()
            ));
        }
    }

    assert!(
        dead.is_empty(),
        "{} endpoint(s) answered 5xx to {who}:\n{}",
        dead.len(),
        dead.join("\n")
    );
}

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_a_stranger() {
    let app = TestApp::spawn().await;
    every_endpoint_answers(&app, "a stranger").await;
}

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_a_signed_in_person() {
    let app = TestApp::spawn().await;
    app.register_user("reader_smoke").await;
    app.login("reader_smoke").await;
    every_endpoint_answers(&app, "a member").await;
}

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_an_admin() {
    let app = TestApp::spawn().await;
    app.register_admin("reader_smoke_admin").await;
    every_endpoint_answers(&app, "an admin").await;
}
