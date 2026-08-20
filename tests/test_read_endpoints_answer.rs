//! Read endpoints that nothing else calls.
//!
//! `GET /users/{username}/ops-profile` returned 500 to every request it had
//! ever received: one figure in its query was cast to `INT` while the struct
//! read every column as `i64`, so sqlx refused to decode the row. It survived
//! because exactly one test reached it, incidentally, on its way to checking
//! something else — a wholly dead endpoint and one awkward test look identical
//! from outside.
//!
//! An audit of the router found 404 of 922 registered routes that no test
//! calls at all. Writing 404 tests is not the answer. This calls the ones that
//! are cheapest to check and most likely to be hiding the same thing: GET, no
//! path parameter, and reached by nothing.
//!
//! What is asserted is the *shape* of the answer, not its content — so these
//! stay true as the endpoints grow, and they fail only for the reason worth
//! failing on. Unauthenticated is 401, forbidden is 403, absent is 404, and a
//! disabled integration is 503. A 500 to a well-formed request is none of
//! those: it is what a query that has stopped decoding produces.
//!
//! Excluded, deliberately: the OAuth start and callback routes, which redirect
//! or need a provider configured, and `security.txt`, which is text rather
//! than JSON. A 5xx from those says something about the environment.

mod common;
use common::TestApp;

/// Every GET that takes no path parameter and that no other test calls.
const READ_ENDPOINTS: &[&str] = &[
    "/api/api-plans",
    "/api/activity/heatmap",
    "/api/admin/accounting/export",
    "/api/admin/audit-log/generic",
    "/api/admin/dashboard/health",
    "/api/admin/dashboard/moderation",
    "/api/admin/dashboard/moderation-queue",
    "/api/admin/dashboard/overview",
    "/api/admin/disputes",
    "/api/admin/feature-flags",
    "/api/admin/reports",
    "/api/admin/revenue/by-pillar",
    "/api/admin/slices",
    "/api/admin/tournaments/prizes/outstanding",
    "/api/admin/validator-applications",
    "/api/admin/validators/collusion-matrix",
    "/api/ambassador-programs/open",
    "/api/audio/castings",
    "/api/audio/mentors/for-me",
    "/api/audio/portfolios",
    "/api/auth/me/oauth-providers",
    "/api/auth/webauthn/credentials",
    "/api/beginner/verifications/mine",
    "/api/beginner/verifications/queue",
    "/api/beta-programs/open",
    "/api/challenges/categories",
    "/api/challenges/featured",
    "/api/challenges/onboarding",
    "/api/challenges/tags",
    "/api/code/languages/top",
    "/api/community/challenges/mine",
    "/api/community/challenges/popular",
    "/api/contact/interest/sent",
    "/api/design/tiers",
    "/api/developer/webhooks",
    "/api/diplomas/my",
    "/api/disputes",
    "/api/dm/conversations",
    "/api/docs/openapi.json",
    "/api/enterprise/credits",
    "/api/enterprise/credits/transactions",
    "/api/enterprise/dashboard/funnel",
    "/api/enterprise/dashboard/overview",
    "/api/enterprise/invite/preview",
    "/api/enterprise/invoices",
    "/api/enterprise/kyc",
    "/api/enterprise/members",
    "/api/enterprise/memberships",
    "/api/enterprise/pipeline",
    "/api/enterprise/pipeline/export.csv",
    "/api/enterprise/sponsored-challenges",
    "/api/enterprise/subscriptions/current",
    "/api/enterprises/me/agency-clients",
    "/api/enterprises/me/type-config",
    "/api/feed/me",
    "/api/forum/categories",
    "/api/forum/search",
    "/api/geo/cities",
    "/api/health",
    "/api/health/deep",
    "/api/health/live",
    "/api/i18n/locales",
    "/api/launch-campaigns/open",
    "/api/legal/consent-version",
    "/api/manifest.webmanifest",
    "/api/me/feed/challenges",
    "/api/me/validation/queue",
    "/api/me/validator-applications",
    "/api/mentors/me/connect/status",
    "/api/metrics",
    "/api/metrics/summary",
    "/api/missions/types",
    "/api/notifications",
    "/api/notifications/push/vapid-public-key",
    "/api/notifications/unread-count",
    "/api/onboarding/bonjour-skilluv/status",
    "/api/payments/methods",
    "/api/pricing",
    "/api/profile/me/availability",
    "/api/profile/me/educations",
    "/api/profile/me/experiences",
    "/api/profile/me/languages",
    "/api/projects/curated",
    "/api/projects/looking-for-contributors",
    "/api/public/v1/usage",
    "/api/reports/mine",
    "/api/review-queue",
    "/api/sandbox/languages",
    "/api/scim/v2/ResourceTypes",
    "/api/scim/v2/Schemas",
    "/api/seasons",
    "/api/seasons/current",
    "/api/skills",
    "/api/skills/tree",
    "/api/sponsored-challenges/active",
    "/api/tags",
    "/api/team-slots/open",
    "/api/teams/marketplace",
    "/api/tournaments/feed",
    "/api/tracks",
    "/api/users/me/code-portfolios",
    "/api/users/me/interviews",
    "/api/users/me/mentor-subscriptions",
    "/api/users/me/missions",
    "/api/users/me/onboardings",
    "/api/users/me/peer-matching/enrollments",
    "/api/users/me/push-tokens",
    "/api/users/me/recommendations/projects",
    "/api/users/me/reverse-recruitment",
    "/api/users/me/skill-recommendations",
    "/api/users/me/slices",
    "/api/users/me/stewardships",
    "/api/users/me/teams",
    "/api/users/me/tracks",
    "/api/users/me/trials",
    "/api/users/me/vouchings",
    "/api/users/me/wallet/statement.csv",
    "/api/users/me/wallet/transactions",
];

/// The three doors, because an endpoint can decode fine on the path that
/// refuses you and fail on the one that reads rows. The unauthenticated pass
/// alone would not have caught the ops profile.
async fn every_endpoint_answers(app: &TestApp, who: &str) {
    let mut dead = Vec::new();

    for path in READ_ENDPOINTS {
        let resp = app.get(path).await;
        let status = resp.status().as_u16();
        // 503 is allowed and 500 is not, which is the whole distinction: an
        // integration this deployment does not have is unavailable, not
        // broken. Stripe is absent here and in CI, and four handlers used to
        // call that an internal error — telling a caller the server failed
        // when the honest answer is that payments were never configured.
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
