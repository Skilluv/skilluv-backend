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
