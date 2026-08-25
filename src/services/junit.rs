//! Reading a test report instead of taking somebody's word for its numbers.
//!
//! `quality_practice::import_test_run` took the figures as input: a member
//! typed how many tests ran and how many failed, and a public link stood as
//! the proof a reviewer could check. That is the platform's usual shape —
//! declared, then verified by a human — and for most things it is the right
//! one.
//!
//! It is the wrong one here, for a reason specific to this domain. The figure
//! a quality attestation rests on is the number of tests, and a reviewer
//! asked to check it has to open a CI artefact and count. Nobody does that
//! more than twice. So the numbers were declared, unchecked in practice, and
//! attested.
//!
//! This module reads them. What it parses is the summary line every JUnit
//! writer produces, which is the format GitHub Actions, GitLab CI, Jenkins,
//! pytest, Jest, PHPUnit, Playwright, Cypress and the rest all emit or can be
//! made to.
//!
//! ## Why the format is parsed by hand
//!
//! The summary lives in the attributes of one or two element types, and
//! everything else in the file — every individual case, every captured stdout
//! line, every stack trace — is ignored. A general XML parser would read all
//! of it into memory to let this take six numbers off the top.
//!
//! The parser is deliberately narrow about what it will accept, and says so
//! when it refuses: a report that does not look like JUnit is refused rather
//! than read as zeroes, because zero tests is a claim and "I could not read
//! this" is not.
//!
//! ## What it does not do
//!
//! Decide anything. A parsed report still waits for a reviewer, and
//! `verified_at` still means a person looked. What changes is what they are
//! checking: whether the run is the one it claims to be, rather than whether
//! somebody's arithmetic was honest.

use crate::errors::AppError;

/// What a run reports about itself.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub tests_total: i32,
    pub tests_failed: i32,
    pub tests_skipped: i32,
    /// Seconds, rounded. Absent when the report does not time itself.
    pub duration_seconds: Option<i32>,
    /// How many `<testsuite>` elements were read, so a caller can say "six
    /// suites" rather than implying one.
    pub suites: i32,
}

/// Read the summary out of a JUnit XML report.
///
/// A `<testsuites>` root carries totals across its children. Many writers
/// emit a bare `<testsuite>` with no wrapper, and some emit a wrapper whose
/// totals are absent or wrong — pytest-xdist has shipped both. So the suites
/// are always summed, and the root's own totals are used only to check the
/// sum rather than to replace it: when they disagree the report is refused,
/// because one of the two numbers is wrong and this module cannot tell which.
pub fn parse(xml: &str) -> Result<Summary, AppError> {
    if !xml.contains("<testsuite") {
        return Err(AppError::Validation(
            "that does not look like a JUnit report — no `<testsuite>` anywhere in it. \
             If your runner writes a different format, export JUnit XML: every runner \
             this platform has met can."
                .into(),
        ));
    }

    let mut summary = Summary::default();
    let mut seconds = 0f64;
    let mut timed = false;

    for element in elements_named(xml, "testsuite") {
        summary.suites += 1;
        summary.tests_total += attr_i32(element, "tests").unwrap_or(0);
        // `errors` and `failures` are different words for the same outcome
        // as far as anybody reading a report is concerned: the test did not
        // pass. Keeping them apart would need a column that is never read.
        summary.tests_failed +=
            attr_i32(element, "failures").unwrap_or(0) + attr_i32(element, "errors").unwrap_or(0);
        summary.tests_skipped += attr_i32(element, "skipped").unwrap_or(0);

        if let Some(t) = attr_f64(element, "time") {
            seconds += t;
            timed = true;
        }
    }

    if summary.suites == 0 {
        return Err(AppError::Validation(
            "a `<testsuite>` was found but nothing could be read from it".into(),
        ));
    }

    // The wrapper's own totals, where it has them, as a check on the sum.
    if let Some(root) = elements_named(xml, "testsuites").next()
        && let Some(declared) = attr_i32(root, "tests")
        && declared != summary.tests_total
    {
        return Err(AppError::Validation(format!(
            "the report disagrees with itself: the root says {declared} tests and its \
             suites add up to {}. One of the two is wrong and this cannot tell which",
            summary.tests_total
        )));
    }

    if summary.tests_failed + summary.tests_skipped > summary.tests_total {
        return Err(AppError::Validation(format!(
            "the report says {} failed and {} skipped out of {} — more outcomes than \
             tests",
            summary.tests_failed, summary.tests_skipped, summary.tests_total
        )));
    }

    summary.duration_seconds =
        timed.then(|| seconds.round().clamp(0.0, f64::from(i32::MAX)) as i32);

    Ok(summary)
}

/// Every opening tag of one element name, as the text of the tag itself.
///
/// Matches `<name ` and `<name>` and never `</name>` or `<namespaced`, which
/// is why the character after the name is examined rather than assumed.
fn elements_named<'a>(xml: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let opener = format!("<{name}");
    // Collected because `match_indices` borrows the pattern, and the pattern
    // is built here rather than being a literal.
    let starts: Vec<usize> = xml.match_indices(opener.as_str()).map(|(i, _)| i).collect();
    starts
        .into_iter()
        .filter_map(move |start| {
            let after = xml.get(start + 1 + name.len()..)?;
            let next = after.chars().next()?;
            // `<testsuites` must not be read as `<testsuite`, and a namespace
            // prefix is a different element.
            if !next.is_whitespace() && next != '>' && next != '/' {
                return None;
            }
            let end = xml[start..].find('>')? + start;
            Some(&xml[start..end])
        })
        // Bounded so a file of a million empty suites cannot hold this open.
        .take(50_000)
}

/// The value of one attribute of one tag.
fn attr<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    // ` name="` — the leading space is what stops `skipped` matching inside
    // another attribute's value, and `tests` matching `tests_total`.
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(start) = element.find(&needle) {
            let from = start + needle.len();
            let rest = &element[from..];
            if let Some(end) = rest.find(quote) {
                return Some(&rest[..end]);
            }
        }
    }
    None
}

fn attr_i32(element: &str, name: &str) -> Option<i32> {
    attr(element, name)?.trim().parse().ok()
}

fn attr_f64(element: &str, name: &str) -> Option<f64> {
    let v: f64 = attr(element, name)?.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_report_is_summed_from_its_suites() {
        let xml = r#"<?xml version="1.0"?>
<testsuites tests="7" failures="1" errors="1" time="4.5">
  <testsuite name="a" tests="4" failures="1" errors="0" skipped="1" time="1.5"/>
  <testsuite name="b" tests="3" failures="0" errors="1" skipped="0" time="3.0"/>
</testsuites>"#;
        let s = parse(xml).unwrap();
        assert_eq!(s.tests_total, 7);
        // One failure and one error: both are "did not pass".
        assert_eq!(s.tests_failed, 2);
        assert_eq!(s.tests_skipped, 1);
        assert_eq!(s.duration_seconds, Some(5));
        assert_eq!(s.suites, 2);
    }

    #[test]
    fn a_bare_suite_with_no_wrapper_is_read() {
        let xml = r#"<testsuite name="pytest" tests="12" failures="2" errors="0" skipped="3" time="0.4"></testsuite>"#;
        let s = parse(xml).unwrap();
        assert_eq!(s.tests_total, 12);
        assert_eq!(s.tests_failed, 2);
        assert_eq!(s.tests_skipped, 3);
        // Rounded, and 0.4 seconds is nought seconds.
        assert_eq!(s.duration_seconds, Some(0));
        assert_eq!(s.suites, 1);
    }

    #[test]
    fn the_wrapper_is_not_counted_as_a_suite() {
        // `<testsuites` starts with `<testsuite`, and reading it as one would
        // double every figure in the commonest report shape there is.
        let xml = r#"<testsuites tests="2"><testsuite tests="2" failures="0"/></testsuites>"#;
        let s = parse(xml).unwrap();
        assert_eq!(s.suites, 1);
        assert_eq!(s.tests_total, 2);
    }

    #[test]
    fn a_report_that_contradicts_itself_is_refused() {
        let xml = r#"<testsuites tests="99"><testsuite tests="2" failures="0"/></testsuites>"#;
        let err = parse(xml).unwrap_err();
        assert!(format!("{err:?}").contains("disagrees"), "{err:?}");
    }

    #[test]
    fn more_outcomes_than_tests_is_refused() {
        let xml = r#"<testsuite tests="2" failures="3" skipped="1"/>"#;
        assert!(parse(xml).is_err());
    }

    #[test]
    fn something_that_is_not_a_report_is_refused_rather_than_read_as_zero() {
        // The distinction that matters: nought tests is a claim, and "this is
        // not a test report" is not the same claim.
        for junk in ["<html><body>404</body></html>", "", "{\"tests\": 4}"] {
            assert!(parse(junk).is_err(), "{junk} should be refused");
        }
    }

    #[test]
    fn single_quoted_attributes_are_read() {
        let xml = "<testsuite tests='5' failures='0' skipped='1'/>";
        let s = parse(xml).unwrap();
        assert_eq!(s.tests_total, 5);
        assert_eq!(s.tests_skipped, 1);
    }

    #[test]
    fn an_untimed_report_reports_no_duration() {
        let xml = r#"<testsuite tests="1" failures="0"/>"#;
        assert_eq!(parse(xml).unwrap().duration_seconds, None);
    }

    #[test]
    fn a_name_containing_the_word_tests_is_not_an_attribute() {
        // ` tests="` needs its leading space, or `subtests="9"` would answer
        // for `tests`.
        let xml = r#"<testsuite name="x" subtests="9" tests="3" failures="0"/>"#;
        assert_eq!(parse(xml).unwrap().tests_total, 3);
    }

    #[test]
    fn a_namespaced_element_is_not_mistaken_for_a_suite() {
        let xml = r#"<testsuite tests="1" failures="0"/><testsuitex tests="99"/>"#;
        let s = parse(xml).unwrap();
        assert_eq!(s.suites, 1);
        assert_eq!(s.tests_total, 1);
    }
}
