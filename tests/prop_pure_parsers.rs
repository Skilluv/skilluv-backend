//! FZ/MU — property and exhaustive tests for the pure parsers and decision
//! tables, in the form that runs on the pinned stable toolchain without
//! libFuzzer.
//!
//! Rust removes memory bugs, not panics, overflow, hidden `unwrap`s, or
//! violated business invariants. These functions take attacker-influenced
//! input (a CVSS vector a reporter types, a webhook signature, a state name
//! from a request) and must never panic and never return a value outside their
//! contract. A panic in any generated case fails the test; every `Ok`/`true`
//! path asserts an invariant the rest of the system relies on.
//!
//! The cargo-fuzz targets in `fuzz/` wrap these same functions for the nightly
//! CI amplifier (libFuzzer + nightly, not runnable on Windows). This file is
//! the locally verifiable core.

use proptest::prelude::*;

use uuid::Uuid;

use skilluv_backend::routes::talent_search_v4;
use skilluv_backend::services::cvss::{
    self, AttackVector, Base, Complexity, Impact, Interaction, Privileges,
};
use skilluv_backend::services::linear_sync::verify_signature;
use skilluv_backend::services::public_feed;
use skilluv_backend::services::security_findings::{
    Actor, allowed_transition, fragments_for, parse_scope,
};

// ═══════════════════════════════════════════════════════════════════
// CVSS — the number that decides a payout
// ═══════════════════════════════════════════════════════════════════

const AVS: [AttackVector; 4] = [
    AttackVector::Network,
    AttackVector::Adjacent,
    AttackVector::Local,
    AttackVector::Physical,
];
const ACS: [Complexity; 2] = [Complexity::Low, Complexity::High];
const PRS: [Privileges; 3] = [Privileges::None, Privileges::Low, Privileges::High];
const UIS: [Interaction; 2] = [Interaction::None, Interaction::Required];
const IMPACTS: [Impact; 3] = [Impact::High, Impact::Low, Impact::None];

/// Every one of the 2592 base-metric combinations, exhaustively: the score
/// stays in range, is already rounded to one decimal, the tier agrees with the
/// score, and the canonical vector the printer produces parses back to the same
/// score and the same string. Parser and printer must agree or two reports of
/// the same defect stop comparing equal.
#[test]
fn every_cvss_base_scores_in_range_and_round_trips() {
    let mut n = 0u32;
    for &av in &AVS {
        for &ac in &ACS {
            for &pr in &PRS {
                for &ui in &UIS {
                    for &scope_changed in &[false, true] {
                        for &c in &IMPACTS {
                            for &i in &IMPACTS {
                                for &a in &IMPACTS {
                                    let base = Base {
                                        attack_vector: av,
                                        attack_complexity: ac,
                                        privileges_required: pr,
                                        user_interaction: ui,
                                        scope_changed,
                                        confidentiality: c,
                                        integrity: i,
                                        availability: a,
                                    };
                                    let score = cvss::base_score(&base);
                                    assert!(
                                        (0.0..=10.0).contains(&score),
                                        "score {score} out of range for {base:?}"
                                    );
                                    // base_score returns roundup(...): re-rounding is a no-op.
                                    assert_eq!(
                                        cvss::roundup(score),
                                        score,
                                        "score {score} is not one-decimal for {base:?}"
                                    );

                                    let vector = cvss::canonical(&base);
                                    let parsed = cvss::score_vector(&vector).unwrap_or_else(|e| {
                                        panic!("canonical vector {vector} did not parse: {e}")
                                    });
                                    assert_eq!(
                                        parsed.score, score,
                                        "round-trip score for {vector}"
                                    );
                                    assert_eq!(parsed.vector, vector, "round-trip vector");
                                    assert_eq!(
                                        parsed.tier,
                                        cvss::tier_for_score(score),
                                        "tier disagrees with score for {vector}"
                                    );
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(n, 2592, "expected every base combination");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// `roundup` over its real domain [0, 10]: stays in range, lands on a
    /// one-decimal grid, never rounds *down* past the specification's
    /// five-decimal tolerance, moves by less than a tenth, and is idempotent.
    #[test]
    fn roundup_is_a_bounded_one_decimal_ceiling(x in 0.0f64..=10.0) {
        let r = cvss::roundup(x);
        prop_assert!((0.0..=10.0).contains(&r), "roundup({x}) = {r} out of range");

        let tenths = (r * 10.0).round();
        prop_assert!(((r * 10.0) - tenths).abs() < 1e-9, "roundup({x}) = {r} not one decimal");

        // Ceiling, with the spec's 5-decimal tolerance (6.900001 -> 6.9).
        prop_assert!(r + 1e-4 >= x, "roundup({x}) = {r} rounded down");
        prop_assert!(r - x < 0.1 + 1e-4, "roundup({x}) = {r} moved more than a tenth");

        prop_assert_eq!(cvss::roundup(r), r, "roundup not idempotent at {}", r);
    }

    /// `score_vector` on arbitrary bytes: never panics, and any success is
    /// bounded, tier-consistent, one-decimal, and stable under re-scoring the
    /// canonical vector it returned.
    #[test]
    fn score_vector_never_panics_and_ok_is_bounded(s in ".*") {
        if let Ok(scored) = cvss::score_vector(&s) {
            prop_assert!((0.0..=10.0).contains(&scored.score));
            prop_assert_eq!(scored.tier, cvss::tier_for_score(scored.score));
            prop_assert_eq!(cvss::roundup(scored.score), scored.score);
            let again = cvss::score_vector(&scored.vector)
                .expect("a canonical vector must re-parse");
            prop_assert_eq!(again.score, scored.score);
            prop_assert_eq!(again.vector, scored.vector);
        }
    }

    /// The same, but with input shaped like a real vector — a `CVSS:3.1`
    /// prefix followed by plausible `X:Y` tokens — so the parser's metric
    /// arms, missing-metric errors and unknown-value errors are all exercised
    /// instead of being rejected at the prefix.
    #[test]
    fn structured_cvss_never_panics(
        tokens in prop::collection::vec("[A-Za-z]{0,4}:[A-Za-z]{0,3}", 0..12)
    ) {
        let vector = format!("CVSS:3.1/{}", tokens.join("/"));
        if let Ok(scored) = cvss::score_vector(&vector) {
            prop_assert!((0.0..=10.0).contains(&scored.score));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// fragments_for — the award, by tier
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Never panics on any tier string, and every award is strictly positive:
    /// a confirmed finding is always worth something, and an unknown tier falls
    /// back to the informational floor rather than to zero or a negative.
    #[test]
    fn fragments_for_is_always_positive(tier in ".*") {
        prop_assert!(fragments_for(&tier) > 0);
    }
}

#[test]
fn fragments_for_scale_is_monotonic_by_tier() {
    // The ratios are the anti-volume argument (F-05): a critical is worth two
    // hundred informationals. Ordering must hold or a volume strategy appears.
    let (crit, high, med, low, info) = (
        fragments_for("critical"),
        fragments_for("high"),
        fragments_for("medium"),
        fragments_for("low"),
        fragments_for("anything-unknown"),
    );
    assert!(crit > high && high > med && med > low && low > info);
    assert!(info > 0);
}

// ═══════════════════════════════════════════════════════════════════
// allowed_transition — who may move a finding, and to where
// ═══════════════════════════════════════════════════════════════════

const STATES: [&str; 8] = [
    "submitted",
    "triaged",
    "withdrawn",
    "not_applicable",
    "confirmed",
    "duplicate",
    "fixed",
    "published",
];
const TERMINAL: [&str; 4] = ["withdrawn", "not_applicable", "duplicate", "published"];
const ACTORS: [Actor; 4] = [
    Actor::Reporter,
    Actor::Triager,
    Actor::Reviewer,
    Actor::Admin,
];

/// The security-relevant invariants of the transition table, exhaustively over
/// every (actor, from, to). These are privilege boundaries: breaking one is an
/// escalation, not a cosmetic bug.
#[test]
fn transition_table_holds_its_privilege_boundaries() {
    for &actor in &ACTORS {
        for &from in &STATES {
            // No state transitions to itself.
            assert!(
                !allowed_transition(actor, from, from),
                "{actor:?} allowed a self-transition on {from}"
            );
            for &to in &STATES {
                let allowed = allowed_transition(actor, from, to);

                // Terminal states are sinks: nothing leaves them.
                if TERMINAL.contains(&from) {
                    assert!(!allowed, "{actor:?} left terminal state {from} -> {to}");
                }

                if allowed {
                    // Only an administrator publishes — the irreversible door.
                    if to == "published" {
                        assert_eq!(actor, Actor::Admin, "non-admin published {from} -> {to}");
                    }
                    // A reporter's only reachable state is withdrawal.
                    if actor == Actor::Reporter {
                        assert_eq!(to, "withdrawn", "reporter reached {to}, not withdrawn");
                    }
                    // A triager may neither confirm nor publish (confirming
                    // asserts publicly that a vulnerability is real).
                    if actor == Actor::Triager {
                        assert_ne!(to, "confirmed", "triager confirmed a finding");
                        assert_ne!(to, "published", "triager published a finding");
                    }
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Arbitrary state names never panic, and a transition out of an unknown
    /// state is never allowed — the table is closed, not open by default.
    #[test]
    fn unknown_states_never_transition(from in ".*", to in ".*") {
        let from_known = STATES.contains(&from.as_str());
        for &actor in &ACTORS {
            let allowed = allowed_transition(actor, &from, &to);
            if !from_known {
                prop_assert!(!allowed, "transition allowed out of unknown state {from:?}");
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// verify_signature — the webhook HMAC gate
// ═══════════════════════════════════════════════════════════════════

// RFC 4231 test case 2 for HMAC-SHA256 — a published vector, so this asserts
// the function agrees with the standard, not merely with itself.
const RFC4231_KEY: &str = "Jefe";
const RFC4231_DATA: &[u8] = b"what do ya want for nothing?";
const RFC4231_MAC: &str = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";

#[test]
fn verify_signature_accepts_the_rfc_vector_and_rejects_tampering() {
    // Correct signature, bare hex.
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, RFC4231_MAC).is_ok());
    // Correct signature, `sha256=` prefixed (GitHub's form).
    let prefixed = format!("sha256={RFC4231_MAC}");
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, &prefixed).is_ok());

    // One flipped hex nibble is rejected.
    let mut tampered: Vec<char> = RFC4231_MAC.chars().collect();
    tampered[0] = if tampered[0] == '5' { '6' } else { '5' };
    let tampered: String = tampered.into_iter().collect();
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, &tampered).is_err());

    // A body change is rejected under the same signature.
    assert!(verify_signature(RFC4231_KEY, b"what do ya want for something?", RFC4231_MAC).is_err());

    // Non-hex, empty, and wrong-length signatures are refused, not panicked.
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, "not-hex").is_err());
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, "").is_err());
    assert!(verify_signature(RFC4231_KEY, RFC4231_DATA, "ab").is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Arbitrary secret, body and signature: never panics, whatever the bytes.
    #[test]
    fn verify_signature_never_panics(
        secret in ".*",
        body in prop::collection::vec(any::<u8>(), 0..256),
        signature in ".*"
    ) {
        let _ = verify_signature(&secret, &body, &signature);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Keyset cursors — decode never panics, and a cursor survives a round trip
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// The public feed cursor (base64url of `<rfc3339>|<uuid>`) survives a
    /// round trip for any instant and id, and the encoding is URL-safe — the
    /// bug the encoding exists to prevent is a `+` in a query string, so this
    /// asserts no character needs escaping.
    #[test]
    fn public_feed_cursor_round_trips(ms in 0i64..=32_503_680_000_000i64, raw_id in any::<u128>()) {
        let occurred_at = chrono::DateTime::from_timestamp_millis(ms).unwrap();
        let id = Uuid::from_u128(raw_id);
        let cursor = public_feed::Cursor { occurred_at, id };

        let encoded = cursor.encode();
        prop_assert!(
            encoded.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "cursor is not URL-safe: {encoded}"
        );

        let decoded = public_feed::Cursor::decode(&encoded).expect("a cursor it just made must decode");
        prop_assert_eq!(decoded.occurred_at, occurred_at);
        prop_assert_eq!(decoded.id, id);
    }

    /// Arbitrary bytes never panic the public-feed decoder — answering "from
    /// the beginning" to a broken cursor would silently re-read the whole feed,
    /// so it must return None, not a default.
    #[test]
    fn public_feed_cursor_decode_never_panics(raw in ".*") {
        let _ = public_feed::Cursor::decode(&raw);
    }

    /// The talent-search cursor (`<i64>|<uuid>`) survives a round trip for any
    /// key and id, negative keys included.
    #[test]
    fn talent_cursor_round_trips(key in any::<i64>(), raw_id in any::<u128>()) {
        let user_id = Uuid::from_u128(raw_id);
        let cursor = talent_search_v4::Cursor { key, user_id };
        let decoded = talent_search_v4::Cursor::decode(&cursor.encode())
            .expect("a cursor it just made must decode");
        prop_assert_eq!(decoded.key, key);
        prop_assert_eq!(decoded.user_id, user_id);
    }

    /// Arbitrary bytes never panic the talent-search decoder.
    #[test]
    fn talent_cursor_decode_never_panics(raw in ".*") {
        let _ = talent_search_v4::Cursor::decode(&raw);
    }
}

// ═══════════════════════════════════════════════════════════════════
// parse_scope — the published bug-bounty scope is never empty
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Whatever the operator puts in SKILLUV_SECURITY_SCOPE_HOSTS, the parsed
    /// scope is never empty and every host is trimmed, lowercased and
    /// non-blank. This matters because scope_hosts() gates finding submission
    /// (security_findings.rs: "'{host}' is not in the published scope"): an
    /// empty scope would reject *every* submission — a silent availability
    /// lockout from a malformed env var. So a garbage override falls back to
    /// the defaults rather than to nothing.
    #[test]
    fn parse_scope_is_never_empty_and_is_normalised(raw in ".*") {
        let hosts = parse_scope(Some(&raw));
        prop_assert!(!hosts.is_empty(), "empty scope from override {raw:?}");
        for h in &hosts {
            prop_assert!(!h.is_empty(), "blank host in scope");
            prop_assert_eq!(&h.trim().to_ascii_lowercase(), h, "host not normalised: {}", h);
        }
    }
}

#[test]
fn parse_scope_falls_back_on_a_blank_or_punctuation_only_override() {
    // The regressions this locks: None, "", whitespace, and — the one the
    // first version missed — a string of separators with no host between them.
    let default = parse_scope(None);
    assert!(!default.is_empty());
    assert_eq!(parse_scope(Some("")), default);
    assert_eq!(parse_scope(Some("   ")), default);
    assert_eq!(parse_scope(Some(",")), default);
    assert_eq!(parse_scope(Some(" , , ")), default);

    // A real override is honoured and normalised.
    let custom = parse_scope(Some("Example.COM, staging.example.com ,"));
    assert_eq!(
        custom,
        vec!["example.com".to_string(), "staging.example.com".to_string()]
    );
}
