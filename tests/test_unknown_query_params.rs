//! A query parameter nobody declared is refused, not ignored.
//!
//! Every listing endpoint carries `#[serde(deny_unknown_fields)]`, and the
//! OpenAPI document says the same thing by listing exactly the parameters that
//! exist. The two have to agree, and nothing was checking that they did: an
//! endpoint that silently drops an unknown filter answers 200 with a full list
//! to somebody who thinks they narrowed it, which is the worst of the three
//! possible behaviours.
//!
//! The contract fuzzer flagged `GET /api/ai/competitions` for exactly this and
//! the simplified repro it printed did not reproduce. These tests are the
//! direct question, on the endpoints where a dropped filter would be
//! misread as an empty or an unfiltered result.

mod common;
use common::TestApp;

/// Endpoints whose query struct denies unknown fields, with a valid query to
/// hang the unknown parameter off.
const LISTINGS: &[&str] = &[
    "/api/ai/competitions?include_closed=false",
    "/api/missions?limit=5",
    "/api/feed/public?limit=5",
    "/api/talents/search?limit=5",
];

#[tokio::test]
async fn an_undeclared_query_parameter_is_refused() {
    let app = TestApp::spawn().await;

    for base in LISTINGS {
        let sep = if base.contains('?') { '&' } else { '?' };
        let path = format!("{base}{sep}nosuchfilter=1");
        let resp = app.get(&path).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "GET {path} accepted a parameter it does not have: {:?}",
            resp.text().await
        );
    }
}

#[tokio::test]
async fn an_empty_named_parameter_is_refused_too() {
    let app = TestApp::spawn().await;

    // `?=1` parses to a field whose name is the empty string. It is not a
    // parameter any of these endpoints has, so it is refused like any other —
    // and it is the shape a fuzzer reaches for first.
    for base in LISTINGS {
        let sep = if base.contains('?') { '&' } else { '?' };
        let path = format!("{base}{sep}=1");
        let resp = app.get(&path).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "GET {path} accepted an unnamed parameter: {:?}",
            resp.text().await
        );
    }
}

#[tokio::test]
async fn the_declared_parameters_are_still_accepted() {
    let app = TestApp::spawn().await;

    // The other half of the same claim: refusing the undeclared must not come
    // from refusing everything.
    for base in LISTINGS {
        let resp = app.get(base).await;
        assert_eq!(
            resp.status().as_u16(),
            200,
            "GET {base} refused its own parameters: {:?}",
            resp.text().await
        );
    }
}

#[tokio::test]
async fn a_nul_byte_is_a_client_error_not_a_server_one() {
    let app = TestApp::spawn().await;

    // PostgreSQL cannot hold a NUL in a text column at all, so a NUL that
    // reaches the driver comes back as DATABASE_ERROR — a 500 telling the
    // caller our server broke, over input no text column anywhere will accept.
    // It arrives percent-encoded and is a literal NUL by the time a parameter
    // has been deserialised, which is why the check reads the raw URI.
    let resp = app.get("/api/talents/search?q=%00").await;
    assert_eq!(resp.status().as_u16(), 400, "{:?}", resp.text().await);

    let resp = app.get("/api/feed/public?kind=a%00b").await;
    assert_eq!(resp.status().as_u16(), 400, "{:?}", resp.text().await);
}

#[tokio::test]
async fn a_declared_minimum_is_enforced_and_not_only_documented() {
    let app = TestApp::spawn().await;

    // `min_craft_score` is declared 0..=10000 in the contract. A negative
    // floor matches everybody, so an unenforced bound answers 200 with what
    // looks like a working search rather than saying the query meant nothing.
    let resp = app.get("/api/talents/search?min_craft_score=-1").await;
    assert_eq!(resp.status().as_u16(), 400, "{:?}", resp.text().await);

    let resp = app.get("/api/talents/search?min_craft_score=0").await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
}

#[tokio::test]
async fn a_declared_pattern_is_enforced_and_not_only_documented() {
    let app = TestApp::spawn().await;

    // `?language_spoken=` was accepted, applied, and reported back in
    // `filters_applied` — so the answer claimed to have narrowed the search on
    // a language while matching nobody, which reads as "no such people"
    // rather than "that is not a language code".
    for bad in ["", "x", "fra", "1r"] {
        let path = format!("/api/talents/search?language_spoken={bad}");
        let resp = app.get(&path).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "{path}: {:?}",
            resp.text().await
        );
    }

    for bad in ["", "fr", "FRA"] {
        let path = format!("/api/talents/search?country_iso2={bad}");
        let resp = app.get(&path).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "{path}: {:?}",
            resp.text().await
        );
    }

    let resp = app
        .get("/api/talents/search?language_spoken=fr&country_iso2=BJ")
        .await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
}

#[tokio::test]
async fn the_domain_filters_accept_every_live_domain() {
    let app = TestApp::spawn().await;

    // Three endpoints declared a domain list in their contract and let it go
    // stale — four domains in one, seven in two others, against eight live.
    // A contract that understates what it accepts sends a caller looking for
    // an endpoint that does not exist.
    let live: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM skill_domains WHERE is_active ORDER BY slug")
            .fetch_all(&app.db)
            .await
            .unwrap();

    for domain in &live {
        for base in ["/api/slices", "/api/explore", "/api/challenges"] {
            let path = format!("{base}?domain={domain}");
            let resp = app.get(&path).await;
            assert_ne!(
                resp.status().as_u16(),
                400,
                "{path} refuses a live domain: {:?}",
                resp.text().await
            );
        }
    }
}
