//! Read endpoints that nothing else calls.
//!
//! `GET /users/{username}/ops-profile` returned 500 to every request it had
//! ever received: one figure in its query was cast to `INT` while the struct
//! read every column as `i64`, so sqlx refused to decode the row. It survived
//! to CI because exactly one test reached it, incidentally, on its way to
//! checking something else — a wholly dead endpoint and one awkward test look
//! identical from outside.
//!
//! An audit of the router found 404 of 922 registered routes that no test
//! calls at all. Writing 404 tests is not the answer. This covers the ones
//! most likely to be dead on arrival and cheapest to check: read endpoints
//! taking no path parameter, added recently, called by nothing.
//!
//! What is asserted is the *shape* of the answer, not its content — so these
//! stay true as the endpoints grow, and they only fail for the reason worth
//! failing on. Unauthenticated is 401, forbidden is 403, absent is 404. A 5xx
//! is never a correct answer to a well-formed request, and it is what a query
//! that stopped decoding produces.

mod common;
use common::TestApp;

/// Every one of these is registered, and no other test calls it.
const READ_ENDPOINTS: &[&str] = &[
    // The applicant tracker's price list, read before subscribing.
    "/api/ats/plans",
    // What the platform asks consent for. Public by design: somebody has to
    // be able to read the purposes without holding an account.
    "/api/data/purposes",
    // The credential queue a reviewer works from.
    "/api/admin/credentials/pending",
    "/api/admin/validators/stats",
    "/api/enterprise/dashboard/platform-stats",
    "/api/enterprise/dashboard/my-stats",
    "/api/users/me/contest-invitations",
];

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_a_stranger() {
    let app = TestApp::spawn().await;

    for path in READ_ENDPOINTS {
        let resp = app.get(path).await;
        let status = resp.status().as_u16();
        assert!(
            status < 500,
            "GET {path} answered {status} with no session: {:?}",
            resp.text().await
        );
    }
}

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_a_signed_in_person() {
    let app = TestApp::spawn().await;
    app.register_user("reader_smoke").await;
    app.login("reader_smoke").await;

    // The same list with a session. An endpoint can decode fine on the
    // rejection path and fail on the one that actually reads rows, so the
    // unauthenticated pass alone would not have caught the ops profile.
    for path in READ_ENDPOINTS {
        let resp = app.get(path).await;
        let status = resp.status().as_u16();
        assert!(
            status < 500,
            "GET {path} answered {status} to a member: {:?}",
            resp.text().await
        );
    }
}

#[tokio::test]
async fn no_read_endpoint_answers_5xx_to_an_admin() {
    let app = TestApp::spawn().await;
    app.register_admin("reader_smoke_admin").await;

    // The admin surfaces are the ones a stranger never reaches, so a broken
    // query behind the gate is invisible to both passes above.
    for path in READ_ENDPOINTS {
        let resp = app.get(path).await;
        let status = resp.status().as_u16();
        assert!(
            status < 500,
            "GET {path} answered {status} to an admin: {:?}",
            resp.text().await
        );
    }
}
