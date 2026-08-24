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
    "/api/communication/mentors/for-me",
    "/api/communication/review-languages",
    "/api/opportunities",
    "/api/portfolio-platforms",
    "/api/portfolios",
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

/// The same, for routes that take a path parameter — filled with an id
/// nothing owns.
///
/// A handler naming a column that does not exist fails when the query runs,
/// not when a row matches, so a stranger's id still reaches the failure. 404
/// is the right answer here and 500 is not: that is what
/// `/api/enterprises/me/agency-clients` was doing, and it took an audit
/// rather than a user to notice.
const PARAMETERISED_ENDPOINTS: &[&str] = &[
    "/api/activity/heatmap/00000000-0000-4000-8000-000000000000",
    "/api/admin/domains/nothing-by-this-name/featured-queue",
    "/api/admin/projects/nothing-by-this-name/stats",
    "/api/admin/sales/enterprises/00000000-0000-4000-8000-000000000000",
    "/api/admin/tenants/00000000-0000-4000-8000-000000000000",
    "/api/admin/tenants/00000000-0000-4000-8000-000000000000/cohorts",
    "/api/admin/tenants/00000000-0000-4000-8000-000000000000/members",
    "/api/admin/tournaments/00000000-0000-4000-8000-000000000000/vote-bursts",
    "/api/assistant/jobs/00000000-0000-4000-8000-000000000000",
    "/api/audio/castings/00000000-0000-4000-8000-000000000000",
    "/api/audio/files/00000000-0000-4000-8000-000000000000/listen",
    "/api/audio/slices/00000000-0000-4000-8000-000000000000/files",
    "/api/communication/slices/00000000-0000-4000-8000-000000000000/publications",
    "/api/communication/slices/00000000-0000-4000-8000-000000000000/translation-reviews",
    "/api/slices/00000000-0000-4000-8000-000000000000/revisions",
    "/api/audio/slices/00000000-0000-4000-8000-000000000000/sources",
    "/api/badge/repo/nothing-by-this-name/nothing-by-this-name/validated.svg",
    "/api/badge/user/nothing-by-this-name/validated.svg",
    "/api/beginner/verifications/questions/00000000-0000-4000-8000-000000000000",
    "/api/challenges/00000000-0000-4000-8000-000000000000/eligibility",
    "/api/challenges/00000000-0000-4000-8000-000000000000/teams",
    "/api/challenges/00000000-0000-4000-8000-000000000000/timer",
    "/api/contact/conversations/00000000-0000-4000-8000-000000000000",
    "/api/deliverables/00000000-0000-4000-8000-000000000000",
    "/api/deliverables/00000000-0000-4000-8000-000000000000/reviews",
    "/api/dev/verify-tokens/nothing-by-this-name",
    "/api/developer/keys/00000000-0000-4000-8000-000000000000/usage",
    "/api/dm/conversations/00000000-0000-4000-8000-000000000000/messages",
    "/api/enterprise/ambassador-programs/00000000-0000-4000-8000-000000000000/ambassadors",
    "/api/enterprise/consultations/00000000-0000-4000-8000-000000000000",
    "/api/enterprise/contests/00000000-0000-4000-8000-000000000000/submissions",
    "/api/enterprise/invoices/00000000-0000-4000-8000-000000000000",
    "/api/enterprise/invoices/00000000-0000-4000-8000-000000000000/html",
    "/api/enterprise/invoices/00000000-0000-4000-8000-000000000000/pdf",
    "/api/enterprise/invoices/00000000-0000-4000-8000-000000000000/preview",
    "/api/enterprise/lists/00000000-0000-4000-8000-000000000000",
    "/api/enterprise/sponsored-challenges/00000000-0000-4000-8000-000000000000/submissions",
    "/api/featured/nothing-by-this-name/recent",
    "/api/guilds/00000000-0000-4000-8000-000000000000/members",
    "/api/guilds/nothing-by-this-name/composition",
    "/api/guilds/nothing-by-this-name/projects",
    "/api/maintainer-digest/confirm/nothing-by-this-name",
    "/api/maintainer-digest/unsubscribe/nothing-by-this-name",
    "/api/marketplace/downloads/nothing-by-this-name",
    "/api/payments/00000000-0000-4000-8000-000000000000/status",
    "/api/projects/00000000-0000-4000-8000-000000000000/stewards",
    "/api/projects/nothing-by-this-name/active-skilluvers",
    "/api/projects/nothing-by-this-name/contributors",
    "/api/public/v1/talent-attestations/nothing-by-this-name",
    "/api/review-queue/00000000-0000-4000-8000-000000000000",
    "/api/sandbox/result/nothing-by-this-name",
    "/api/skills/nothing-by-this-name/talents",
    "/api/skills/tree/00000000-0000-4000-8000-000000000000",
    "/api/social/comments/nothing-by-this-name/00000000-0000-4000-8000-000000000000",
    "/api/social/reactions/nothing-by-this-name/00000000-0000-4000-8000-000000000000/summary",
    "/api/social/tag-map/nothing-by-this-name/00000000-0000-4000-8000-000000000000",
    "/api/stewards/00000000-0000-4000-8000-000000000000/inbox",
    "/api/studios/00000000-0000-4000-8000-000000000000",
    "/api/teams/00000000-0000-4000-8000-000000000000/slices",
    "/api/teams/00000000-0000-4000-8000-000000000000/slots",
    "/api/tournaments/nothing-by-this-name/community-ranking",
    "/api/tracks/nothing-by-this-name",
    "/api/tracks/nothing-by-this-name/progress",
    "/api/u/nothing-by-this-name/cv",
    "/api/u/nothing-by-this-name/repos",
    "/api/users/00000000-0000-4000-8000-000000000000/attestations",
    "/api/users/00000000-0000-4000-8000-000000000000/deliverables",
    "/api/users/00000000-0000-4000-8000-000000000000/skills",
    "/api/users/me/orientations/nothing-by-this-name/playlist",
    "/api/users/nothing-by-this-name/audio-profile",
    "/api/users/nothing-by-this-name/badge.svg",
    "/api/users/nothing-by-this-name/design-profile",
    "/api/users/nothing-by-this-name/portfolio.json",
    "/api/v1/users/nothing-by-this-name/badges",
    "/api/v1/users/nothing-by-this-name/skills",
    "/api/verify/nothing-by-this-name/pdf",
];

/// The three doors, because an endpoint can decode fine on the path that
/// refuses you and fail on the one that reads rows. The unauthenticated pass
/// alone would not have caught the ops profile.
async fn every_endpoint_answers(app: &TestApp, who: &str) {
    let mut dead = Vec::new();

    for path in READ_ENDPOINTS.iter().chain(PARAMETERISED_ENDPOINTS) {
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
