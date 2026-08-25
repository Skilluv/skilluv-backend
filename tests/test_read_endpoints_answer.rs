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
///
/// The dev helper is here for a different reason. It reads a pending email
/// verification token out of Redis so end-to-end tooling can follow the link
/// without a mailbox, it answers only while `SKILLUV_DEV_MODE=true`, and the
/// config layer refuses to boot with that set in production. Publishing it
/// would advertise an email-ownership bypass as part of the product.
const NOT_PART_OF_THE_API: &[&str] = &[
    "/.well-known/security.txt",
    "/dev/verify-tokens/{email}",
    "/health/live",
    "/manifest.webmanifest",
    "/metrics",
    "/security.txt",
];

/// Every route the router registers is in the published document.
///
/// The gap the derived sweep moved rather than closed. A route that exists and
/// is undocumented is invisible to this file, to the OpenAPI consumers and to
/// the front end — and adding one is a single line in a `Router`, with nothing
/// asking for the `#[utoipa::path]` that should come with it.
///
/// This held a list of eighty-five inherited exceptions: sixty-two reads and
/// twenty-three writes, thirty-two of which had an annotation nobody had
/// registered in `src/openapi.rs`. They are all closed, so the list is gone
/// and there is nothing to add to. Being undocumented is the condition that
/// let `/users/me/performance` answer 500 to every signed-in caller for as
/// long as it did.
///
/// Every method, not only the reads. A `POST` nobody documented is worse than
/// a `GET` nobody documented: it changes state, and whoever has to call it is
/// guessing the body.
///
/// Read from the source rather than from the router, because `axum::Router`
/// cannot be enumerated. That is a blunt instrument and it is the only one
/// available; it stays narrow by only ever reading `.route("…", …method(…)…)`
/// literals, which is how every route in this codebase is written.
#[tokio::test]
async fn an_undocumented_route_is_not_a_hidden_one() {
    let doc = ApiDoc::openapi();
    let documented: std::collections::HashSet<(&str, String)> = doc
        .paths
        .paths
        .iter()
        .flat_map(|(path, item)| {
            [
                ("get", item.get.is_some()),
                ("post", item.post.is_some()),
                ("put", item.put.is_some()),
                ("patch", item.patch.is_some()),
                ("delete", item.delete.is_some()),
            ]
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(method, _)| (method, path.clone()))
            .collect::<Vec<_>>()
        })
        .collect();

    let mut undocumented: Vec<(String, String)> = Vec::new();

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    for entry in std::fs::read_dir(&dir).expect("src/routes is readable") {
        let file = entry.expect("a directory entry").path();
        if file.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&file).expect("a readable module");

        // Split on the call rather than reading line by line: the literal
        // sometimes sits on the line below, and a scanner that assumes one
        // shape quietly stops seeing a whole module's routes.
        for piece in source.split(".route(").skip(1) {
            let Some(rest) = piece.trim_start().strip_prefix('"') else {
                continue;
            };
            let Some(route) = rest.split('"').next() else {
                continue;
            };
            // The methods a route is served with are named just after its
            // literal. Bounded, because the piece runs to the next `.route(`
            // — for the last route in a module that is everything left in the
            // file, and every `get(` in a handler body would read as a route.
            // Counted in characters, not bytes: these modules are separated
            // by box-drawing banners, and a byte offset lands inside one.
            let tail: String = rest[route.len()..].chars().take(200).collect();

            for method in ["get", "post", "put", "patch", "delete"] {
                if !tail.contains(&format!("{method}(")) {
                    continue;
                }
                // Three nesting styles coexist. Most routers are nested under
                // `/api`, so their literal is a suffix. `well_known_routes`
                // and `metrics_routes` are merged at the root and write the
                // prefix out, so their literal is already absolute. Both are
                // accepted, which is why this compares two candidates.
                let nested = format!("/api{route}");
                if documented.contains(&(method, route.to_string()))
                    || documented.contains(&(method, nested))
                {
                    continue;
                }
                undocumented.push((
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    format!("{} {route}", method.to_uppercase()),
                ));
            }
        }
    }

    undocumented.sort();
    undocumented.dedup();

    let known: std::collections::HashSet<&str> = NOT_PART_OF_THE_API.iter().copied().collect();

    let fresh: Vec<String> = undocumented
        .iter()
        // The findings carry their method, the exemptions do not: nothing is
        // exempt for one verb and expected for another.
        .filter(|(_, route)| {
            !known.contains(route.split_once(' ').map_or(route.as_str(), |(_, p)| p))
        })
        .map(|(file, route)| format!("{file}: {route}"))
        .collect();

    assert!(
        fresh.is_empty(),
        "{} route(s) were added without an OpenAPI entry, so nothing sweeps them \
         and no client knows they exist — add the `#[utoipa::path]` and register the \
         handler in `src/openapi.rs`:\n{}",
        fresh.len(),
        fresh.join("\n")
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

/// Every path `docs/API-ROUTES.md` names is a path the service actually serves.
///
/// The reference is hand-written and covers a fraction of the surface on
/// purpose, so it is never checked for completeness. It is checked for
/// truthfulness: a front-end developer reading it should not be sent to a
/// route that was renamed or merged away. It named three wallet payout
/// endpoints — `/withdraw/stripe`, `/withdraw/momo` and `/onboard/stripe` —
/// for long after the rails were unified behind one `/withdraw`.
#[test]
fn every_route_the_reference_names_exists() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // Every routed literal, whatever the method. `src/websocket` holds `/ws`,
    // which is a route like any other even though nothing sweeps an upgrade.
    let mut served: std::collections::HashSet<String> = std::collections::HashSet::new();
    for sub in ["src/routes", "src/websocket"] {
        for entry in std::fs::read_dir(root.join(sub)).expect("a readable module directory") {
            let file = entry.expect("a directory entry").path();
            if file.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&file).expect("a readable module");
            // Neither anchored to the start of a line nor to the same line:
            // `Router::new().route(…)`, chained `.route(…).route(…)` and a
            // call whose literal sits on the next line are all written here,
            // and a scanner that assumes one shape silently misses the rest.
            for piece in source.split(".route(").skip(1) {
                let Some(rest) = piece.trim_start().strip_prefix('"') else {
                    continue;
                };
                if let Some(route) = rest.split('"').next() {
                    served.insert(route.to_string());
                }
            }
        }
    }

    // `src/openapi.rs` serves the document itself, and mounts Swagger UI as a
    // whole subtree rather than route by route — so its mount point never
    // appears as a `.route(` literal. Both are served paths, and the reference
    // sends readers to both.
    let openapi = std::fs::read_to_string(root.join("src/openapi.rs")).expect("readable");
    for piece in openapi.split(".route(").skip(1) {
        if let Some(route) = piece
            .trim_start()
            .strip_prefix('"')
            .and_then(|rest| rest.split('"').next())
        {
            served.insert(route.to_string());
        }
    }
    for piece in openapi.split("SwaggerUi::new(\"").skip(1) {
        if let Some(mount) = piece.split('"').next() {
            served.insert(mount.to_string());
        }
    }

    let reference = std::fs::read_to_string(root.join("docs/API-ROUTES.md"))
        .expect("the reference is readable");

    let mut ghosts: Vec<String> = Vec::new();
    for chunk in reference.split('`').skip(1).step_by(2) {
        // Only the path-shaped ones. Prose in backticks, field names and
        // JSON fragments are not paths and must not be read as claims.
        if !chunk.starts_with('/') || chunk.contains(' ') || chunk.contains("...") {
            continue;
        }
        let path = chunk.split('?').next().unwrap_or(chunk);
        let bare = path.strip_prefix("/api").unwrap_or(path);
        if served.contains(path) || served.contains(bare) {
            continue;
        }
        ghosts.push(path.to_string());
    }

    ghosts.sort();
    ghosts.dedup();

    assert!(
        ghosts.is_empty(),
        "{} path(s) in docs/API-ROUTES.md are not served by anything. Either the \
         reference is out of date or the route was lost:\n{}",
        ghosts.len(),
        ghosts.join("\n")
    );
}
