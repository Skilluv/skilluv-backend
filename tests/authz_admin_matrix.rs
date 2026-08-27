//! AZ-01 (runtime half) — every admin route refuses a non-admin.
//!
//! The static gate (scripts/check-admin-guards.py) proves each admin handler
//! *has* a guard; this proves the guard *works*, end to end, against the
//! running app. It is generated from the source of truth: it fetches
//! `/api/openapi.json`, enumerates every documented `/admin/*` (path, method),
//! and sends each one as two actors who must never be let in — a logged-in
//! non-admin and an anonymous client — asserting none receives a 2xx.
//!
//! The test client carries `Origin: http://localhost:5174` by default (see
//! tests/common/mod.rs), a dev admin origin, so `AdminGate`'s origin check
//! passes for the logged-in actor and the *authorization* is what is under
//! test, not the origin. The anonymous actor uses a bare client with no
//! session and no admin origin, so it is refused at the gate or at `AuthUser`.
//!
//! It collects every violation before asserting, so one run is a full audit
//! rather than a stop at the first. A failure lists each admin route a
//! non-admin reached: each is either an escalation to fix or an intentional
//! exception to record in `ALLOW` below with a reason.
//!
//! Coverage note: this sees only routes carrying a `#[utoipa::path]`
//! annotation (what the spec contains). The static gate covers every
//! `.route()` registration; the two together are the matrix.

mod common;

use common::TestApp;
use serde_json::{Value, json};

/// Admin routes a non-admin may reach on purpose. Empty by design: add an
/// entry only with a written reason, after deciding the route is meant to be
/// open. Format: ("METHOD", "/api/admin/...").
const ALLOW: &[(&str, &str)] = &[];

const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];
const DUMMY_ID: &str = "00000000-0000-0000-0000-000000000001";

fn concrete_path(template: &str) -> String {
    // Replace every {param} with a value that forms a valid URL segment. The
    // handler may then 404 on a missing row, which is still not a 2xx — and
    // authorization runs before the lookup anyway.
    let mut out = String::with_capacity(template.len());
    let mut in_param = false;
    for c in template.chars() {
        match c {
            '{' => in_param = true,
            '}' => {
                in_param = false;
                out.push_str(DUMMY_ID);
            }
            _ if !in_param => out.push(c),
            _ => {}
        }
    }
    out
}

fn admin_paths(spec: &Value) -> Vec<(String, String)> {
    // -> Vec<(method, path_template)> for every documented /admin/ operation.
    let mut out = Vec::new();
    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .expect("openapi spec has paths");
    for (path, item) in paths {
        if !path.contains("/admin") {
            continue;
        }
        for m in METHODS {
            if item.get(m).is_some() {
                out.push((m.to_string(), path.clone()));
            }
        }
    }
    out
}

#[tokio::test]
async fn no_non_admin_reaches_any_admin_route() {
    let app = TestApp::spawn().await;

    // A plain, verified, logged-in user — not an admin, no capabilities.
    app.register_user("az_matrix_user").await;
    app.login("az_matrix_user").await;

    let spec: Value = app
        .get("/api/openapi.json")
        .await
        .json()
        .await
        .expect("openapi.json is JSON");

    let routes = admin_paths(&spec);
    assert!(
        routes.len() > 20,
        "expected many admin routes in the spec, found {}",
        routes.len()
    );

    // Anonymous actor: a bare client, no cookie jar, no admin origin.
    let anon = reqwest::Client::new();

    let mut reached = Vec::new();
    for (method, template) in &routes {
        if ALLOW.contains(&(method.as_str(), template.as_str())) {
            continue;
        }
        let path = concrete_path(template);
        let url = format!("{}{}", app.addr, path);

        // (1) logged-in non-admin, via the shared client.
        let body = json!({});
        let resp = match method.as_str() {
            "get" => app.get(&path).await,
            "post" => app.post(&path, &body).await,
            "put" => app.put(&path, &body).await,
            "patch" => app.patch(&path, &body).await,
            "delete" => app.delete(&path).await,
            _ => unreachable!(),
        };
        if resp.status().is_success() {
            reached.push(format!(
                "non-admin {} {} -> {}",
                method,
                template,
                resp.status()
            ));
        }

        // (2) anonymous, via the bare client.
        let req = match method.as_str() {
            "get" => anon.get(&url),
            "post" => anon.post(&url).json(&body),
            "put" => anon.put(&url).json(&body),
            "patch" => anon.patch(&url).json(&body),
            "delete" => anon.delete(&url),
            _ => unreachable!(),
        };
        let resp = req.send().await.expect("anon request sent");
        if resp.status().is_success() {
            reached.push(format!(
                "anonymous {} {} -> {}",
                method,
                template,
                resp.status()
            ));
        }
    }

    assert!(
        reached.is_empty(),
        "{} admin route(s) let a non-admin in — each is an escalation to fix or an \
         intentional exception to record in ALLOW:\n  {}",
        reached.len(),
        reached.join("\n  ")
    );
}
