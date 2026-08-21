//! Reading a status page that is already public.
//!
//! ## The line this module is on the right side of
//!
//! An objective closes with a figure its own author typed. Checking it means
//! looking at where the figure came from, and the tempting way to automate
//! that is an API key to the client's monitoring — Datadog, Instana, a
//! private Grafana.
//!
//! Skilluv does not do that, and `docs/ops/LEGAL.md` says why: such a key
//! carries the service map, the incident history and the traffic volumes of
//! somebody's estate. That is the list the reinforced NDA exists to protect,
//! and holding it for many clients at once would make this platform worth
//! attacking for what it knows about other people rather than for what it
//! knows about itself.
//!
//! So this module reads **only pages anybody can already open**. No key, no
//! credential, nothing stored that was not already published by the operator
//! to the whole internet.
//!
//! ## What it does not claim
//!
//! It does not replace the declared figure, and it does not compute an
//! authoritative availability. A status page records the incidents its
//! operator chose to publish; an outage nobody posted is invisible here just
//! as it is everywhere else.
//!
//! What it gives a reviewer is the other half of the conversation: the public
//! record, next to the claim, with the dates. Somebody who announces 99.99%
//! over a window in which their own status page shows eleven hours of major
//! outage has not lied to a machine — they have written something a reader
//! can now see does not add up.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

/// A public status page, identified from a URL.
///
/// Only the Atlassian Statuspage shape is recognised, because it is the only
/// one with a documented, keyless, machine-readable endpoint that a large
/// share of operators actually run. Others are left unrecognised rather than
/// guessed at: a scraper against an HTML page is a thing that breaks quietly
/// and then reports "no incidents".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPageRef {
    /// The page's own origin, e.g. `https://www.githubstatus.com`.
    pub origin: String,
}

/// Recognise a public status page from the URL somebody gave as evidence.
///
/// Deliberately narrow. A URL that is not obviously a status page returns
/// `None`, and the objective keeps working exactly as before — declared,
/// sourced, read by a human.
pub fn identify(url: &str) -> Option<StatusPageRef> {
    let url = url.trim();
    let rest = url.strip_prefix("https://")?;
    let (host, _path) = match rest.split_once('/') {
        Some((h, p)) => (h, p),
        None => (rest, ""),
    };

    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.contains(' ') {
        return None;
    }

    // Two shapes cover almost every page in the wild: the vendor's own
    // subdomain, and a custom domain whose name says what it is.
    let hosted = host.ends_with(".statuspage.io");
    let named = host.starts_with("status.")
        || host.ends_with("status.com")
        || host.ends_with("status.io")
        || host.ends_with("statuspage.com");

    (hosted || named).then(|| StatusPageRef {
        origin: format!("https://{host}"),
    })
}

/// One published incident, as the page reported it.
#[derive(Debug, Clone, Serialize)]
pub struct PublicIncident {
    pub name: String,
    /// `minor`, `major`, `critical` — the page's own word, not ours.
    pub impact: String,
    pub started_at: DateTime<Utc>,
    /// Absent while an incident is still open.
    pub resolved_at: Option<DateTime<Utc>>,
    pub minutes: Option<i64>,
}

/// What the public record says about a window.
#[derive(Debug, Clone, Serialize, Default)]
pub struct PublicObservation {
    pub incidents: Vec<PublicIncident>,
    /// Total published downtime in the window, in minutes. Only incidents the
    /// page marked `major` or `critical` are counted: a `minor` degradation
    /// is not an outage and counting it would understate availability against
    /// a target that never promised perfection.
    pub major_downtime_minutes: i64,
    /// Availability implied by the published incidents alone.
    ///
    /// Named "implied" rather than "measured", and it is the whole caveat: it
    /// is what the operator's own published record works out to, not what a
    /// probe saw.
    pub implied_availability_percent: Option<f64>,
}

#[derive(Deserialize)]
struct IncidentsResponse {
    incidents: Vec<RawIncident>,
}

#[derive(Deserialize)]
struct RawIncident {
    name: String,
    impact: Option<String>,
    created_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
}

/// Work out what a window looked like from a list of published incidents.
///
/// Pure, and tested, because this is where a mistake is silent: an
/// availability figure that is wrong still looks like an availability figure.
pub fn observe(
    incidents: Vec<PublicIncident>,
    window_days: i64,
    window_end: DateTime<Utc>,
) -> PublicObservation {
    let window_start = window_end - Duration::days(window_days);

    let in_window: Vec<PublicIncident> = incidents
        .into_iter()
        .filter(|i| i.started_at >= window_start && i.started_at <= window_end)
        .collect();

    // An incident still open at the moment we look is counted up to now, not
    // ignored. Ignoring it would let an ongoing outage improve the figure.
    let major_downtime_minutes: i64 = in_window
        .iter()
        .filter(|i| matches!(i.impact.as_str(), "major" | "critical"))
        .map(|i| {
            i.minutes
                .unwrap_or_else(|| (window_end - i.started_at).num_minutes().max(0))
        })
        .sum();

    let window_minutes = window_days * 24 * 60;
    let implied_availability_percent = (window_minutes > 0).then(|| {
        let up = (window_minutes - major_downtime_minutes).max(0) as f64;
        (up / window_minutes as f64) * 100.0
    });

    PublicObservation {
        incidents: in_window,
        major_downtime_minutes,
        implied_availability_percent,
    }
}

/// Ask a public status page what it published, over one window.
///
/// Failure is not an error the caller has to handle loudly: a page that is
/// down, moved or not a Statuspage after all leaves the objective exactly as
/// it was — declared, sourced, read by a human. That is the fallback, and it
/// is the normal path rather than a degraded one.
pub async fn fetch(
    client: &reqwest::Client,
    page: &StatusPageRef,
    window_days: i64,
) -> Result<PublicObservation, AppError> {
    let body: IncidentsResponse = client
        .get(format!("{}/api/v2/incidents.json", page.origin))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("status page unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("status page refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("not a Statuspage feed: {e}")))?;

    let incidents = body
        .incidents
        .into_iter()
        .filter_map(|raw| {
            let started_at = raw.started_at.or(raw.created_at)?;
            let minutes = raw
                .resolved_at
                .map(|end| (end - started_at).num_minutes().max(0));
            Some(PublicIncident {
                name: raw.name,
                impact: raw.impact.unwrap_or_else(|| "none".into()),
                started_at,
                resolved_at: raw.resolved_at,
                minutes,
            })
        })
        .collect();

    Ok(observe(incidents, window_days, Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn incident(impact: &str, start: &str, minutes: Option<i64>) -> PublicIncident {
        PublicIncident {
            name: "Une panne".into(),
            impact: impact.into(),
            started_at: at(start),
            resolved_at: None,
            minutes,
        }
    }

    #[test]
    fn a_hosted_page_and_a_named_one_are_both_recognised() {
        assert_eq!(
            identify("https://kctbh9vrtdwd.statuspage.io"),
            Some(StatusPageRef {
                origin: "https://kctbh9vrtdwd.statuspage.io".into()
            })
        );
        assert_eq!(
            identify("https://www.githubstatus.com/"),
            Some(StatusPageRef {
                origin: "https://www.githubstatus.com".into()
            })
        );
        assert_eq!(
            identify("https://status.example.org/incidents/42"),
            Some(StatusPageRef {
                origin: "https://status.example.org".into()
            })
        );
    }

    #[test]
    fn anything_that_is_not_obviously_a_status_page_is_left_alone() {
        // The fallback is a human reading the link, which is not a
        // degradation — it is what happens today for everything.
        assert_eq!(identify("https://app.datadoghq.com/dashboard/abc"), None);
        assert_eq!(identify("https://grafana.example.com/d/xyz"), None);
        assert_eq!(identify("https://example.com/uptime.png"), None);
        // Never over plain HTTP: the whole point is a public record nobody
        // rewrote in transit.
        assert_eq!(identify("http://status.example.org"), None);
        assert_eq!(identify(""), None);
    }

    #[test]
    fn only_major_and_critical_count_as_downtime() {
        // A degraded search box is not an outage, and counting it would judge
        // a target that never promised perfection.
        let observation = observe(
            vec![
                incident("major", "2026-06-10T00:00:00Z", Some(120)),
                incident("minor", "2026-06-11T00:00:00Z", Some(600)),
                incident("critical", "2026-06-12T00:00:00Z", Some(60)),
            ],
            30,
            at("2026-06-30T00:00:00Z"),
        );
        assert_eq!(observation.major_downtime_minutes, 180);
        assert_eq!(observation.incidents.len(), 3, "all three are still shown");
    }

    #[test]
    fn an_incident_outside_the_window_does_not_count() {
        let observation = observe(
            vec![incident("critical", "2026-01-01T00:00:00Z", Some(5000))],
            30,
            at("2026-06-30T00:00:00Z"),
        );
        assert_eq!(observation.major_downtime_minutes, 0);
        assert!(observation.incidents.is_empty());
    }

    #[test]
    fn an_open_incident_counts_up_to_now() {
        // Otherwise an outage still in progress would improve the figure,
        // which is the one direction the arithmetic must never go.
        let observation = observe(
            vec![incident("critical", "2026-06-29T00:00:00Z", None)],
            30,
            at("2026-06-30T00:00:00Z"),
        );
        assert_eq!(observation.major_downtime_minutes, 24 * 60);
    }

    #[test]
    fn the_implied_figure_is_what_the_published_record_works_out_to() {
        // 43 200 minutes in thirty days; 43,2 of them down is exactly 99.9%.
        let observation = observe(
            vec![incident("major", "2026-06-10T00:00:00Z", Some(43))],
            30,
            at("2026-06-30T00:00:00Z"),
        );
        let implied = observation.implied_availability_percent.unwrap();
        assert!(
            (implied - 99.9).abs() < 0.01,
            "expected about 99.9, got {implied}"
        );
    }

    #[test]
    fn a_page_with_nothing_published_implies_a_full_window() {
        // And that is worth being explicit about: silence on a status page is
        // not proof of uptime, it is proof of silence. The figure is
        // "implied", and the reviewer still reads the claim.
        let observation = observe(vec![], 90, at("2026-06-30T00:00:00Z"));
        assert_eq!(observation.major_downtime_minutes, 0);
        assert_eq!(observation.implied_availability_percent, Some(100.0));
    }
}
