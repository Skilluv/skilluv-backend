//! Observability wiring: Sentry / GlitchTip error reporting + tracing bridge.

use crate::config::AppConfig;

/// Initialise Sentry (or any Sentry-API-compatible backend, e.g. self-hosted GlitchTip).
///
/// Returns a [`sentry::ClientInitGuard`] that **must** stay alive for the lifetime of the
/// process — drop it and Sentry stops flushing.
///
/// When `SENTRY_DSN` is unset (typical local dev), returns `None` and Sentry is fully
/// disabled with no overhead.
pub fn init_sentry(config: &AppConfig) -> Option<sentry::ClientInitGuard> {
    let dsn = config.sentry_dsn.clone()?;
    let release = config.release.clone().map(Into::into);
    let environment = config.environment.clone().into();
    let traces_sample_rate = config.sentry_traces_sample_rate;

    Some(sentry::init((
        dsn,
        sentry::ClientOptions {
            release,
            environment: Some(environment),
            traces_sample_rate,
            attach_stacktrace: true,
            // RGPD: don't auto-capture IPs / cookies. We tag user_id manually from JWT in
            // the AuthUser extractor, which is the only PII Sentry sees from us.
            send_default_pii: false,
            ..Default::default()
        },
    )))
}

/// Forward `tracing` events at WARN/ERROR levels to Sentry as breadcrumbs + events.
pub fn sentry_tracing_layer<S>() -> sentry_tracing::SentryLayer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    sentry_tracing::layer().event_filter(|md| match *md.level() {
        tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
        tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb,
        _ => sentry_tracing::EventFilter::Ignore,
    })
}
