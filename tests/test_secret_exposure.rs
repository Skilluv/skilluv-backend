//! SE-02 -- the .env is not reachable at runtime.
//!
//! Even with zero secrets in the tree, the running process holds them. This
//! closes the runtime surfaces, each as a regression test so a future change
//! that reopens one is caught in CI, not by a scanner months later.
//!
//! Covered here (classes 1-4 of the ticket): direct file fetch, path traversal
//! on the proof download, the SSRF allowlist, and error-body leakage. Classes 5
//! (swagger is env-gated -- default-on in the test env, off in prod by
//! SKILLUV_HIDE_SWAGGER) and 6 (unsigned object-store access) are config- and
//! infra-level and verified against the mirror, not from this process.

mod common;

use common::TestApp;
use serde_json::json;
use skilluv_backend::services::design_auto_checks::fetchable;

/// Class 1 -- no route serves a file off disk; every config path is a 404.
#[tokio::test]
async fn config_files_are_not_served() {
    let app = TestApp::spawn().await;
    for path in [
        "/.env",
        "/.env.local",
        "/config",
        "/app/.env",
        "/.git/config",
        "/Cargo.toml",
        "/api/../.env",
    ] {
        let status = app.get(path).await.status();
        assert_eq!(
            status.as_u16(),
            404,
            "{path} did not 404 (returned {status}) -- is something serving files?"
        );
    }
}

/// Class 2 -- the proof download refuses a traversal key before it ever
/// resolves a path. The handler requires a `security-proofs/` prefix and no
/// `..`, so `../../.env` cannot escape the bucket prefix.
#[tokio::test]
async fn proof_download_refuses_path_traversal() {
    let app = TestApp::spawn().await;
    app.register_user("traversal_user").await;
    app.login("traversal_user").await;

    for key in [
        "../../.env",
        "..%2f..%2f.env",
        "security-proofs/../../../.env",
        "/etc/passwd",
        "security-proofs/../secret",
    ] {
        let resp = app.get(&format!("/api/security/proofs?key={key}")).await;
        assert_ne!(
            resp.status().as_u16(),
            200,
            "traversal key {key:?} was accepted"
        );
        let body = resp.text().await.unwrap_or_default();
        assert!(
            !body.contains("DATABASE_URL") && !body.contains("JWT_SECRET"),
            "the proof download leaked file content for {key:?}"
        );
    }
}

/// Class 3 -- the SSRF allowlist. `fetchable` is the single gate on any URL the
/// design auto-checks will retrieve: only https + a known extension passes, so
/// file://, the cloud metadata endpoint, plain http and localhost are all
/// refused before a request is made.
#[test]
fn ssrf_allowlist_refuses_dangerous_urls() {
    for url in [
        "file:///etc/environ",
        "file:///proc/self/environ",
        "http://169.254.169.254/latest/meta-data/",
        "http://169.254.169.254/latest.json",
        "http://localhost:3001/x.svg",
        "http://127.0.0.1/a.json",
        "https://example.com/notallowed.txt",
        "gopher://x",
    ] {
        assert!(fetchable(url).is_none(), "SSRF allowlist accepted {url}");
    }
    // The one shape it is meant to accept.
    assert!(fetchable("https://cdn.example.com/artifact.svg").is_some());
    assert!(fetchable("https://cdn.example.com/tokens.json").is_some());
}

/// Class 4 -- an error body never carries a secret or a stack trace. AppError
/// renders a structured JSON code, not the panic/query/connection detail behind
/// it. Force a validation error and assert the body is clean.
#[tokio::test]
async fn error_bodies_do_not_leak_secrets_or_stack_traces() {
    let app = TestApp::spawn().await;
    // A malformed registration triggers a 4xx with a structured error body.
    let resp = app
        .post("/api/auth/register", &json!({ "email": "x" }))
        .await;
    assert!(
        resp.status().is_client_error(),
        "expected a 4xx, got {}",
        resp.status()
    );
    let body = resp.text().await.unwrap_or_default();
    for needle in [
        "postgres://",
        "redis://",
        "JWT_SECRET",
        "password_hash",
        "panicked at",
        "src/",
        ".rs:",
        "/home/",
        "Traceback",
    ] {
        assert!(
            !body.contains(needle),
            "error body leaked {needle:?}: {body}"
        );
    }
}
