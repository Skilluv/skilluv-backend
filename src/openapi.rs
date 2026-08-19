//! OpenAPI 3.1 schema generation (BE-P1-CONTRACT).
//!
//! Root `ApiDoc` derive collects every handler annotated with
//! `#[utoipa::path(...)]` across the codebase and every DTO annotated with
//! `#[derive(ToSchema)]`. The schema is exposed at :
//!
//!   - `GET  /api/openapi.json`  — raw JSON (consumed by schemathesis in CI)
//!   - `GET  /api/docs`          — Swagger UI (interactive exploration)
//!
//! Contract testing lives in `.github/workflows/contract-test.yml` — it
//! boots the API against a real Postgres, fetches `/api/openapi.json` and
//! runs schemathesis property-based fuzzing to catch payload contract
//! drifts between backend and front.
//!
//! ## How to add a handler
//!
//! 1. Put `#[utoipa::path(...)]` on the handler function.
//! 2. Ensure every request/response struct derives `utoipa::ToSchema`.
//! 3. Register the handler in `paths(...)` and any bespoke DTOs in
//!    `components(schemas(...))` below.
//! 4. `cargo check` — the derive will yell about anything missing.
//!
//! Shared envelope types (`ApiResponse<T>`, `ErrorResponse`) live in
//! `crate::api_response` and are pre-registered here.

use axum::{Router, response::IntoResponse};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_swagger_ui::SwaggerUi;

use crate::api_response::{ApiResponse, ErrorObject, ErrorResponse, MetaInfo, SimpleMessage};

/// Root OpenAPI document. Every route module contributes its own set of
/// `#[utoipa::path]` handlers here via the `paths(...)` and `components(...)`
/// arguments. Kept intentionally short — the real work happens in the
/// per-handler annotations spread across `src/routes/`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Skilluv Backend API",
        description = "Compétences tech prouvées par des artefacts opposables. \
                       Contract-tested via schemathesis on every PR.",
        contact(name = "Skilluv Team", email = "security@skill-uv.com"),
        license(name = "Proprietary"),
    ),
    servers(
        // Paths in the spec already carry the `/api/` prefix (utoipa
        // `path = "/api/..."` on every handler). Base URLs here must NOT
        // duplicate it — schemathesis / Swagger UI concatenate base + path
        // literally, so `/api` here yielded `/api/api/...` 404s.
        (url = "/", description = "Same-origin (typical Coolify deploy)"),
        (url = "http://localhost:3001", description = "Local dev"),
    ),
    modifiers(&SecurityAddon, &CommonErrorResponsesAddon),
    paths(
        // ─── auth ─────────────────────────────────────────────────
        crate::routes::auth::verify_email,
        crate::routes::auth::resend_verification,
        crate::routes::auth::forgot_password,
        crate::routes::auth::reset_password,
        crate::routes::auth::change_password,
        crate::routes::auth::logout,
        crate::routes::auth::list_sessions,
        crate::routes::auth::revoke_session,
        crate::routes::auth::revoke_all_other_sessions,
        crate::routes::auth::totp_disable,
        crate::routes::auth::email_2fa_enable,
        crate::routes::auth::email_2fa_disable,
        crate::routes::auth::me,
        crate::routes::auth::request_email_change,
        crate::routes::auth::confirm_email_change,
        crate::routes::auth::refresh,
        crate::routes::auth::complete_profile,
        crate::routes::auth::totp_setup,
        crate::routes::auth::totp_enable,
        crate::routes::auth::regenerate_backup_codes,
        crate::routes::auth::delete_account,
        crate::routes::auth::request_data_export,
        crate::routes::auth::register,
        crate::routes::auth::login,
        crate::routes::auth::email_2fa_verify,
        // ─── health ───────────────────────────────────────────────
        crate::routes::health::liveness,
        crate::routes::health::deep_health,
        // ─── magic link ───────────────────────────────────────────
        crate::routes::magic_link::request_link,
        crate::routes::magic_link::consume_link,
        // ─── geo ──────────────────────────────────────────────────
        crate::routes::geo::list_countries,
        crate::routes::geo::search_cities,
        // ─── i18n ─────────────────────────────────────────────────
        crate::routes::i18n::list_locales,
        // ─── email preferences ────────────────────────────────────
        // SKI-293 — the v2 routes under /users/me were shipped in SKI-287 and
        // never registered here, so the document advertised only the legacy
        // /auth/me pair, with a different payload shape. That is why the front
        // reported "two routes, two shapes".
        crate::routes::email_prefs::get_prefs_v2,
        crate::routes::email_prefs::replace_prefs,
        crate::routes::email_prefs::unsubscribe,
        crate::routes::email_prefs::unsubscribe_by_path,
        crate::routes::email_prefs::brevo_webhook,
        crate::routes::email_prefs::admin_run_weekly_digest,
        // Notification settings across all three channels.
        crate::routes::notification_preferences::list_preferences,
        crate::routes::notification_preferences::update_preferences,
        crate::routes::notification_preferences::reset_preferences,
        crate::routes::notification_preferences::set_quiet_hours,
        // ─── mentions (SKI-286) ───────────────────────────────────
        crate::routes::mentions::list_mine,
        crate::routes::mentions::read_one,
        crate::routes::mentions::read_all,
        // ─── metrics (internal dashboards) ────────────────────────
        crate::routes::metrics::metrics_summary,
        // ─── tracks ───────────────────────────────────────────────
        crate::routes::tracks::list_tracks,
        crate::routes::tracks::get_track,
        crate::routes::tracks::enroll_track,
        crate::routes::tracks::track_progress,
        crate::routes::tracks::my_tracks,
        crate::routes::tracks::challenge_eligibility,
        // ─── notifications ────────────────────────────────────────
        crate::routes::notifications::list_notifications,
        crate::routes::notifications::mark_read,
        crate::routes::notifications::mark_all_read,
        crate::routes::notifications::unread_count,
        // ─── legal / consent ──────────────────────────────────────
        crate::routes::legal::consent_version,
        crate::routes::legal::record_consent,
        // ─── badges ───────────────────────────────────────────────
        crate::routes::badges::user_badges,
        crate::routes::badges::list_rules,
        // ─── reports (user-facing moderation) ─────────────────────
        crate::routes::reports::create_report,
        crate::routes::reports::my_reports,
        crate::routes::reports::cancel_report,
        // ─── capabilities ─────────────────────────────────────────
        crate::routes::capabilities::user_capabilities_public,
        crate::routes::capabilities::my_capabilities,
        crate::routes::capabilities::admin_grant_capability,
        crate::routes::capabilities::admin_revoke_capability,
        // ─── portfolio ────────────────────────────────────────────
        crate::routes::portfolio::portfolio_json,
        crate::routes::portfolio::badge_svg,
        // ─── events ───────────────────────────────────────────────
        crate::routes::events::list_events,
        crate::routes::events::read_event,
        crate::routes::events::join_event,
        crate::routes::events::my_events,
        crate::routes::events::appoint,
        crate::routes::events::set_status,
        crate::routes::events::add_livestream,
        // ─── the ops domain ───────────────────────────────────────
        crate::routes::ops_practice::reference,
        crate::routes::ops_practice::declare_objective,
        crate::routes::ops_practice::my_objectives,
        crate::routes::ops_practice::close_objective,
        crate::routes::ops_practice::verify_objective,
        crate::routes::ops_practice::open_incident,
        crate::routes::ops_practice::my_incidents,
        crate::routes::ops_practice::resolve_incident,
        crate::routes::ops_practice::add_action,
        crate::routes::ops_practice::publish_postmortem,
        crate::routes::ops_practice::overdue_actions,
        crate::routes::ops_practice::record_cost_work,
        crate::routes::ops_practice::verify_cost_work,
        crate::routes::ops_practice::attest_artefact,
        crate::routes::ops_practice::attest_featured,
        crate::routes::ops_practice::ops_profile,
        crate::routes::ops_practice::toolkit,
        crate::routes::ops_practice::ops_mentor_matches,
        crate::routes::ops_practice::complete_onboarding,
        crate::routes::ops_practice::skip_onboarding,
        crate::routes::ats::plans,
        crate::routes::ats::subscribe,
        crate::routes::ats::my_subscription,
        crate::routes::ats::open_position,
        crate::routes::ats::my_openings,
        crate::routes::ats::close_position,
        crate::routes::ats::pipeline,
        crate::routes::ats::add_candidate,
        crate::routes::ats::move_candidate,
        crate::routes::credentials::declare_credential,
        crate::routes::credentials::my_credentials,
        crate::routes::credentials::pending,
        crate::routes::credentials::verify,
        crate::routes::credentials::refuse,
        // ─── consolidated overview and the sales pipeline ─────────
        crate::routes::sales_pipeline::my_overview,
        crate::routes::sales_pipeline::open_opportunity,
        crate::routes::sales_pipeline::pipeline,
        crate::routes::sales_pipeline::read_opportunity,
        crate::routes::sales_pipeline::set_stage,
        crate::routes::sales_pipeline::record_activity,
        crate::routes::sales_pipeline::overdue,
        crate::routes::sales_pipeline::renewals,
        crate::routes::sales_pipeline::enterprise_file,
        // ─── long placements, learning seats, open calls ──────────
        crate::routes::additional_products::propose,
        crate::routes::additional_products::my_placements,
        crate::routes::additional_products::respond,
        crate::routes::additional_products::bill_month,
        crate::routes::additional_products::end_placement,
        crate::routes::additional_products::list_plans,
        crate::routes::additional_products::subscribe_learning,
        crate::routes::additional_products::invite_seat,
        crate::routes::additional_products::activate_seat,
        crate::routes::additional_products::seat_usage,
        crate::routes::additional_products::list_rfps,
        crate::routes::additional_products::open_rfp,
        crate::routes::additional_products::propose_on_rfp,
        crate::routes::additional_products::read_proposals,
        crate::routes::additional_products::decide,
        crate::routes::additional_products::award,
        // ─── onboarding, labs, team proposals ─────────────────────
        crate::routes::continuous::propose_onboarding,
        crate::routes::continuous::my_onboardings,
        crate::routes::continuous::respond_to_onboarding,
        crate::routes::continuous::check_in,
        crate::routes::continuous::record_retention,
        crate::routes::continuous::open_lab,
        crate::routes::continuous::open_labs,
        crate::routes::continuous::join_lab,
        crate::routes::continuous::contribute,
        crate::routes::continuous::judge_contribution,
        crate::routes::continuous::settle_month,
        crate::routes::continuous::draft_proposal,
        crate::routes::continuous::visible_proposals,
        crate::routes::continuous::add_member,
        crate::routes::continuous::respond_to_proposal,
        crate::routes::continuous::publish_proposal,
        crate::routes::continuous::express_interest,
        crate::routes::continuous::record_signature,
        // ─── consultation line ────────────────────────────────────
        crate::routes::consultations::request_consultation,
        crate::routes::consultations::my_consultations,
        crate::routes::consultations::read_consultation,
        crate::routes::consultations::rate,
        crate::routes::consultations::invite_expert,
        crate::routes::consultations::respond,
        crate::routes::consultations::submit_opinion,
        crate::routes::consultations::deliver,
        crate::routes::consultations::open_audit,
        crate::routes::consultations::readiness,
        crate::routes::consultations::inform_employee,
        crate::routes::consultations::assess,
        crate::routes::consultations::share_assessment,
        crate::routes::consultations::deliver_audit,
        crate::routes::consultations::my_assessments,
        crate::routes::consultations::respond_to_assessment,
        // ─── paid mentoring ───────────────────────────────────────
        crate::routes::mentoring_products::subscribe,
        crate::routes::mentoring_products::my_subscriptions,
        crate::routes::mentoring_products::cancel_subscription,
        crate::routes::mentoring_products::subscription_usage,
        crate::routes::mentoring_products::record_hours,
        crate::routes::mentoring_products::award_commission,
        crate::routes::mentoring_products::open_slot,
        crate::routes::mentoring_products::open_slots,
        crate::routes::mentoring_products::open_programs,
        crate::routes::mentoring_products::open_program,
        crate::routes::mentoring_products::enrol,
        // ─── ecosystem line ───────────────────────────────────────
        crate::routes::ecosystem::list_programs,
        crate::routes::ecosystem::list_live,
        crate::routes::ecosystem::request_certification,
        crate::routes::ecosystem::audit,
        crate::routes::ecosystem::revoke,
        crate::routes::ecosystem::expire_lapsed,
        crate::routes::ecosystem::list_items,
        crate::routes::ecosystem::read_item,
        crate::routes::ecosystem::list_item,
        crate::routes::ecosystem::publish_item,
        crate::routes::ecosystem::purchase,
        crate::routes::ecosystem::download,
        crate::routes::ecosystem::rate,
        // ─── finance line ─────────────────────────────────────────
        crate::routes::finance_line::list_partners,
        crate::routes::finance_line::request_referral,
        crate::routes::finance_line::request_advance,
        crate::routes::finance_line::my_advances,
        crate::routes::finance_line::subscribe_guarantee,
        crate::routes::finance_line::open_partnership,
        crate::routes::finance_line::activate_partnership,
        crate::routes::finance_line::record_decision,
        crate::routes::finance_line::disburse,
        crate::routes::finance_line::mark_repaid,
        crate::routes::finance_line::write_off,
        crate::routes::finance_line::honour_guarantee,
        // ─── data line: consent, metered API, licensing ───────────
        crate::routes::data_line::list_purposes,
        crate::routes::data_line::my_consent,
        crate::routes::data_line::set_consent,
        crate::routes::data_line::my_unified_profile,
        crate::routes::data_line::my_partners,
        crate::routes::data_line::set_partner,
        crate::routes::data_line::list_api_plans,
        crate::routes::data_line::talent_score,
        crate::routes::data_line::talent_attestations,
        crate::routes::data_line::key_usage,
        crate::routes::data_line::list_reports,
        crate::routes::data_line::commission_report,
        crate::routes::data_line::deliver_report,
        crate::routes::data_line::list_licences,
        crate::routes::data_line::open_licence,
        crate::routes::data_line::settle_licence,
        crate::routes::data_line::list_deployments,
        crate::routes::data_line::provision_deployment,
        crate::routes::data_line::go_live,
        crate::routes::data_line::cohort_sizes,
        // ─── enterprise contests and interviews ───────────────────
        crate::routes::contests::open_contests,
        crate::routes::contests::read_contest,
        crate::routes::contests::open_contest,
        crate::routes::contests::my_contests,
        crate::routes::contests::set_status,
        crate::routes::contests::read_submissions,
        crate::routes::contests::invite,
        crate::routes::contests::judge,
        crate::routes::contests::record_hire,
        crate::routes::contests::set_outcome,
        crate::routes::contests::conclude,
        crate::routes::contests::respond_to_invitation,
        crate::routes::contests::submit,
        crate::routes::contests::my_invitations,
        crate::routes::contests::propose_interview,
        crate::routes::contests::complete,
        crate::routes::contests::my_interviews,
        crate::routes::contests::confirm_interview,
        crate::routes::contests::decline_interview,
        // ─── brand: sponsors, campaigns, ambassadors, audience ────
        crate::routes::brand::list_packages,
        crate::routes::brand::propose_sponsorship,
        crate::routes::brand::my_sponsorships,
        crate::routes::brand::read_leads,
        crate::routes::brand::export_leads,
        crate::routes::brand::visit_stand,
        crate::routes::brand::open_annual_contract,
        crate::routes::brand::sign_sponsorship,
        crate::routes::brand::honour_sponsorship,
        crate::routes::brand::cancel_sponsorship,
        crate::routes::brand::commission_content,
        crate::routes::brand::publish_content,
        crate::routes::brand::open_campaign,
        crate::routes::brand::my_campaigns,
        crate::routes::brand::open_campaigns,
        crate::routes::brand::submit_piece,
        crate::routes::brand::read_pieces_as_sponsor,
        crate::routes::brand::decide_piece,
        crate::routes::brand::open_submissions,
        crate::routes::brand::review_quality,
        crate::routes::brand::close_campaign,
        crate::routes::brand::open_ambassador_program,
        crate::routes::brand::my_ambassador_programs,
        crate::routes::brand::read_ambassadors,
        crate::routes::brand::open_ambassador_programs,
        crate::routes::brand::respond_to_invite,
        crate::routes::brand::record_deliverable,
        crate::routes::brand::invite_ambassador,
        crate::routes::brand::activate_program,
        crate::routes::brand::pay_stipend,
        crate::routes::brand::list_audience_plans,
        crate::routes::brand::subscribe,
        crate::routes::brand::cancel_subscription,
        crate::routes::brand::my_audience_access,
        // ─── skills ───────────────────────────────────────────────
        crate::routes::skills::list_skills,
        crate::routes::skills::find_talents,
        crate::routes::skills::user_skills,
        crate::routes::skills::my_skill_recommendations,
        // ─── ai jobs ──────────────────────────────────────────────
        crate::routes::ai_jobs::request_code_review,
        crate::routes::ai_jobs::request_recommendations,
        crate::routes::ai_jobs::get_job_result,
        crate::routes::ai_jobs::admin_hidden_gems,
        crate::routes::ai_jobs::admin_churn,
        // ─── attestations ─────────────────────────────────────────
        crate::routes::attestations::list_user_attestations,
        crate::routes::attestations::verify_attestation,
        crate::routes::attestations::issue_compagnonnage,
        crate::routes::attestations::revoke_attestation,
        // SKI-292 — share card. Documented so the front knows the URL to put
        // in `og:image` without reading the router.
        crate::routes::attestations_public::verify_og_card,
        // ─── push notifications ───────────────────────────────────
        crate::routes::push::vapid_public_key,
        crate::routes::push::subscribe,
        crate::routes::push::unsubscribe,
        crate::routes::push::register_mobile_token,
        crate::routes::push::revoke_mobile_token,
        crate::routes::push::list_mobile_tokens,
        // ─── seasons + stewards ───────────────────────────────────
        crate::routes::seasons::list_seasons,
        crate::routes::seasons::current_season,
        crate::routes::seasons::get_season,
        crate::routes::seasons::create_season,
        crate::routes::seasons::activate_season,
        crate::routes::seasons::list_project_stewards,
        crate::routes::seasons::add_steward,
        crate::routes::seasons::remove_steward,
        crate::routes::seasons::my_stewardships,
        // ─── agency_clients + type_config ─────────────────────────
        crate::routes::agency_clients::get_type_config,
        crate::routes::agency_clients::patch_type_config,
        crate::routes::agency_clients::list,
        crate::routes::agency_clients::create,
        crate::routes::agency_clients::update,
        crate::routes::agency_clients::deactivate,
        // ─── ai coach ─────────────────────────────────────────────
        crate::routes::ai_coach::my_performance,
        crate::routes::ai_coach::suggest_orientations,
        // ─── certifications + diplomas ────────────────────────────
        crate::routes::certifications::list_certifications,
        crate::routes::certifications::purchase_certification,
        crate::routes::certifications::start_attempt,
        crate::routes::certifications::submit_attempt,
        crate::routes::certifications::verify_diploma,
        crate::routes::certifications::my_diplomas,
        // ─── leaderboard ──────────────────────────────────────────
        crate::routes::leaderboard::list_leaderboards,
        crate::routes::leaderboard::get_leaderboard,
        crate::routes::leaderboard::my_rank,
        // ─── legal well-known (enterprise dashboard + accounting) ─
        crate::routes::legal_well_known::admin_accounting_export,
        crate::routes::legal_well_known::dashboard_overview,
        crate::routes::legal_well_known::dashboard_funnel,
        // ─── sponsored challenges ─────────────────────────────────
        crate::routes::sponsored_challenges::request_sponsorship,
        crate::routes::sponsored_challenges::list_my_requests,
        crate::routes::sponsored_challenges::admin_list_requests,
        crate::routes::sponsored_challenges::admin_decide_request,
        crate::routes::sponsored_challenges::admin_link_challenge,
        crate::routes::sponsored_challenges::public_active,
        crate::routes::sponsored_challenges::sponsor_view_submissions,
        // ─── talent line: entitlements, trials, reverse recruitment
        crate::routes::talent_line::my_entitlements,
        crate::routes::talent_line::grant_entitlement,
        crate::routes::talent_line::start_trial,
        crate::routes::talent_line::enterprise_trials,
        crate::routes::talent_line::my_trials,
        crate::routes::talent_line::log_hours,
        crate::routes::talent_line::trial_hours,
        crate::routes::talent_line::decide_hours,
        crate::routes::talent_line::conclude_trial,
        crate::routes::talent_line::post_wanted,
        crate::routes::talent_line::my_posting,
        crate::routes::talent_line::browse_postings,
        crate::routes::talent_line::send_pitch,
        crate::routes::talent_line::my_pitches,
        crate::routes::talent_line::respond_to_pitch,
        // ─── recruitment campaigns ────────────────────────────────
        crate::routes::recruitment::open_campaign,
        crate::routes::recruitment::my_campaigns,
        crate::routes::recruitment::read_shortlist,
        crate::routes::recruitment::confirm_hire,
        crate::routes::recruitment::talent_response,
        crate::routes::recruitment::my_invitations,
        crate::routes::recruitment::all_campaigns,
        crate::routes::recruitment::assign,
        crate::routes::recruitment::add_to_shortlist,
        crate::routes::recruitment::record_departure,
        // ─── studios, engagements, beta programmes ────────────────
        crate::routes::engagements::list_studios,
        crate::routes::engagements::read_studio,
        crate::routes::engagements::create_studio,
        crate::routes::engagements::add_studio_member,
        crate::routes::engagements::activate_studio,
        crate::routes::engagements::disband_studio,
        crate::routes::engagements::open_engagement,
        crate::routes::engagements::my_engagements,
        crate::routes::engagements::read_engagement,
        crate::routes::engagements::read_milestones,
        crate::routes::engagements::accept_milestone,
        crate::routes::engagements::respond,
        crate::routes::engagements::add_member,
        crate::routes::engagements::staff,
        crate::routes::engagements::add_milestone,
        crate::routes::engagements::start,
        crate::routes::engagements::review_milestone,
        crate::routes::engagements::open_programs,
        crate::routes::engagements::open_program,
        crate::routes::engagements::my_programs,
        crate::routes::engagements::read_testers,
        crate::routes::engagements::review_feedback,
        crate::routes::engagements::join_program,
        crate::routes::engagements::submit_feedback,
        crate::routes::engagements::close_program,
        // ─── enterprise products ──────────────────────────────────
        crate::routes::enterprise_products::list_types,
        crate::routes::enterprise_products::my_products,
        crate::routes::enterprise_products::products_of,
        crate::routes::enterprise_products::record_product,
        crate::routes::enterprise_products::set_status,
        crate::routes::enterprise_products::renewals,
        // ─── revenue streams ──────────────────────────────────────
        crate::routes::revenue::list_streams,
        crate::routes::revenue::by_pillar,
        // ─── public artefact feed ─────────────────────────────────
        crate::routes::public_feed::read_feed,
        crate::routes::public_feed::my_preferences,
        crate::routes::public_feed::set_preference,
        crate::routes::public_feed::withdraw,
        // ─── code profile: craft score + tiers ────────────────────
        crate::routes::code_profile::code_profile,
        crate::routes::code_profile::recompute_mine,
        crate::routes::code_profile::list_tiers,
        crate::routes::code_profile::my_portfolios,
        crate::routes::code_profile::claim_portfolio,
        crate::routes::code_profile::drop_portfolio,
        crate::routes::code_profile::complete_onboarding,
        crate::routes::code_profile::skip_onboarding,
        crate::routes::code_profile::mentor_matches,
        // ─── missions marketplace ─────────────────────────────────
        crate::routes::missions::list_types,
        crate::routes::missions::list_missions,
        crate::routes::missions::get_mission,
        crate::routes::missions::create_mission,
        crate::routes::missions::set_mission_status,
        crate::routes::missions::apply_to_mission,
        crate::routes::missions::list_applications,
        crate::routes::missions::decide,
        crate::routes::missions::list_invoices,
        crate::routes::missions::issue_invoice,
        crate::routes::missions::pay_invoice,
        crate::routes::missions::my_missions,
        // ─── code awards ──────────────────────────────────────────
        crate::routes::awards::list_categories,
        crate::routes::awards::edition_standings,
        crate::routes::awards::nominate,
        crate::routes::awards::vote,
        crate::routes::awards::shortlist,
        // ─── code contests: submissions + judging ─────────────────
        crate::routes::tournament::submit_entry,
        crate::routes::tournament::list_submissions,
        crate::routes::tournament::judge_entry,
        crate::routes::tournament::list_jury,
        crate::routes::tournament::respond_to_jury,
        crate::routes::tournament::community_vote,
        crate::routes::tournament::community_ranking,
        crate::routes::tournament::admin_invite_juror,
        crate::routes::tournament::admin_vote_bursts,
        // ─── code: first-issue feed + language ecosystems ─────────
        crate::routes::domain_profile::get_profile,
        crate::routes::domain_profile::put_profile,
        crate::routes::domain_profile::skip_profile,
        crate::routes::ai::toolkit,
        crate::routes::ai::mentor_matches,
        crate::routes::benchmarks::list_benchmarks,
        crate::routes::benchmarks::record_benchmark,
        crate::routes::benchmarks::reproduce_benchmark,
        crate::routes::ai_safety::list_reports,
        crate::routes::ai_safety::record_report,
        crate::routes::ai_safety::update_disclosure,
        crate::routes::ai_safety::reproduce_report,
        crate::routes::ai::competitions,
        crate::routes::ai::artifacts,
        crate::routes::ai::user_ai_profile,
        crate::routes::audio::user_audio_profile,
        crate::routes::audio::list_files,
        crate::routes::audio::upload_file,
        crate::routes::audio::listen,
        crate::routes::audio::list_sources,
        crate::routes::audio::declare_source,
        crate::routes::audio::complete_sources,
        crate::routes::audio::list_revisions,
        crate::routes::audio::request_revision,
        crate::routes::audio::resolve_revision,
        crate::routes::audio::list_castings,
        crate::routes::audio::open_casting,
        crate::routes::audio::get_casting,
        crate::routes::audio::audition,
        crate::routes::audio::select_voice,
        crate::routes::audio::credit,
        crate::routes::audio::mentor_matches,
        crate::routes::audio::project_credits,
        crate::routes::audio::my_portfolios,
        crate::routes::audio::declare_portfolio,
        crate::routes::audio::drop_portfolio,
        crate::routes::domain_profile::list_questions,
        crate::routes::code::first_issues,
        crate::routes::code::language_ecosystems,
        crate::routes::guides::list_guides,
        crate::routes::guides::get_guide,
        // ─── orientations ─────────────────────────────────────────
        crate::routes::orientations::list_orientations,
        crate::routes::orientations::get_orientation,
        crate::routes::orientations::my_orientations,
        crate::routes::orientations::register_orientation,
        crate::routes::orientations::update_orientation,
        crate::routes::orientations::orientation_playlist,
        crate::routes::orientations::end_orientation,
        crate::routes::orientations::public_user_orientations,
        // ─── talent lists + bookmarks ─────────────────────────────
        crate::routes::talent_lists::add_bookmark,
        crate::routes::talent_lists::remove_bookmark,
        crate::routes::talent_lists::list_bookmarks,
        crate::routes::talent_lists::create_list,
        crate::routes::talent_lists::list_lists,
        crate::routes::talent_lists::get_list,
        crate::routes::talent_lists::update_list,
        crate::routes::talent_lists::delete_list,
        crate::routes::talent_lists::add_to_list,
        crate::routes::talent_lists::remove_from_list,
        // ─── tenants (white-label) ────────────────────────────────
        crate::routes::tenants::get_current_tenant,
        crate::routes::tenants::list_tenants,
        crate::routes::tenants::create_tenant,
        crate::routes::tenants::get_tenant,
        crate::routes::tenants::update_tenant,
        crate::routes::tenants::list_members,
        crate::routes::tenants::add_member,
        crate::routes::tenants::list_cohorts,
        crate::routes::tenants::create_cohort,
        // ─── onboarding (Bonjour Skilluv) ─────────────────────────
        crate::routes::onboarding::start_bonjour_skilluv,
        crate::routes::onboarding::get_bonjour_skilluv_status,
        // ─── challenge tags + featured ────────────────────────────
        crate::routes::challenge_tags::list_tags,
        crate::routes::challenge_tags::list_categories,
        crate::routes::challenge_tags::featured_challenges,
        // ─── sandbox (Judge0) ─────────────────────────────────────
        crate::routes::sandbox::execute,
        crate::routes::sandbox::execute_async,
        crate::routes::sandbox::get_result,
        crate::routes::sandbox::list_languages,
        // ─── admin content ops ────────────────────────────────────
        crate::routes::admin_content_ops::hello_wall_mirror_run,
        crate::routes::admin_content_ops::profile_readme_sync_run,
        crate::routes::admin_content_ops::recompute_badges_for_user,
        // ─── enterprise subscriptions ─────────────────────────────
        crate::routes::enterprise_subscriptions::subscribe_to_pipeline,
        crate::routes::enterprise_subscriptions::current_subscription,
        crate::routes::enterprise_subscriptions::cancel_subscription,
        // ─── enterprise dashboard stats ───────────────────────────
        crate::routes::enterprise_dashboard::platform_stats,
        crate::routes::enterprise_dashboard::my_stats,
        // ─── gamification (skill tree + heatmap) ──────────────────
        crate::routes::gamification::my_skill_tree,
        crate::routes::gamification::user_skill_tree,
        crate::routes::gamification::my_heatmap,
        crate::routes::gamification::user_heatmap,
        // ─── public API v1 ────────────────────────────────────────
        crate::routes::public_api::get_user_profile,
        crate::routes::public_api::get_user_badges,
        crate::routes::public_api::get_user_skills,
        // ─── review queue ─────────────────────────────────────────
        crate::routes::review_queue::list_open,
        crate::routes::review_queue::get_task,
        crate::routes::review_queue::claim_task,
        crate::routes::review_queue::submit_review,
        crate::routes::review_queue::list_reviews,
        // ─── talent search v3 ─────────────────────────────────────
        // ─── explore feed ─────────────────────────────────────────
        crate::routes::explore::explore,
        // ─── admin dashboard ──────────────────────────────────────
        crate::routes::admin_dashboard::overview,
        crate::routes::admin_dashboard::financial,
        crate::routes::admin_dashboard::moderation_queue,
        crate::routes::admin_dashboard::ops_health,
        // ─── admin community (challenge review) ───────────────────
        crate::routes::admin_community::pending_review,
        crate::routes::admin_community::approve_challenge,
        crate::routes::admin_community::reject_challenge,
        // ─── DM ───────────────────────────────────────────────────
        crate::routes::dm::open_conversation,
        crate::routes::dm::list_conversations,
        crate::routes::dm::list_messages,
        crate::routes::dm::send_message,
        crate::routes::dm::mark_read,
        crate::routes::dm::block_user,
        crate::routes::dm::unblock_user,
        crate::routes::dm::list_blocks,
        // ─── talent search (v1 + v2) ──────────────────────────────
        crate::routes::talent_search_v4::search,
        crate::routes::talent_search_v4::talent_card,
        // ─── forum ────────────────────────────────────────────────
        crate::routes::forum::list_categories,
        crate::routes::forum::list_posts,
        crate::routes::forum::create_post,
        crate::routes::forum::get_post,
        crate::routes::forum::edit_post,
        crate::routes::forum::delete_post,
        crate::routes::forum::accept_answer,
        crate::routes::forum::toggle_pin,
        crate::routes::forum::toggle_lock,
        crate::routes::forum::search,
        // ─── admin users ──────────────────────────────────────────
        crate::routes::admin_users::admin_recompute_proofs,
        crate::routes::admin_users::admin_rank_override,
        // ─── projects ─────────────────────────────────────────────
        crate::routes::projects::create_project,
        crate::routes::projects::list_looking,
        crate::routes::projects::list_curated,
        crate::routes::projects::by_slug,
        crate::routes::projects::by_user,
        crate::routes::projects::by_guild_slug,
        crate::routes::projects::list_contributors,
        crate::routes::projects::add_contributor,
        crate::routes::projects::remove_contributor,
        crate::routes::projects::archive,
        crate::routes::projects::admin_set_curated,
        crate::routes::projects::my_project_recommendations,
        crate::routes::projects::mark_projects_interested,
        crate::routes::projects::list_my_project_interests,
        crate::routes::projects::unmark_project_interested,
        // ─── feed ─────────────────────────────────────────────────
        crate::routes::feed::my_feed,
        crate::routes::feed::for_you_feed,
        // ─── community challenges ─────────────────────────────────
        crate::routes::community::create_community_challenge,
        crate::routes::community::my_challenges,
        crate::routes::community::update_community_challenge,
        crate::routes::community::vote_challenge,
        crate::routes::community::unvote_challenge,
        crate::routes::community::popular_challenges,
        // ─── design (the critique loop) ───────────────────────────
        crate::routes::design::submit_version,
        crate::routes::design::history,
        crate::routes::design::review,
        crate::routes::design::reviewer_queue,
        crate::routes::design_profile::design_profile,
        crate::routes::design_profile::recompute_mine,
        crate::routes::design_profile::list_tiers,
        // ─── deliverables ─────────────────────────────────────────
        crate::routes::deliverables::get_deliverable,
        crate::routes::deliverables::list_user_deliverables,
        crate::routes::deliverables::github_slices_webhook,
        // ─── slices ───────────────────────────────────────────────
        crate::routes::slices::list_open,
        crate::routes::slices::get_slice,
        crate::routes::slices::claim_slice,
        crate::routes::slices::unclaim_slice,
        crate::routes::slices::my_slices,
        crate::routes::slices::claim_slice_as_team,
        crate::routes::slices::unclaim_slice_by_team,
        crate::routes::slices::team_slices,
        crate::routes::slices::steward_inbox,
        crate::routes::slices::publish_slice,
        crate::routes::slices::reject_slice,
        // ─── enterprise pipeline (kanban) ─────────────────────────
        crate::routes::enterprise_pipeline::list_entries,
        crate::routes::enterprise_pipeline::add_entry,
        crate::routes::enterprise_pipeline::update_entry,
        crate::routes::enterprise_pipeline::remove_entry,
        crate::routes::enterprise_pipeline::export_csv,
        // ─── moderation (inline non-admin) ────────────────────────
        crate::routes::moderation::community_review_queue,
        crate::routes::moderation::community_challenge_approve,
        crate::routes::moderation::community_challenge_reject,
        crate::routes::moderation::fraud_flagged_list,
        crate::routes::moderation::fraud_mark_valid,
        crate::routes::moderation::fraud_revoke,
        crate::routes::moderation::forum_moderate_post,
        crate::routes::moderation::forum_mute_user,
        // ─── tournament ───────────────────────────────────────────
        crate::routes::tournament::admin_create_season,
        crate::routes::tournament::admin_set_season_status,
        crate::routes::tournament::admin_close_season,
        crate::routes::tournament::list_tournaments,
        crate::routes::tournament::get_tournament,
        crate::routes::tournament::get_leaderboard,
        crate::routes::tournament::register,
        crate::routes::tournament::admin_create_tournament,
        crate::routes::tournament::admin_set_tournament_status,
        crate::routes::tournament::admin_set_score,
        crate::routes::tournament::admin_conclude,
        crate::routes::tournament::events_feed,
        // ─── profile (rank history + public profile) ──────────────
        crate::routes::profile::user_rank_history,
        crate::routes::profile::public_profile,
        // ─── user_profile (me settings) ───────────────────────────
        crate::routes::user_profile::update_profile,
        crate::routes::user_profile::upload_avatar,
        crate::routes::user_profile::delete_avatar,
        crate::routes::user_profile::get_privacy,
        crate::routes::user_profile::update_privacy,
        crate::routes::user_profile::update_display_name,
        crate::routes::user_profile::update_skill_domain,
        // ─── profile_extras (experiences, edu, langs) ─────────────
        crate::routes::profile_extras::get_availability,
        crate::routes::profile_extras::update_availability,
        crate::routes::profile_extras::list_experiences,
        crate::routes::profile_extras::add_experience,
        crate::routes::profile_extras::update_experience,
        crate::routes::profile_extras::delete_experience,
        crate::routes::profile_extras::list_educations,
        crate::routes::profile_extras::add_education,
        crate::routes::profile_extras::update_education,
        crate::routes::profile_extras::delete_education,
        crate::routes::profile_extras::list_languages,
        crate::routes::profile_extras::set_language,
        crate::routes::profile_extras::remove_language,
        crate::routes::profile_extras::clear_languages,
        // ─── social (comments, reactions, mentions, tags) ─────────
        crate::routes::social::create_comment,
        crate::routes::social::list_comments,
        crate::routes::social::edit_comment,
        crate::routes::social::delete_comment,
        crate::routes::social::toggle_reaction,
        crate::routes::social::reaction_summary,
        crate::routes::social::list_tags,
        crate::routes::social::list_target_tags,
        crate::routes::social::attach_tag,
        crate::routes::social::detach_tag,
        crate::routes::social::admin_create_tag,
        // ─── webauthn ─────────────────────────────────────────────
        crate::routes::webauthn::register_start,
        crate::routes::webauthn::register_finish,
        crate::routes::webauthn::list_credentials,
        crate::routes::webauthn::rename_credential,
        crate::routes::webauthn::delete_credential,
        crate::routes::webauthn::login_start,
        crate::routes::webauthn::login_finish,
        // ─── github integration ───────────────────────────────────
        crate::routes::github::start,
        crate::routes::github::callback,
        crate::routes::github::disconnect,
        crate::routes::github::sync_now,
        crate::routes::github::admin_sync,
        crate::routes::github::public_repos,
        crate::routes::github::cv_html,
        // ─── developer (API keys + webhooks) ──────────────────────
        crate::routes::developer::create_key,
        crate::routes::developer::list_keys,
        crate::routes::developer::revoke_key,
        crate::routes::developer::regenerate_key,
        crate::routes::developer::key_usage,
        crate::routes::developer::create_webhook,
        crate::routes::developer::list_webhooks,
        crate::routes::developer::update_webhook,
        crate::routes::developer::delete_webhook,
        crate::routes::developer::test_webhook,
        // ─── bounties (OSS) ───────────────────────────────────────
        crate::routes::bounties::list_bounties,
        crate::routes::bounties::get_bounty,
        crate::routes::bounties::create_bounty,
        crate::routes::bounties::claim_bounty,
        crate::routes::bounties::submit_pr,
        crate::routes::bounties::cancel_bounty,
        crate::routes::bounties::github_webhook,
        // ─── admin ops ────────────────────────────────────────────
        crate::routes::admin_ops::admin_sweep_proof_hooks,
        crate::routes::admin_ops::admin_trigger_gdpr_export,
        crate::routes::admin_ops::admin_recompute_capabilities,
        crate::routes::admin_ops::admin_list_badge_events,
        crate::routes::admin_ops::admin_create_badge_event,
        // ─── admin orientations ──────────────────────────────────
        crate::routes::admin_orientations::create_orientation,
        crate::routes::admin_orientations::patch_orientation,
        crate::routes::admin_orientations::attach_skill,
        crate::routes::admin_orientations::detach_skill,
        // ─── admin skills ────────────────────────────────────────
        crate::routes::admin_skills::list_skills,
        crate::routes::admin_skills::create_skill,
        crate::routes::admin_skills::update_skill,
        // ─── admin enterprises ───────────────────────────────────
        crate::routes::admin_enterprises::list_enterprises,
        crate::routes::admin_enterprises::get_enterprise,
        crate::routes::admin_enterprises::patch_type,
        crate::routes::admin_enterprises::get_type_config,
        crate::routes::admin_enterprises::list_agency_clients,
        // ─── admin badge rules ───────────────────────────────────
        crate::routes::admin_badge_rules::create_rule,
        crate::routes::admin_badge_rules::patch_rule,
        crate::routes::admin_badge_rules::deprecate_rule,
        // ─── admin fraud ─────────────────────────────────────────
        crate::routes::admin_fraud::fraud_queue,
        crate::routes::admin_fraud::mark_deliverable_valid,
        crate::routes::admin_fraud::revoke_deliverable,
        crate::routes::admin_fraud::mark_user_valid,
        crate::routes::admin_fraud::scan_deliverable_endpoint,
        crate::routes::admin_fraud::detect_multi_accounts_endpoint,
        crate::routes::admin_fraud::llm_evaluate_endpoint,
        crate::routes::admin_fraud::deep_plagiarism_scan_endpoint,
        // ─── oauth ───────────────────────────────────────────────
        crate::routes::oauth::list_my_providers,
        crate::routes::oauth::unlink_provider,
        crate::routes::oauth::google_start,
        crate::routes::oauth::google_link_start,
        crate::routes::oauth::google_callback,
        crate::routes::oauth::linkedin_start,
        crate::routes::oauth::linkedin_link_start,
        crate::routes::oauth::linkedin_callback,
        crate::routes::oauth::github_login_start,
        crate::routes::oauth::github_login_callback,
        // ─── scim ────────────────────────────────────────────────
        crate::routes::scim::create_scim_token,
        crate::routes::scim::revoke_scim_token,
        crate::routes::scim::set_group_role_mapping,
        crate::routes::scim::sp_config,
        crate::routes::scim::resource_types,
        crate::routes::scim::schemas,
        crate::routes::scim::list_users,
        crate::routes::scim::create_user,
        crate::routes::scim::get_user,
        crate::routes::scim::replace_user,
        crate::routes::scim::patch_user,
        crate::routes::scim::delete_user,
        crate::routes::scim::create_group,
        crate::routes::scim::list_groups,
        crate::routes::scim::get_group,
        crate::routes::scim::replace_group,
        crate::routes::scim::patch_group,
        crate::routes::scim::delete_group,
        // ─── admin (challenges CRUD + audit + SSO sessions + 2fa reset) ─
        crate::routes::admin::admin_reset_2fa,
        crate::routes::admin::list_audit_log,
        crate::routes::admin::create_challenge,
        crate::routes::admin::list_all_challenges,
        crate::routes::admin::update_challenge,
        crate::routes::admin::publish_challenge,
        crate::routes::admin::archive_challenge,
        crate::routes::admin::admin_stats,
        crate::routes::admin::rebuild_leaderboards,
        crate::routes::admin::list_sso_sessions,
        crate::routes::admin::revoke_sso_session,
        crate::routes::admin::admin_generate_variant,
        // ─── admin moderation ────────────────────────────────────
        crate::routes::admin_moderation::list_users,
        crate::routes::admin_moderation::get_user,
        crate::routes::admin_moderation::ban_user,
        crate::routes::admin_moderation::unban_user,
        crate::routes::admin_moderation::list_reports,
        crate::routes::admin_moderation::handle_report,
        crate::routes::admin_moderation::audit_log,
        crate::routes::admin_moderation::moderation_dashboard,
        // ─── admin projects ──────────────────────────────────────
        crate::routes::admin_projects::create_project,
        crate::routes::admin_projects::list_projects,
        crate::routes::admin_projects::get_project,
        crate::routes::admin_projects::patch_project,
        crate::routes::admin_projects::archive_project,
        // SKI-111 — was absent from the spec entirely.
        crate::routes::admin_projects::project_stats,
        // ─── contact (interest + conversations) ──────────────────
        crate::routes::contact::send_interest,
        crate::routes::contact::sent_requests,
        crate::routes::contact::received_requests,
        crate::routes::contact::accept_interest,
        crate::routes::contact::decline_interest,
        crate::routes::contact::list_conversations,
        crate::routes::contact::get_conversation,
        crate::routes::contact::send_message,
        crate::routes::contact::block_enterprise,
        crate::routes::contact::unblock_enterprise,
        // ─── enterprise KYC ──────────────────────────────────────
        crate::routes::enterprise_kyc::get_status,
        crate::routes::enterprise_kyc::upload_document,
        crate::routes::enterprise_kyc::admin_list,
        crate::routes::enterprise_kyc::admin_decide,
        // ─── enterprise SSO ──────────────────────────────────────
        crate::routes::enterprise_sso::upsert_config,
        crate::routes::enterprise_sso::get_config,
        crate::routes::enterprise_sso::disable_config,
        crate::routes::enterprise_sso::discover,
        crate::routes::enterprise_sso::start,
        crate::routes::enterprise_sso::callback,
        // ─── challenges ──────────────────────────────────────────
        crate::routes::challenges::get_challenge,
        crate::routes::challenges::get_onboarding,
        crate::routes::challenges::list_challenges,
        crate::routes::challenges::my_submissions,
        crate::routes::challenges::start_challenge,
        crate::routes::challenges::submit_challenge,
        // ─── challenge teams ─────────────────────────────────────
        crate::routes::challenge_teams::attach_team_to_guild,
        crate::routes::challenge_teams::create_persistent_team,
        crate::routes::challenge_teams::create_team,
        crate::routes::challenge_teams::create_team_slot,
        crate::routes::challenge_teams::delete_team_slot,
        crate::routes::challenge_teams::detach_team_from_guild,
        crate::routes::challenge_teams::disband_team,
        crate::routes::challenge_teams::extend_timer,
        crate::routes::challenge_teams::fill_team_slot,
        crate::routes::challenge_teams::get_team,
        crate::routes::challenge_teams::get_timer,
        crate::routes::challenge_teams::join_persistent_team,
        crate::routes::challenge_teams::join_team,
        crate::routes::challenge_teams::leave_team_slot,
        crate::routes::challenge_teams::list_open_slots_by_role,
        crate::routes::challenge_teams::list_team_slots,
        crate::routes::challenge_teams::list_teams,
        crate::routes::challenge_teams::marketplace_slots,
        crate::routes::challenge_teams::my_teams,
        crate::routes::challenge_teams::submit_team,
        // ─── enterprise ──────────────────────────────────────────
        crate::routes::enterprise::accept_invite,
        crate::routes::enterprise::delete_logo,
        crate::routes::enterprise::get_profile,
        crate::routes::enterprise::invite_preview,
        crate::routes::enterprise::invite_recruiter,
        crate::routes::enterprise::invite_register_and_accept,
        crate::routes::enterprise::list_members,
        crate::routes::enterprise::list_memberships,
        crate::routes::enterprise::register_enterprise,
        crate::routes::enterprise::revoke_member,
        crate::routes::enterprise::switch_enterprise,
        crate::routes::enterprise::update_profile,
        crate::routes::enterprise::upload_logo,
        // ─── enterprise credits ──────────────────────────────────
        crate::routes::enterprise_credits::billing_portal,
        crate::routes::enterprise_credits::create_checkout,
        crate::routes::enterprise_credits::get_credits,
        crate::routes::enterprise_credits::get_invoice,
        crate::routes::enterprise_credits::get_invoice_html,
        crate::routes::enterprise_credits::list_invoices,
        crate::routes::enterprise_credits::list_txns,
        crate::routes::enterprise_credits::public_pricing,
        crate::routes::enterprise_credits::redeem_promo,
        crate::routes::enterprise_credits::stripe_webhook,
        // ─── guild ───────────────────────────────────────────────
        crate::routes::guild::accept_invite,
        crate::routes::guild::admin_dissolve,
        crate::routes::guild::apply,
        crate::routes::guild::conclude_war,
        crate::routes::guild::create_guild,
        crate::routes::guild::create_token_link,
        crate::routes::guild::decide_application,
        crate::routes::guild::get_by_slug,
        crate::routes::guild::guild_composition,
        crate::routes::guild::invite_direct,
        crate::routes::guild::join_by_token,
        crate::routes::guild::kick_member,
        crate::routes::guild::leave_guild,
        crate::routes::guild::list_applications,
        // SKI-289 — revocation was absent from the document entirely, which
        // is why the front reported it as a missing endpoint.
        crate::routes::guild::revoke_invitation,
        crate::routes::guild::revoke_guild_invitation,
        crate::routes::guild::list_for_leaderboard,
        crate::routes::guild::list_invitations,
        crate::routes::guild::list_members,
        crate::routes::guild::list_wars,
        crate::routes::guild::promote_member,
        crate::routes::guild::propose_war,
        crate::routes::guild::respond_war,
        // ─── mentorship ──────────────────────────────────────────
        crate::routes::mentorship::add_availability,
        crate::routes::mentorship::book_session,
        crate::routes::mentorship::cancel_session,
        crate::routes::mentorship::connect_status,
        crate::routes::mentorship::get_mentor_profile,
        crate::routes::mentorship::get_my_mentor_profile,
        crate::routes::mentorship::list_mentors,
        crate::routes::mentorship::list_my_sessions,
        crate::routes::mentorship::mark_completed,
        crate::routes::mentorship::start_connect_onboarding,
        crate::routes::mentorship::submit_review,
        crate::routes::mentorship::upsert_my_mentor_profile,
        // ─── talent wallet ───────────────────────────────────────
        crate::routes::talent_wallet::my_wallet,
        crate::routes::talent_wallet::my_wallet_transactions,
        crate::routes::talent_wallet::register_momo_phone,
        crate::routes::talent_wallet::set_my_residency,
        crate::routes::talent_wallet::stripe_connect_webhook,
        crate::routes::talent_wallet::stripe_onboard,
        crate::routes::talent_wallet::withdraw,
        crate::routes::talent_wallet::wallet_statement_csv,
        crate::routes::payment_webhooks::receive,
        crate::routes::email_preview::preview,
        crate::routes::email_preview::index,
        crate::routes::disputes::raise,
        crate::routes::disputes::concede,
        crate::routes::disputes::contest,
        crate::routes::disputes::withdraw,
        crate::routes::disputes::mine,
        crate::routes::disputes::decide,
        crate::routes::disputes::queue,
        crate::routes::payments::methods,
        crate::routes::payments::charge,
        crate::routes::payments::status,
        crate::routes::admin_money::overview,
        crate::routes::admin_money::payments,
        crate::routes::admin_money::payouts,
        crate::routes::admin_money::routes,
        crate::routes::admin_money::toggle_route,
        crate::routes::admin_money::methods,
        crate::routes::admin_money::toggle_method,
    ),
    components(
        schemas(
            ErrorResponse,
            ErrorObject,
            MetaInfo,
            SimpleMessage,
            ApiResponse<SimpleMessage>,
            // SKI-291 — the profile page reads `github_repo_owner` /
            // `github_repo_name` off these, so the shape has to be published.
            crate::services::projects::Project,
            crate::routes::projects::UserProjectsData,
            crate::routes::projects::UserProjectsResponse,
            // SKI-293 — the decide contract was untyped, so the only way to
            // find the field name was to probe the running API.
            crate::routes::notification_preferences::KindPreference,
            crate::routes::notification_preferences::PreferencesData,
            crate::routes::notification_preferences::PreferencesResponse,
            crate::routes::notification_preferences::PreferenceUpdate,
            crate::routes::notification_preferences::UpdatePreferencesRequest,
            crate::routes::notification_preferences::UpdateResult,
            crate::routes::notification_preferences::ResetResult,
            crate::routes::guild::DecideBody,
            crate::routes::guild::DecidedApplicationData,
            crate::routes::guild::DecidedApplicationResponse,
            crate::services::guild::GuildApplication,
            // SKI-293 — mention inbox, shipped in SKI-286 without a schema.
            crate::services::mentions::Mention,
            crate::services::mentions::MentionAuthor,
            crate::routes::mentions::MentionListResponse,
            crate::routes::mentions::MentionRead,
            crate::routes::mentions::MentionReadResponse,
            crate::routes::mentions::MentionsMarked,
            crate::routes::mentions::MentionsMarkedResponse,
            crate::routes::auth::ForgotPasswordRequest,
            crate::routes::auth::VerifyEmailQuery,
            crate::routes::auth::ResetPasswordRequest,
            crate::routes::auth::ChangePasswordRequest,
            crate::routes::auth::ListSessionsResponse,
            crate::services::session::SessionRow,
            crate::routes::auth::TotpDisableRequest,
            crate::routes::auth::PasswordConfirmRequest,
            crate::routes::auth::Email2faEnableRequest,
            crate::routes::auth::MeResponse,
            crate::routes::auth::RankInfo,
            crate::models::UserPrivate,
            crate::routes::auth::ChangeEmailRequest,
            crate::routes::auth::ConfirmEmailChangeQuery,
            crate::routes::auth::RefreshResponse,
            crate::routes::auth::CompleteProfileRequest,
            crate::routes::auth::CompleteProfileResponse,
            crate::routes::auth::TotpCodeRequest,
            crate::routes::auth::TotpSetupResponse,
            crate::routes::auth::TotpEnableResponse,
            crate::routes::auth::DeleteAccountRequest,
            crate::routes::auth::DeleteAccountResponse,
            crate::routes::auth::DataExportResponse,
            crate::routes::auth::RegisterRequest,
            crate::routes::auth::RegisterResponse,
            crate::routes::auth::LoginRequest,
            crate::routes::auth::LoginSuccessResponse,
            crate::routes::auth::LoginPending2faResponse,
            crate::routes::auth::LoginOutcome,
            crate::routes::auth::Email2faVerifyRequest,
            crate::routes::health::LivenessResponse,
            crate::routes::health::DeepHealthResponse,
            crate::routes::health::HealthServices,
            crate::routes::health::ServiceHealth,
            crate::routes::health::WebsocketStats,
            crate::routes::magic_link::MagicLinkRequestBody,
            crate::routes::magic_link::MagicLinkRequestResponse,
            crate::routes::magic_link::MagicLinkConsumeBody,
            crate::routes::magic_link::MagicLinkConsumeResponse,
            crate::routes::geo::CityOut,
            crate::services::geo::Country,
            crate::routes::i18n::LocalesResponse,
            crate::routes::i18n::LocaleEntry,
            crate::routes::email_prefs::EmailPrefs,
            crate::routes::email_prefs::AdminDigestResponse,
            crate::services::digest::DigestRunReport,
            crate::routes::metrics::MetricsSummary,
            crate::routes::metrics::UsersStats,
            crate::routes::metrics::ChallengesStats,
            crate::routes::metrics::ModerationStats,
            crate::routes::metrics::MessagingStats,
            crate::routes::metrics::MetricsWebsocketStats,
            crate::routes::metrics::DatabasePoolStats,
            crate::services::tracks::Track,
            crate::services::tracks::UserTrack,
            crate::services::tracks::TrackProgress,
            crate::services::tracks::EligibilityCheck,
            crate::routes::tracks::TracksListResponse,
            crate::routes::tracks::TrackDetailResponse,
            crate::routes::tracks::EnrollResponse,
            crate::routes::tracks::TrackProgressResponse,
            crate::routes::tracks::MyTrackEntry,
            crate::routes::tracks::MyTracksResponse,
            crate::routes::tracks::EligibilityResponse,
            crate::models::Notification,
            crate::routes::notifications::Pagination,
            crate::routes::notifications::NotificationsListResponse,
            crate::routes::notifications::UnreadCountResponse,
            crate::routes::legal::LegalPages,
            crate::routes::legal::ConsentVersionResponse,
            crate::routes::legal::ConsentBody,
            crate::routes::legal::ConsentRecordedResponse,
            crate::routes::badges::BadgeItem,
            crate::routes::badges::RankRow,
            crate::routes::badges::UserBadgesResponse,
            crate::routes::badges::RuleCatalogRow,
            crate::routes::badges::RulesCatalogResponse,
            crate::routes::reports::CreateReportRequest,
            crate::routes::reports::Report,
            crate::routes::reports::CreateReportResponse,
            crate::routes::reports::MyReportsResponse,
            crate::routes::capabilities::CapabilityRow,
            crate::routes::capabilities::UserCapabilitiesResponse,
            crate::routes::capabilities::GrantBody,
            crate::routes::capabilities::CapabilityGrantResponse,
            crate::routes::capabilities::CapabilityRevokeResponse,
            // events
            crate::routes::events::EventRow,
            crate::routes::events::EventsListResponse,
            crate::routes::events::JoinEventResponse,
            crate::routes::events::MyEventRow,
            crate::routes::events::MyEventsResponse,
            // skills
            crate::models::SkillNode,
            crate::services::skills::UserSkillEnriched,
            crate::services::skills::SkillTalent,
            crate::services::skills::SliceRecommendation,
            crate::services::skills::RecommendationSkillMatch,
            crate::routes::skills::SkillsListResponse,
            crate::routes::skills::SkillTalentsResponse,
            crate::routes::skills::UserSkillsResponse,
            crate::routes::skills::SkillRecommendationsResponse,
            // ai jobs
            crate::routes::ai_jobs::CodeReviewBody,
            crate::routes::ai_jobs::AiJobEnqueuedResponse,
            crate::routes::ai_jobs::AiJobResultResponse,
            // attestations
            crate::services::attestations::Attestation,
            crate::routes::attestations::UserAttestationsResponse,
            crate::routes::attestations::AttestationVerifyResponse,
            crate::routes::attestations::CompagnonnageBody,
            crate::routes::attestations::IssueAttestationResponse,
            crate::routes::attestations::RevokeBody,
            crate::routes::attestations::RevokeAttestationResponse,
            // push
            crate::routes::push::VapidPublicKeyResponse,
            crate::routes::push::SubscribeBody,
            crate::routes::push::SubscribeResponse,
            crate::routes::push::UnsubscribeResponse,
            crate::routes::push::RegisterMobileTokenBody,
            crate::routes::push::MobileTokenRegisteredResponse,
            crate::routes::push::MobileTokenRevokedResponse,
            crate::routes::push::MobileTokenSummary,
            crate::routes::push::MobileTokensListResponse,
            // seasons + stewards
            crate::services::seasons::Season,
            crate::services::stewards::ProjectSteward,
            crate::routes::seasons::SeasonsListResponse,
            crate::routes::seasons::SeasonResponse,
            crate::routes::seasons::CurrentSeasonResponse,
            crate::routes::seasons::CreateSeasonBody,
            crate::routes::seasons::StewardsListResponse,
            crate::routes::seasons::StewardResponse,
            crate::routes::seasons::AddStewardBody,
            crate::routes::seasons::StewardshipsResponse,
            // agency_clients
            crate::routes::agency_clients::TypeConfigResponse,
            crate::routes::agency_clients::TypeConfigUpdatedResponse,
            crate::routes::agency_clients::AgencyClientRow,
            crate::routes::agency_clients::AgencyClientsListResponse,
            crate::routes::agency_clients::CreateBody,
            crate::routes::agency_clients::AgencyClientCreatedResponse,
            crate::routes::agency_clients::UpdateBody,
            crate::routes::agency_clients::AgencyClientUpdatedResponse,
            crate::routes::agency_clients::AgencyClientDeactivatedResponse,
            // ai coach
            crate::routes::ai_coach::AiCoachEnvelope,
            crate::routes::ai_coach::SuggestBody,
            // certifications
            crate::routes::certifications::CertificationRow,
            crate::routes::certifications::CertificationsListResponse,
            crate::routes::certifications::PurchaseResponse,
            crate::routes::certifications::StartAttemptResponse,
            crate::routes::certifications::SubmitAttemptResponse,
            crate::routes::certifications::DiplomaHolder,
            crate::routes::certifications::DiplomaCertification,
            crate::routes::certifications::VerifyDiplomaResponse,
            crate::routes::certifications::MyDiplomaRow,
            crate::routes::certifications::MyDiplomasResponse,
            // leaderboard
            crate::routes::leaderboard::LeaderboardMeta,
            crate::routes::leaderboard::LeaderboardsIndexResponse,
            crate::routes::leaderboard::LeaderboardEntry,
            crate::routes::leaderboard::LeaderboardPage,
            crate::routes::leaderboard::LeaderboardPageResponse,
            crate::routes::leaderboard::MyRankResponse,
            // legal_well_known
            crate::routes::legal_well_known::DashboardCredits,
            crate::routes::legal_well_known::DashboardInterestRequests,
            crate::routes::legal_well_known::DashboardOverviewResponse,
            crate::routes::legal_well_known::DashboardFunnel,
            crate::routes::legal_well_known::DashboardFunnelResponse,
            // sponsored challenges
            crate::routes::sponsored_challenges::RequestBody,
            crate::routes::sponsored_challenges::RequestCreatedResponse,
            crate::routes::sponsored_challenges::SponsorshipRequestRow,
            crate::routes::sponsored_challenges::MyRequestsResponse,
            crate::routes::sponsored_challenges::AdminSponsorshipRow,
            crate::routes::sponsored_challenges::DecideBody,
            crate::routes::sponsored_challenges::DecideResponse,
            crate::routes::sponsored_challenges::LinkChallengeBody,
            crate::routes::sponsored_challenges::LinkChallengeResponse,
            crate::routes::sponsored_challenges::ActiveSponsoredRow,
            crate::routes::sponsored_challenges::ActiveSponsoredResponse,
            crate::routes::sponsored_challenges::SponsorSubmissionRow,
            crate::routes::sponsored_challenges::SponsorSubmissionsResponse,
            // orientations
            crate::routes::domain_profile::DomainProfileBody,
            crate::routes::domain_profile::DomainProfileResponse,
            crate::services::mentorship_matching::Match,
            crate::routes::ai::ToolkitRow,
            crate::routes::benchmarks::BenchmarkRow,
            crate::routes::benchmarks::RecordBenchmarkBody,
            crate::routes::benchmarks::ReproduceBody,
            crate::routes::ai_safety::SafetyReportRow,
            crate::routes::ai_safety::RecordReportBody,
            crate::routes::ai_safety::DisclosureBody,
            crate::routes::ai::ToolkitResponse,
            crate::routes::ai::CompetitionRow,
            crate::routes::ai::CompetitionsResponse,
            crate::routes::ai::ArtifactRow,
            crate::routes::ai::ArtifactsResponse,
            crate::services::ai_profile::AiProfile,
            crate::services::audio_profile::AudioProfile,
            crate::services::audio_profile::AudioHighlight,
            crate::routes::audio::FileRow,
            crate::routes::audio::SourceRow,
            crate::routes::audio::DeclareSourceBody,
            crate::routes::audio::RevisionRow,
            crate::routes::audio::RequestRevisionBody,
            crate::routes::audio::ResolveRevisionBody,
            crate::routes::audio::CastingRow,
            crate::routes::audio::OpenCastingBody,
            crate::routes::audio::AuditionBody,
            crate::routes::audio::SelectVoiceBody,
            crate::routes::audio::CreditBody,
            crate::routes::audio::PortfolioRow,
            crate::routes::audio::CreditRow,
            crate::routes::audio::DeclarePortfolioBody,
            crate::routes::domain_profile::QuestionSpec,
            crate::services::craft_score::Term,
            crate::routes::code_profile::ClaimBody,
            crate::routes::public_feed::PreferenceBody,
            crate::routes::revenue::RevenueStream,
            crate::routes::talent_line::LogHoursBody,
            crate::routes::talent_line::HoursDecisionBody,
            crate::routes::talent_line::ConcludeBody,
            crate::routes::talent_line::PitchResponseBody,
            crate::routes::recruitment::HireBody,
            crate::routes::recruitment::ResponseBody,
            crate::routes::recruitment::AssignBody,
            crate::routes::recruitment::ShortlistBody,
            crate::routes::recruitment::DepartureBody,
            crate::routes::ops_practice::CloseBody,
            crate::routes::ops_practice::ResolveBody,
            crate::routes::ops_practice::ActionBody,
            crate::routes::ops_practice::PostmortemBody,
            crate::routes::ops_practice::CostVerdictBody,
            crate::routes::ops_practice::ArtefactAttestationBody,
            crate::routes::ops_practice::FeaturedBody,
            crate::routes::credentials::ReviewBody,
            crate::routes::ats::SubscribeBody,
            crate::routes::ats::MoveBody,
            crate::services::ats::OpeningInput,
            crate::services::ats::CandidateInput,
            crate::services::ats::Plan,
            crate::services::credentials::Credential,
            crate::services::credentials::CredentialInput,
            crate::services::ops_onboarding::WizardAnswers,
            crate::services::ops_onboarding::Recommendation,
            crate::routes::sales_pipeline::StageBody,
            crate::routes::additional_products::RespondBody,
            crate::routes::additional_products::MonthBody,
            crate::routes::additional_products::EndBody,
            crate::routes::additional_products::LearningBody,
            crate::routes::additional_products::SeatBody,
            crate::routes::additional_products::DecisionBody,
            crate::routes::additional_products::AwardBody,
            crate::routes::continuous::RespondBody,
            crate::routes::continuous::CheckInBody,
            crate::routes::continuous::RetentionBody,
            crate::routes::continuous::ContributionBody,
            crate::routes::continuous::JudgementBody,
            crate::routes::continuous::SettleBody,
            crate::routes::continuous::MemberBody,
            crate::routes::continuous::InterestBody,
            crate::routes::continuous::SignatureBody,
            crate::routes::consultations::RatingBody,
            crate::routes::consultations::InviteBody,
            crate::routes::consultations::RespondBody,
            crate::routes::consultations::OpinionBody,
            crate::routes::consultations::DeliverBody,
            crate::routes::consultations::InformBody,
            crate::routes::consultations::AuditDeliveryBody,
            crate::routes::consultations::AssessmentResponseBody,
            crate::routes::mentoring_products::HoursBody,
            crate::routes::mentoring_products::CommissionBody,
            crate::routes::mentoring_products::SlotBody,
            crate::routes::mentoring_products::EnrolBody,
            crate::routes::ecosystem::AuditBody,
            crate::routes::ecosystem::ReasonBody,
            crate::routes::ecosystem::RatingBody,
            crate::routes::finance_line::AdvanceBody,
            crate::routes::finance_line::GuaranteeBody,
            crate::routes::finance_line::DecisionBody,
            crate::routes::finance_line::WriteOffBody,
            crate::routes::finance_line::ClaimBody,
            crate::routes::data_line::DataConsentBody,
            crate::routes::data_line::DeliverBody,
            crate::routes::data_line::SettleBody,
            crate::routes::contests::StatusBody,
            crate::routes::contests::InviteBody,
            crate::routes::contests::HireBody,
            crate::routes::contests::OutcomeBody,
            crate::routes::contests::RespondBody,
            crate::routes::contests::SubmitBody,
            crate::routes::contests::ConfirmBody,
            crate::routes::contests::DeclineBody,
            crate::routes::events::JoinBody,
            crate::routes::events::AppointBody,
            crate::routes::events::StatusBody,
            crate::routes::brand::StandVisitBody,
            crate::routes::brand::ReasonBody,
            crate::routes::brand::PublishBody,
            crate::routes::brand::DecisionBody,
            crate::routes::brand::QualityBody,
            crate::routes::brand::RespondBody,
            crate::routes::brand::InviteBody,
            crate::routes::brand::StipendBody,
            crate::routes::brand::SubscribeBody,
            crate::routes::brand::CancelBody,
            crate::routes::brand::Money,
            crate::routes::engagements::MemberBody,
            crate::routes::engagements::ActivateBody,
            crate::routes::engagements::ReasonBody,
            crate::routes::engagements::RespondBody,
            crate::routes::engagements::ReviewBody,
            crate::routes::engagements::FeedbackBody,
            crate::routes::engagements::FeedbackVerdictBody,
            crate::routes::enterprise_products::EnterpriseProduct,
            crate::routes::enterprise_products::RecordProductBody,
            crate::routes::enterprise_products::ProductStatusBody,
            crate::routes::missions::StatusBody,
            crate::routes::missions::DecisionBody,
            crate::routes::awards::NominateBody,
            crate::routes::awards::ShortlistBody,
            crate::routes::awards::VoteAccepted,
            crate::routes::code::FirstIssueRow,
            crate::routes::code::FirstIssuesResponse,
            crate::routes::code::EcosystemRow,
            crate::routes::guides::GuideSummary,
            crate::routes::guides::Guide,
            crate::routes::code::EcosystemsResponse,
            crate::routes::orientations::OrientationRow,
            crate::routes::orientations::CatalogPagination,
            crate::routes::orientations::OrientationsCatalogResponse,
            crate::routes::orientations::OrientationSkillEntry,
            crate::routes::orientations::OrientationDetailResponse,
            crate::routes::orientations::UserOrientationRow,
            crate::routes::orientations::MyOrientationsResponse,
            crate::routes::orientations::RegisterBody,
            crate::routes::orientations::RegisterOrientationResponse,
            crate::routes::orientations::UpdateBody,
            crate::routes::orientations::UpdateOrientationResponse,
            crate::routes::orientations::EndOrientationResponse,
            crate::routes::orientations::PublicUserOrientationRow,
            crate::routes::orientations::PublicUserOrientationsResponse,
            // talent lists
            crate::models::TalentList,
            crate::routes::talent_lists::CreateListRequest,
            crate::routes::talent_lists::UpdateListRequest,
            crate::routes::talent_lists::BookmarkedTalent,
            crate::routes::talent_lists::BookmarksPageResponse,
            crate::routes::talent_lists::ListResponse,
            crate::routes::talent_lists::TalentListSummary,
            crate::routes::talent_lists::ListsResponse,
            crate::routes::talent_lists::ListMemberTalent,
            crate::routes::talent_lists::ListDetailResponse,
            // tenants
            crate::routes::tenants::PublicTenant,
            crate::routes::tenants::AdminTenant,
            crate::routes::tenants::AdminTenantSummary,
            crate::routes::tenants::CreateTenantBody,
            crate::routes::tenants::TenantCreatedResponse,
            crate::routes::tenants::UpdateTenantBody,
            crate::routes::tenants::TenantUpdatedResponse,
            crate::routes::tenants::TenantMemberRow,
            crate::routes::tenants::MembersResponse,
            crate::routes::tenants::AddMemberBody,
            crate::routes::tenants::MemberAddedResponse,
            crate::routes::tenants::CohortRow,
            crate::routes::tenants::CohortsResponse,
            crate::routes::tenants::CreateCohortBody,
            crate::routes::tenants::CohortCreatedResponse,
            // onboarding
            crate::routes::onboarding::OnboardingProgress,
            crate::routes::onboarding::StartNextSteps,
            crate::routes::onboarding::StartBonjourResponse,
            crate::routes::onboarding::StatusBonjourResponse,
            // challenge tags
            crate::models::ChallengeTemplate,
            crate::routes::challenge_tags::TagWithCount,
            crate::routes::challenge_tags::TagsResponse,
            crate::routes::challenge_tags::CategoryRow,
            crate::routes::challenge_tags::CategoriesResponse,
            crate::routes::challenge_tags::FeaturedChallengesResponse,
            // sandbox
            crate::services::sandbox::LanguageInfo,
            crate::services::sandbox::ExecutionResult,
            crate::services::sandbox::ExecutionStatus,
            crate::routes::sandbox::ExecuteRequest,
            crate::routes::sandbox::ExecuteResponse,
            crate::routes::sandbox::AsyncExecuteResponse,
            crate::routes::sandbox::AsyncResultResponse,
            crate::routes::sandbox::LanguagesResponse,
            // admin content ops
            crate::routes::admin_content_ops::MirrorFailedDetail,
            crate::routes::admin_content_ops::HelloWallMirrorReport,
            crate::routes::admin_content_ops::ProfileReadmeSyncReport,
            crate::routes::admin_content_ops::BadgeRecomputeReport,
            // enterprise subscriptions
            crate::routes::enterprise_subscriptions::SubscribeBody,
            crate::routes::enterprise_subscriptions::SubscribeResponse,
            crate::routes::enterprise_subscriptions::SubscriptionDetail,
            crate::routes::enterprise_subscriptions::CurrentSubscriptionResponse,
            crate::routes::enterprise_subscriptions::CancelSubscriptionResponse,
            // enterprise dashboard
            crate::routes::enterprise_dashboard::DomainBucket,
            crate::routes::enterprise_dashboard::TitleBucket,
            crate::routes::enterprise_dashboard::PlatformStatsResponse,
            crate::routes::enterprise_dashboard::InterestRequestsBreakdown,
            crate::routes::enterprise_dashboard::MyStatsResponse,
            // gamification
            crate::routes::gamification::SkillLeaf,
            crate::routes::gamification::DomainBranch,
            crate::routes::gamification::SkillTreeUser,
            crate::routes::gamification::SkillTreeResponse,
            crate::routes::gamification::ActivityDay,
            crate::routes::gamification::HeatmapSummary,
            crate::routes::gamification::HeatmapResponse,
            // public API v1
            crate::models::BadgeWithEarnedAt,
            crate::routes::public_api::V1Meta,
            crate::routes::public_api::V1UserProfile,
            crate::routes::public_api::V1UserProfileResponse,
            crate::routes::public_api::V1UserBadgesResponse,
            crate::routes::public_api::V1SkillLeaf,
            crate::routes::public_api::V1DomainBranch,
            crate::routes::public_api::V1UserSkillsResponse,
            // review queue
            crate::services::review_queue::ReviewTask,
            crate::services::reviews::SubmitOutcome,
            crate::routes::review_queue::TasksListResponse,
            crate::routes::review_queue::TaskResponse,
            crate::routes::review_queue::ClaimResponse,
            crate::routes::review_queue::SubmitReviewBody,
            crate::routes::review_queue::SubmitReviewResponse,
            crate::routes::review_queue::ReviewRow,
            crate::routes::review_queue::ReviewsListResponse,
            // talent search v3
            // explore
            crate::routes::explore::ExploreItem,
            crate::routes::explore::ExplorePage,
            crate::routes::explore::ExploreResponse,
            // admin dashboard
            crate::routes::admin_dashboard::AdminOverviewResponse,
            crate::routes::admin_dashboard::PurchaseBreakdownRow,
            crate::routes::admin_dashboard::AdminFinancialResponse,
            crate::routes::admin_dashboard::ModerationQueueResponse,
            crate::routes::admin_dashboard::DbPoolInfo,
            crate::routes::admin_dashboard::WsInfo,
            crate::routes::admin_dashboard::OpsHealthResponse,
            // admin community
            crate::routes::admin_community::RejectRequest,
            crate::routes::admin_community::CreatorSummary,
            crate::routes::admin_community::EnrichedChallenge,
            crate::routes::admin_community::AdminChallengeDecisionResponse,
            // DM
            crate::services::dm::DmConversation,
            crate::services::dm::DmMessage,
            crate::services::dm::ConversationSummary,
            crate::services::dm::UserBlock,
            crate::routes::dm::OpenConversationBody,
            crate::routes::dm::OpenConversationResponse,
            crate::routes::dm::ConversationsResponse,
            crate::routes::dm::MessagesResponse,
            crate::routes::dm::SendMessageBody,
            crate::routes::dm::SendMessageResponse,
            crate::routes::dm::MarkReadResponse,
            crate::routes::dm::BlockBody,
            crate::routes::dm::BlockedResponse,
            crate::routes::dm::UnblockedResponse,
            crate::routes::dm::BlocksResponse,
            // talent_search
            crate::routes::talent_search_v4::Talent,
            crate::routes::talent_search_v4::SearchResponse,
            crate::routes::talent_search_v4::CardScore,
            crate::routes::talent_search_v4::TalentCardTopSkill,
            crate::routes::talent_search_v4::TalentCardResponse,
            // forum
            crate::routes::forum::CreatePostBody,
            crate::routes::forum::EditPostBody,
            crate::routes::forum::AcceptAnswerBody,
            crate::routes::forum::TogglePinBody,
            crate::routes::forum::ToggleLockBody,
            // admin users
            crate::routes::admin_users::RecomputeBody,
            crate::routes::admin_users::RankOverrideBody,
            // projects
            crate::routes::projects::AddContributorBody,
            crate::routes::projects::SetCuratedBody,
            crate::routes::projects::MarkInterestedBody,
            // community
            crate::routes::community::CreateCommunityChallenge,
            crate::routes::community::UpdateCommunityChallenge,
            // slices
            crate::routes::slices::ClaimAsTeamBody,
            // enterprise_pipeline
            crate::routes::enterprise_pipeline::AddEntryBody,
            crate::routes::enterprise_pipeline::UpdateEntryBody,
            // moderation
            crate::routes::moderation::RejectBody,
            crate::routes::moderation::ReasonBody,
            crate::routes::moderation::ModeratePostBody,
            crate::routes::moderation::MuteUserBody,
            // tournament
            crate::routes::tournament::StatusBody,
            crate::routes::tournament::RegisterBody,
            crate::routes::tournament::ScoreBody,
            // user_profile
            crate::routes::user_profile::UpdateProfileRequest,
            crate::routes::user_profile::UpdateDisplayNameRequest,
            crate::routes::user_profile::UpdateSkillDomainRequest,
            crate::routes::user_profile::UpdatePrivacyRequest,
            crate::routes::user_profile::PrivacySettings,
            // profile_extras
            crate::routes::profile_extras::AvailabilityBody,
            crate::routes::profile_extras::LanguageBody,
            // social
            crate::routes::social::CreateCommentBody,
            crate::routes::social::EditCommentBody,
            crate::routes::social::ToggleReactionBody,
            crate::routes::social::TagMapBody,
            // webauthn
            crate::routes::webauthn::RegisterStartRequest,
            crate::routes::webauthn::RenameRequest,
            crate::routes::webauthn::LoginStartRequest,
            // developer
            crate::routes::developer::CreateKeyRequest,
            crate::routes::developer::CreateWebhookRequest,
            crate::routes::developer::UpdateWebhookRequest,
            // bounties
            crate::routes::bounties::CreateBountyBody,
            crate::routes::bounties::SubmitPrBody,
            // admin ops
            crate::routes::admin_ops::SweepQuery,
            crate::routes::admin_ops::GdprExportBody,
            crate::routes::admin_ops::ListEventsQuery,
            crate::routes::admin_ops::CreateEventBody,
        ),
    ),
    tags(
        (name = "auth",         description = "Authentication, session, 2FA, passkeys"),
        (name = "profile",      description = "Public + private user profile"),
        (name = "challenges",   description = "Challenges, submissions, review queue"),
        (name = "projects",     description = "Real OSS projects → slices → deliverables"),
        (name = "forum",        description = "Q&A forum with accepted answers"),
        (name = "dm",           description = "Direct messages"),
        (name = "social",       description = "Follows, contact requests, blocks"),
        (name = "guilds",       description = "Persistent teams, wars, applications"),
        (name = "feed",         description = "For-you feed + explore"),
        (name = "enterprise",   description = "B2B credits, invoices, KYC, SSO"),
        (name = "wallet",       description = "Talent payouts (Stripe Connect, Momo)"),
        (name = "moderation",   description = "Community moderation surface"),
        (name = "admin",        description = "Admin panel — requires 2FA + admin origin"),
        (name = "webhooks",     description = "Inbound webhooks (Stripe, GitHub, Brevo)"),
        (name = "health",       description = "Liveness, readiness, metrics"),
    ),
)]
pub struct ApiDoc;

/// Security-scheme modifier. Registers the two auth mechanisms used across
/// the API:
///
/// - **`cookie_auth`**  — the HttpOnly `access_token` cookie set by
///   `/api/auth/login` and refreshed by `/api/auth/refresh`. Used by both
///   frontends (skilluv-frontend, skilluv-admin).
/// - **`bearer_auth`** — API-key bearer token used by third parties on the
///   public API surface (`/api/v1/*`).
///
/// Individual handlers reference these via `security(("cookie_auth" = []))`
/// in their `#[utoipa::path]` annotation.
/// Injecte les réponses d'erreur communes (4xx/5xx) sur *toutes* les
/// operations du document OpenAPI. Motivation :
///
/// Chaque handler du crate peut retourner `AppError` (~30 variants mappés à
/// ~10 codes HTTP distincts) via `?`, mais utoipa ne peut pas inférer ça
/// depuis le type de retour `Result<..., AppError>`. Déclarer manuellement
/// `responses(...)` sur 500+ handlers pour lister 400/401/403/404/500 est
/// intenable et se déphase à la moindre refacto.
///
/// Ce modifier remplit le trou : il ajoute les codes d'erreur communs à
/// chaque operation, SAUF si le handler a déjà déclaré ce code (auquel cas
/// on respecte la déclaration explicite, souvent plus précise). Résultat :
/// schemathesis ne râle plus sur un 400 renvoyé mais non-documenté.
///
/// Non-régressif : on ne touche pas aux 2xx explicites, on n'écrase jamais
/// une déclaration existante, on ne change pas les schémas de body.
struct CommonErrorResponsesAddon;

impl Modify for CommonErrorResponsesAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::response::ResponseBuilder;

        // (status, description) — description bien lue par les IDE Swagger,
        // pas critique pour la conformité schemathesis.
        const COMMON_ERRORS: &[(&str, &str)] = &[
            ("400", "Validation error / bad request payload"),
            ("401", "Authentication required or token invalid"),
            ("403", "Forbidden — caller lacks the required capability"),
            ("404", "Resource not found"),
            ("409", "Conflict — resource state prevents the operation"),
            ("422", "Semantic validation error (well-formed but invalid)"),
            ("429", "Rate limit exceeded"),
            ("500", "Internal server error"),
        ];

        for path_item in openapi.paths.paths.values_mut() {
            let ops = [
                path_item.get.as_mut(),
                path_item.put.as_mut(),
                path_item.post.as_mut(),
                path_item.delete.as_mut(),
                path_item.patch.as_mut(),
                path_item.options.as_mut(),
                path_item.head.as_mut(),
                path_item.trace.as_mut(),
            ];
            for op in ops.into_iter().flatten() {
                for (status, desc) in COMMON_ERRORS {
                    op.responses
                        .responses
                        .entry(status.to_string())
                        .or_insert_with(|| {
                            ResponseBuilder::new().description(*desc).build().into()
                        });
                }
            }
        }
    }
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("components populated by derive");
        components.add_security_scheme(
            "cookie_auth",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("access_token"))),
        );
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

/// Attach the OpenAPI JSON endpoint + Swagger UI to a router.
///
/// SKI-57 (2026-08-10): default changed from "hidden in release" to
/// "exposed by default, hide-out via `SKILLUV_HIDE_SWAGGER=1`". Skilluv
/// wants a documented public API surface — external integrators land on
/// `/api/docs` and read the schema. The old behaviour (hidden by default
/// in release) meant no human could reach the UI in prod without an
/// env-var change, which defeats the purpose of shipping utoipa.
///
/// Historic behaviour preserved via env opt-out: set
/// `SKILLUV_HIDE_SWAGGER=1` to hide the UI (spec JSON still served at
/// `/api/openapi.json` for schemathesis / integrators using the raw file).
pub fn attach<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let hide_swagger = std::env::var("SKILLUV_HIDE_SWAGGER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // Back-compat: old `SKILLUV_EXPOSE_SWAGGER=0` also hides. Silent
    // migration — no deployer needs to change their env.
    let legacy_hide = std::env::var("SKILLUV_EXPOSE_SWAGGER")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    let expose_swagger = !(hide_swagger || legacy_hide);

    // Only ONE of the two branches registers /api/openapi.json — either via
    // SwaggerUi.url() (which serves both the UI and the spec) OR via a manual
    // route (spec only, no UI). Registering both simultaneously causes axum
    // to panic with 'Overlapping method route' the first time any test calls
    // build_router().
    if expose_swagger {
        router.merge(SwaggerUi::new("/api/docs").url("/api/openapi.json", ApiDoc::openapi()))
    } else {
        // Prod-mode : ship the spec JSON at /api/openapi.json but no Swagger
        // UI shell. Schemathesis in CI reads the JSON directly ; humans get 404.
        router.route("/api/openapi.json", axum::routing::get(openapi_json))
    }
}

async fn openapi_json() -> impl IntoResponse {
    axum::Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use utoipa::OpenApi;

    /// utoipa names a component after the Rust type alone and an operation
    /// after the handler's function name, and neither is unique in a codebase
    /// this size: 51 structs collapsed onto 18 component names and 126
    /// handlers onto 56 operation ids, each collision quietly overwriting the
    /// last. Nothing failed — the spec simply described the wrong endpoint,
    /// and a client generated from it called something else.
    ///
    /// Both are one `#[schema(as = ...)]` or `operation_id = "..."` away from
    /// correct. This is what notices the next one.

    #[test]
    fn no_two_operations_share_an_id() {
        // Read through the serialised document rather than utoipa's typed
        // `PathItem`, whose per-method fields move between minor versions.
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let mut seen: HashMap<String, Vec<String>> = HashMap::new();
        for (path, item) in doc["paths"].as_object().unwrap() {
            for (method, op) in item.as_object().unwrap() {
                if let Some(id) = op.get("operationId").and_then(|v| v.as_str()) {
                    seen.entry(id.to_string())
                        .or_default()
                        .push(format!("{method} {path}"));
                }
            }
        }
        let clashes: Vec<_> = seen.iter().filter(|(_, v)| v.len() > 1).collect();
        assert!(
            seen.len() > 700,
            "only {} operation ids read — the document is not being walked",
            seen.len()
        );
        assert!(
            clashes.is_empty(),
            "operationId must be unique across the document: {clashes:#?}"
        );
    }

    /// The same defect on the schema side is invisible from the document
    /// alone — the loser of a collision is simply absent. What is checkable is
    /// that every component another part of the document points at exists.
    #[test]
    fn every_referenced_schema_is_defined() {
        let doc = ApiDoc::openapi();
        let defined: std::collections::HashSet<String> = doc
            .components
            .as_ref()
            .map(|c| c.schemas.keys().cloned().collect())
            .unwrap_or_default();

        let json = serde_json::to_string(&doc).unwrap();
        let mut missing: Vec<String> = Vec::new();
        for part in json.split("\"#/components/schemas/").skip(1) {
            let name = part.split('"').next().unwrap_or_default().to_string();
            if !name.is_empty() && !defined.contains(&name) && !missing.contains(&name) {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "the document points at components it does not define: {missing:?}"
        );
    }
}
