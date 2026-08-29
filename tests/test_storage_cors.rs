//! The object store must let a browser read the `ETag` (SKI-309).
//!
//! ## The failure this prevents
//!
//! `POST /design/uploads/{id}/complete` wants the `ETag` the store returned
//! for each uploaded part. The PUT goes from a browser straight to the store,
//! so it is cross-origin, and a browser can only read a response header the
//! server has named in `Access-Control-Expose-Headers`.
//!
//! Without it, every part uploads perfectly, the store keeps them all, and
//! `complete` can never be called. The symptom is a failure at the **end** of
//! a five-gigabyte upload, on a file that has nothing wrong with it — the most
//! expensive kind to diagnose and the most discouraging to sit through.
//!
//! ## Why this asserts behaviour and not configuration
//!
//! MinIO needs no setting: it answers with a fixed expose list that already
//! contains `Etag`. Real S3 needs a bucket CORS rule, because it exposes
//! nothing by default. A test that checked for the rule would fail on MinIO
//! where nothing is wrong; a test that checks what the store actually answers
//! holds across both, and keeps holding on the day this moves off MinIO —
//! which is the day the rule stops being optional.

use std::time::Duration;

/// Where the store is, in the shape CI and a laptop both have it.
fn endpoint() -> String {
    std::env::var("MINIO_ENDPOINT").unwrap_or_else(|_| "http://localhost:9004".to_string())
}

#[tokio::test]
async fn etag_is_exposed_to_browsers() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();

    // Any path will do, and an unauthenticated one is deliberate: CORS headers
    // are attached before authorisation is decided, so a 403 carries them just
    // as a 200 would. Asking for a real object would need credentials this
    // test has no reason to hold.
    let response = match client
        .get(format!("{}/documents/", endpoint()))
        .header("Origin", "https://skill-uv.com")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // No store running. Skipping rather than failing: this suite is
            // about a deployment property, and a laptop without MinIO up is
            // not a broken deployment.
            eprintln!(
                "object store unreachable at {} ({e}) — skipping",
                endpoint()
            );
            return;
        }
    };

    let exposed = response
        .headers()
        .get("access-control-expose-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_lowercase();

    assert!(
        !exposed.is_empty(),
        "the store named no exposed headers at all, so a browser can read none \
         of them. Every browser multipart upload will fail at `complete`, after \
         the whole file has been transferred. On S3, add a bucket CORS rule \
         with ExposeHeaders: [\"ETag\"]."
    );

    // A `*` is enough — it exposes everything, which includes the ETag. MinIO
    // sends both the wildcard and the name; S3 with a rule sends the name.
    assert!(
        exposed.contains("etag") || exposed.contains('*'),
        "`ETag` is not exposed: {exposed}\n\
         Browser multipart uploads cannot complete. See services::storage."
    );
}

/// The other half of the same requirement: the browser has to be allowed to
/// send the PUT in the first place.
///
/// Separate from the test above because they fail differently and at different
/// moments — this one fails on the *first* part, which is at least fast and
/// legible. It is here so that a store configured to expose the ETag but not
/// to accept the method does not read as "CORS is fine".
#[tokio::test]
async fn a_browser_is_allowed_to_put_a_part() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();

    let response = match client
        .request(
            reqwest::Method::OPTIONS,
            format!("{}/documents/probe", endpoint()),
        )
        .header("Origin", "https://skill-uv.com")
        .header("Access-Control-Request-Method", "PUT")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "object store unreachable at {} ({e}) — skipping",
                endpoint()
            );
            return;
        }
    };

    let allowed = response
        .headers()
        .get("access-control-allow-methods")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_uppercase();

    assert!(
        allowed.contains("PUT"),
        "the store refuses a cross-origin PUT (allow-methods: {allowed:?}), so no \
         browser upload can start"
    );
}
