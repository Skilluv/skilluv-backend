#![no_main]
//! Fuzz the CVSS vector parser with arbitrary bytes. It must never panic; a
//! crash here is a denial-of-service on any endpoint that scores a reported
//! vector. Locally, tests/prop_pure_parsers.rs asserts the same panic-freedom
//! plus the score/tier invariants on Ok results.

use libfuzzer_sys::fuzz_target;
use skilluv_backend::services::cvss;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Success or failure both fine — the contract is "does not panic", and
        // any Ok must stay in range (proptest checks the bound exhaustively).
        if let Ok(scored) = cvss::score_vector(s) {
            assert!((0.0..=10.0).contains(&scored.score));
        }
    }
});
