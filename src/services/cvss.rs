//! CVSS 3.1, from a vector to a number and a word.
//!
//! ## Why the score is computed and not accepted
//!
//! Ticket W-04 asked for a `cvss_score` column that a reporter fills in. That
//! is a number somebody typed, and in a domain where severity decides a payout
//! it is the one number nobody should be allowed to type. What a reporter
//! supplies is the *vector* — eight metrics, each an explicit claim about the
//! defect — and the score follows from it by a published formula.
//!
//! The difference is not pedantry. A disagreement about a score is
//! unresolvable ("I think it is a 9"); a disagreement about a vector is a
//! conversation about one metric ("you have put PR:N and the endpoint requires
//! an account"). Every severity argument this platform has will be about a
//! metric, which is the only kind that converges.
//!
//! ## What is implemented
//!
//! The base metric group, which is what a report carries. Temporal and
//! environmental metrics are accepted in the vector string and ignored in the
//! arithmetic: they describe a moment and an estimate, they are re-scored by
//! whoever is defending the system, and a base score is what two strangers can
//! agree on.
//!
//! ## The rounding
//!
//! CVSS 3.1 defines its own `Roundup`, and it is not "round to one decimal".
//! `4.02` becomes `4.1`, not `4.0`. Getting that wrong shifts scores across
//! tier boundaries — 6.9 against 7.0 is medium against high, and in a bounty
//! programme that is money — so it is implemented as the specification writes
//! it, on integers, rather than with floating-point rounding.

use serde::Serialize;

/// The eight base metrics, parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base {
    pub attack_vector: AttackVector,
    pub attack_complexity: Complexity,
    pub privileges_required: Privileges,
    pub user_interaction: Interaction,
    pub scope_changed: bool,
    pub confidentiality: Impact,
    pub integrity: Impact,
    pub availability: Impact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackVector {
    Network,
    Adjacent,
    Local,
    Physical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privileges {
    None,
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    None,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Impact {
    High,
    Low,
    None,
}

/// What a caller gets back: the number, the word, and the vector as parsed.
#[derive(Debug, Clone, Serialize)]
pub struct Scored {
    /// The base score, 0.0 to 10.0, one decimal, rounded the way the
    /// specification says.
    pub score: f64,
    /// `critical`, `high`, `medium`, `low`, `informational`. The vocabulary
    /// `security_findings.severity_tier` uses.
    pub tier: &'static str,
    /// The vector, normalised: uppercase, base metrics only, in the canonical
    /// order. Stored rather than the string that arrived, so that two reports
    /// of the same defect compare equal.
    pub vector: String,
}

/// The five tiers, from a score.
///
/// `informational` rather than `none` for zero: the platform's severity
/// vocabulary says informational, and a finding scoring zero is still a
/// finding worth recording — a missing header, a version disclosed.
pub fn tier_for_score(score: f64) -> &'static str {
    if score >= 9.0 {
        "critical"
    } else if score >= 7.0 {
        "high"
    } else if score >= 4.0 {
        "medium"
    } else if score > 0.0 {
        "low"
    } else {
        "informational"
    }
}

/// Parse a CVSS 3.1 vector and score it.
///
/// Returns the reason on failure rather than a default: a vector that does not
/// parse must not silently become a 0.0, which would read as "harmless" on a
/// report that might be critical.
pub fn score_vector(raw: &str) -> Result<Scored, String> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('/');

    let prefix = parts.next().unwrap_or_default();
    if !prefix.eq_ignore_ascii_case("CVSS:3.1") {
        return Err(format!(
            "expected a CVSS:3.1 vector, found '{prefix}' — only version 3.1 \
             is scored here"
        ));
    }

    let mut av = None;
    let mut ac = None;
    let mut pr = None;
    let mut ui = None;
    let mut sc = None;
    let mut c = None;
    let mut i = None;
    let mut a = None;

    for part in parts {
        if part.is_empty() {
            continue;
        }
        let Some((metric, value)) = part.split_once(':') else {
            return Err(format!("'{part}' is not a metric:value pair"));
        };
        let metric = metric.to_ascii_uppercase();
        let value = value.to_ascii_uppercase();

        match (metric.as_str(), value.as_str()) {
            ("AV", "N") => av = Some(AttackVector::Network),
            ("AV", "A") => av = Some(AttackVector::Adjacent),
            ("AV", "L") => av = Some(AttackVector::Local),
            ("AV", "P") => av = Some(AttackVector::Physical),
            ("AC", "L") => ac = Some(Complexity::Low),
            ("AC", "H") => ac = Some(Complexity::High),
            ("PR", "N") => pr = Some(Privileges::None),
            ("PR", "L") => pr = Some(Privileges::Low),
            ("PR", "H") => pr = Some(Privileges::High),
            ("UI", "N") => ui = Some(Interaction::None),
            ("UI", "R") => ui = Some(Interaction::Required),
            ("S", "U") => sc = Some(false),
            ("S", "C") => sc = Some(true),
            ("C", v) => c = Some(impact_from(v, "C")?),
            ("I", v) => i = Some(impact_from(v, "I")?),
            ("A", v) => a = Some(impact_from(v, "A")?),
            // Temporal and environmental metrics. Accepted and not scored —
            // see the module header.
            ("E" | "RL" | "RC", _)
            | ("CR" | "IR" | "AR", _)
            | ("MAV" | "MAC" | "MPR" | "MUI" | "MS" | "MC" | "MI" | "MA", _) => {}
            (m, v) => return Err(format!("'{m}:{v}' is not a CVSS 3.1 metric value")),
        }
    }

    let base = Base {
        attack_vector: av.ok_or("the vector is missing AV")?,
        attack_complexity: ac.ok_or("the vector is missing AC")?,
        privileges_required: pr.ok_or("the vector is missing PR")?,
        user_interaction: ui.ok_or("the vector is missing UI")?,
        scope_changed: sc.ok_or("the vector is missing S")?,
        confidentiality: c.ok_or("the vector is missing C")?,
        integrity: i.ok_or("the vector is missing I")?,
        availability: a.ok_or("the vector is missing A")?,
    };

    let score = base_score(&base);
    Ok(Scored {
        score,
        tier: tier_for_score(score),
        vector: canonical(&base),
    })
}

fn impact_from(value: &str, metric: &str) -> Result<Impact, String> {
    match value {
        "H" => Ok(Impact::High),
        "L" => Ok(Impact::Low),
        "N" => Ok(Impact::None),
        other => Err(format!("'{metric}:{other}' is not H, L or N")),
    }
}

/// The vector as the specification writes it: base metrics, canonical order.
pub fn canonical(base: &Base) -> String {
    let av = match base.attack_vector {
        AttackVector::Network => "N",
        AttackVector::Adjacent => "A",
        AttackVector::Local => "L",
        AttackVector::Physical => "P",
    };
    let ac = match base.attack_complexity {
        Complexity::Low => "L",
        Complexity::High => "H",
    };
    let pr = match base.privileges_required {
        Privileges::None => "N",
        Privileges::Low => "L",
        Privileges::High => "H",
    };
    let ui = match base.user_interaction {
        Interaction::None => "N",
        Interaction::Required => "R",
    };
    let s = if base.scope_changed { "C" } else { "U" };
    format!(
        "CVSS:3.1/AV:{av}/AC:{ac}/PR:{pr}/UI:{ui}/S:{s}/C:{}/I:{}/A:{}",
        impact_letter(base.confidentiality),
        impact_letter(base.integrity),
        impact_letter(base.availability),
    )
}

fn impact_letter(i: Impact) -> &'static str {
    match i {
        Impact::High => "H",
        Impact::Low => "L",
        Impact::None => "N",
    }
}

/// The base score, by the CVSS 3.1 specification.
pub fn base_score(base: &Base) -> f64 {
    let iss = 1.0
        - (1.0 - impact_weight(base.confidentiality))
            * (1.0 - impact_weight(base.integrity))
            * (1.0 - impact_weight(base.availability));

    let impact = if base.scope_changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15)
    } else {
        6.42 * iss
    };

    if impact <= 0.0 {
        return 0.0;
    }

    let exploitability = 8.22
        * av_weight(base.attack_vector)
        * ac_weight(base.attack_complexity)
        * pr_weight(base.privileges_required, base.scope_changed)
        * ui_weight(base.user_interaction);

    let raw = if base.scope_changed {
        1.08 * (impact + exploitability)
    } else {
        impact + exploitability
    };

    roundup(raw.min(10.0))
}

fn av_weight(v: AttackVector) -> f64 {
    match v {
        AttackVector::Network => 0.85,
        AttackVector::Adjacent => 0.62,
        AttackVector::Local => 0.55,
        AttackVector::Physical => 0.2,
    }
}

fn ac_weight(v: Complexity) -> f64 {
    match v {
        Complexity::Low => 0.77,
        Complexity::High => 0.44,
    }
}

/// The one weight that depends on another metric: a change of scope makes
/// holding privileges worth more to an attacker, so the same `PR:L` scores
/// differently. Reading this as a two-dimensional lookup rather than
/// multiplying afterwards is what the specification does.
fn pr_weight(v: Privileges, scope_changed: bool) -> f64 {
    match (v, scope_changed) {
        (Privileges::None, _) => 0.85,
        (Privileges::Low, false) => 0.62,
        (Privileges::Low, true) => 0.68,
        (Privileges::High, false) => 0.27,
        (Privileges::High, true) => 0.50,
    }
}

fn ui_weight(v: Interaction) -> f64 {
    match v {
        Interaction::None => 0.85,
        Interaction::Required => 0.62,
    }
}

fn impact_weight(i: Impact) -> f64 {
    match i {
        Impact::High => 0.56,
        Impact::Low => 0.22,
        Impact::None => 0.0,
    }
}

/// The specification's `Roundup`: the smallest one-decimal number not less
/// than the input.
///
/// Not `(x * 10.0).round() / 10.0`. `4.02` is `4.1` here and `4.0` there, and
/// the difference lands on tier boundaries where it decides money. Done on
/// integers because the appendix of the specification says to, having found
/// that the floating-point version disagrees with itself at 6.9 and 7.0.
pub fn roundup(input: f64) -> f64 {
    let scaled = (input * 100_000.0).round() as i64;
    if scaled % 10_000 == 0 {
        scaled as f64 / 100_000.0
    } else {
        ((scaled / 10_000) + 1) as f64 / 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worst_unchanged_scope_is_nine_point_eight() {
        // The most-quoted vector there is: remote, no privileges, no
        // interaction, total loss of all three impacts.
        let s = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").unwrap();
        assert_eq!(s.score, 9.8);
        assert_eq!(s.tier, "critical");
    }

    #[test]
    fn a_changed_scope_reaches_ten() {
        let s = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H").unwrap();
        assert_eq!(s.score, 10.0);
        assert_eq!(s.tier, "critical");
    }

    #[test]
    fn availability_alone_over_the_network_is_seven_point_five() {
        let s = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H").unwrap();
        assert_eq!(s.score, 7.5);
        assert_eq!(s.tier, "high");
    }

    #[test]
    fn no_impact_scores_zero_whatever_the_exploitability() {
        // Impact of zero short-circuits before exploitability is even
        // computed. A finding with no impact is informational, and the tier
        // says so rather than the score being quietly small.
        let s = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N").unwrap();
        assert_eq!(s.score, 0.0);
        assert_eq!(s.tier, "informational");
    }

    #[test]
    fn a_local_low_privilege_confidentiality_loss() {
        // Worked by hand from the specification:
        //   exploitability = 8.22 * 0.55 * 0.77 * 0.62 * 0.85 = 1.83455...
        //   iss = 0.56, impact = 6.42 * 0.56 = 3.5952
        //   sum = 5.42975... -> roundup -> 5.5
        let s = score_vector("CVSS:3.1/AV:L/AC:L/PR:L/UI:N/S:U/C:H/I:N/A:N").unwrap();
        assert_eq!(s.score, 5.5);
        assert_eq!(s.tier, "medium");
    }

    #[test]
    fn roundup_is_not_rounding() {
        // Ordinary rounding would give 4.0 for the first of these. The
        // specification gives 4.1, and the difference lands on tier boundaries
        // where it decides money.
        assert_eq!(roundup(4.02), 4.1);
        assert_eq!(roundup(4.00), 4.0);
        assert_eq!(roundup(6.9000), 6.9);
        // Anything at all above 6.9 becomes 7.0 — which is medium becoming
        // high. This is the boundary the specification's appendix exists for.
        assert_eq!(roundup(6.91), 7.0);
        assert_eq!(roundup(6.9001), 7.0);
        // And the tolerance the integer form buys: a value that differs from
        // 6.9 only below the fifth decimal is 6.9, not 7.0. Floating-point
        // arithmetic produces those, and rounding them up would move scores
        // across a tier for a rounding error.
        assert_eq!(roundup(6.900001), 6.9);
    }

    #[test]
    fn temporal_metrics_are_accepted_and_ignored() {
        let with =
            score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H/E:U/RL:O/RC:C").unwrap();
        let without = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").unwrap();
        assert_eq!(with.score, without.score);
        // And the stored vector is the base one either way, so two reports of
        // the same defect compare equal.
        assert_eq!(with.vector, without.vector);
    }

    #[test]
    fn the_vector_is_normalised() {
        let s = score_vector("cvss:3.1/av:n/ac:l/pr:n/ui:n/s:u/c:h/i:h/a:h").unwrap();
        assert_eq!(s.vector, "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H");
    }

    #[test]
    fn a_missing_metric_is_refused_rather_than_defaulted() {
        let err = score_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H").unwrap_err();
        assert!(err.contains("missing A"), "{err}");
    }

    #[test]
    fn version_two_and_four_are_refused() {
        assert!(score_vector("CVSS:2.0/AV:N/AC:L/Au:N/C:P/I:P/A:P").is_err());
        assert!(score_vector("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N").is_err());
    }

    #[test]
    fn an_unknown_metric_value_is_refused() {
        let err = score_vector("CVSS:3.1/AV:Q/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").unwrap_err();
        assert!(err.contains("AV:Q"), "{err}");
    }

    #[test]
    fn tiers_land_on_the_published_boundaries() {
        assert_eq!(tier_for_score(0.0), "informational");
        assert_eq!(tier_for_score(0.1), "low");
        assert_eq!(tier_for_score(3.9), "low");
        assert_eq!(tier_for_score(4.0), "medium");
        assert_eq!(tier_for_score(6.9), "medium");
        assert_eq!(tier_for_score(7.0), "high");
        assert_eq!(tier_for_score(8.9), "high");
        assert_eq!(tier_for_score(9.0), "critical");
        assert_eq!(tier_for_score(10.0), "critical");
    }
}
