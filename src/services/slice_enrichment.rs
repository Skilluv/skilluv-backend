//! P26 v2 SKI-101 — enrich a slice ingested from a GitHub issue with data
//! derived from the issue's labels and body, replacing the old hardcoded
//! defaults (`primary_domain='code'`, `difficulty=3`, no acceptance
//! criteria).
//!
//! Everything here is **pure** (no I/O) so the caller in
//! `slice_ingestion.rs` stays a thin adaptor and every rule is unit-testable.
//!
//! ─── Conventions ───────────────────────────────────────────────────
//!
//! Labels the parser recognises (case-insensitive):
//!
//!   Domain:      `domain:code` / `domain:design` / `domain:game` /
//!                `domain:security` / `domain:ops` / `domain:ai` /
//!                `domain:soft_skills`
//!   Difficulty:  `difficulty:1` … `difficulty:5`
//!                Aliases (community-standard, so external repos work
//!                without setup):
//!                  `good first issue` / `beginner` / `easy`  → 1
//!                  `intermediate`                             → 3
//!                  `hard` / `advanced`                        → 5
//!
//! Body section for acceptance criteria (any of, case-insensitive):
//!
//!   ## Acceptance criteria
//!   ### Acceptance criteria
//!   ## Acceptance Criteria
//!   ## Definition of Done
//!   ## DoD
//!
//! Content is taken from the heading line to the next `##`/`###` header
//! or end of body, trimmed, then truncated to 4000 chars.
//!
//! ─── Fallback policy ───────────────────────────────────────────────
//!
//! When a signal is missing, we fall back to values that don't lie:
//! - `primary_domain` → the project's fallback (see `Enricher::default_domain`)
//! - `difficulty`     → 3 (mid) — same as pre-SKI-101 behaviour
//! - `acceptance_criteria` → NULL (nothing to parse ≠ empty criteria)

use crate::services::validator_applications::VALID_DOMAINS;

pub const ACCEPTANCE_HEADINGS: &[&str] = &[
    "## acceptance criteria",
    "### acceptance criteria",
    "## definition of done",
    "### definition of done",
    "## dod",
    "### dod",
];

pub const MAX_ACCEPTANCE_LEN: usize = 4000;

/// Output of `enrich_from_issue`. Only carries what we might overwrite —
/// caller keeps the raw `title` / `description` / `external_ref` as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichedFields {
    pub primary_domain: String,
    pub difficulty: i16,
    pub acceptance_criteria: Option<String>,
}

/// Infer domain + difficulty from labels; parse acceptance criteria from
/// the body. `default_domain` is used when no `domain:*` label is set —
/// callers typically pass the project's primary domain (e.g. a
/// design-heavy repo passes `"design"`).
pub fn enrich_from_issue(
    labels: &[String],
    body: Option<&str>,
    default_domain: &str,
) -> EnrichedFields {
    let lower_labels: Vec<String> = labels.iter().map(|l| l.to_lowercase()).collect();
    EnrichedFields {
        primary_domain: infer_domain(&lower_labels, default_domain),
        difficulty: infer_difficulty(&lower_labels),
        acceptance_criteria: body.and_then(parse_acceptance_criteria),
    }
}

fn infer_domain(lower_labels: &[String], default_domain: &str) -> String {
    for l in lower_labels {
        if let Some(rest) = l.strip_prefix("domain:")
            && VALID_DOMAINS.contains(&rest)
        {
            return rest.to_string();
        }
    }
    // Fallback must itself be a valid domain — guard against a caller
    // passing junk (would otherwise poison downstream validators).
    if VALID_DOMAINS.contains(&default_domain) {
        default_domain.to_string()
    } else {
        "code".to_string()
    }
}

fn infer_difficulty(lower_labels: &[String]) -> i16 {
    // Explicit `difficulty:N` wins over aliases.
    for l in lower_labels {
        if let Some(n) = l.strip_prefix("difficulty:")
            && let Ok(v) = n.parse::<i16>()
            && (1..=5).contains(&v)
        {
            return v;
        }
    }
    // Community aliases.
    for l in lower_labels {
        match l.as_str() {
            "good first issue" | "beginner" | "easy" => return 1,
            "intermediate" => return 3,
            "hard" | "advanced" => return 5,
            _ => {}
        }
    }
    3 // pre-SKI-101 fallback — unchanged so ingest of unlabelled issues stays stable.
}

/// Extract the acceptance-criteria section from the body. Returns `None`
/// when no recognised heading is present.
fn parse_acceptance_criteria(body: &str) -> Option<String> {
    // Split on newlines but keep enough context to identify headings
    // even when they carry trailing spaces / colons.
    let lines: Vec<&str> = body.lines().collect();
    let start = lines.iter().position(|line| {
        let trimmed = line.trim().trim_end_matches(':').to_lowercase();
        ACCEPTANCE_HEADINGS.iter().any(|h| trimmed == *h)
    })?;

    // Find the next heading of `##`/`###` OR end of body.
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| {
            let t = line.trim_start();
            t.starts_with("## ") || t.starts_with("### ")
        })
        .map(|(idx, _)| idx)
        .unwrap_or(lines.len());

    let section = lines[start + 1..end].join("\n").trim().to_string();
    if section.is_empty() {
        return None;
    }
    if section.chars().count() > MAX_ACCEPTANCE_LEN {
        let cut = section
            .char_indices()
            .nth(MAX_ACCEPTANCE_LEN - 1)
            .map(|(i, _)| i)
            .unwrap_or(section.len());
        return Some(format!("{}…", &section[..cut]));
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn domain_from_explicit_label() {
        let out = enrich_from_issue(&strs(&["domain:design", "help wanted"]), None, "code");
        assert_eq!(out.primary_domain, "design");
    }

    #[test]
    fn domain_falls_back_to_project_default() {
        let out = enrich_from_issue(&strs(&["bug", "regression"]), None, "design");
        assert_eq!(out.primary_domain, "design");
    }

    #[test]
    fn domain_case_insensitive() {
        let out = enrich_from_issue(&strs(&["Domain:Security"]), None, "code");
        assert_eq!(out.primary_domain, "security");
    }

    #[test]
    fn domain_rejects_unknown_and_falls_back() {
        let out = enrich_from_issue(&strs(&["domain:blockchain"]), None, "code");
        assert_eq!(out.primary_domain, "code");
    }

    #[test]
    fn difficulty_explicit_wins_over_alias() {
        // `good first issue` alias maps to 1, but explicit `difficulty:4` should override.
        let out = enrich_from_issue(&strs(&["difficulty:4", "good first issue"]), None, "code");
        assert_eq!(out.difficulty, 4);
    }

    #[test]
    fn difficulty_aliases() {
        for (label, expected) in [
            ("good first issue", 1),
            ("beginner", 1),
            ("intermediate", 3),
            ("hard", 5),
            ("advanced", 5),
        ] {
            let out = enrich_from_issue(&strs(&[label]), None, "code");
            assert_eq!(out.difficulty, expected, "label={label}");
        }
    }

    #[test]
    fn difficulty_default_when_no_signal() {
        let out = enrich_from_issue(&strs(&["bug"]), None, "code");
        assert_eq!(out.difficulty, 3);
    }

    #[test]
    fn difficulty_rejects_out_of_range() {
        let out = enrich_from_issue(&strs(&["difficulty:9"]), None, "code");
        assert_eq!(out.difficulty, 3);
    }

    #[test]
    fn acceptance_criteria_h2() {
        let body = "\
Context intro.

## Acceptance criteria
- [ ] first
- [ ] second

## Notes
irrelevant";
        let out = enrich_from_issue(&[], Some(body), "code");
        assert_eq!(
            out.acceptance_criteria.as_deref(),
            Some("- [ ] first\n- [ ] second")
        );
    }

    #[test]
    fn acceptance_criteria_h3_and_synonym() {
        let body = "### Definition of Done\n- ship it\n- add test\n";
        let out = enrich_from_issue(&[], Some(body), "code");
        assert_eq!(
            out.acceptance_criteria.as_deref(),
            Some("- ship it\n- add test")
        );
    }

    #[test]
    fn acceptance_criteria_none_when_no_heading() {
        let body = "Just a body without headings.";
        let out = enrich_from_issue(&[], Some(body), "code");
        assert_eq!(out.acceptance_criteria, None);
    }

    #[test]
    fn acceptance_criteria_empty_section_returns_none() {
        let body = "## Acceptance criteria\n\n## Notes\nx";
        let out = enrich_from_issue(&[], Some(body), "code");
        assert_eq!(out.acceptance_criteria, None);
    }

    #[test]
    fn acceptance_criteria_stops_at_next_heading() {
        // Guard against greedy capture that would swallow later sections.
        let body = "## Acceptance criteria\ndo A\n### Notes\ndo NOT capture this";
        let out = enrich_from_issue(&[], Some(body), "code");
        assert_eq!(out.acceptance_criteria.as_deref(), Some("do A"));
    }
}
