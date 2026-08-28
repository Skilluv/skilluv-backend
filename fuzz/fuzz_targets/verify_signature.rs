#![no_main]
//! Fuzz the webhook HMAC verifier with an arbitrary (secret, body, signature).
//! It must never panic on any bytes — the signature string is fully
//! attacker-controlled (it arrives in a request header), so a panic is a
//! remotely triggerable crash. Correctness (a valid MAC verifies, a tampered
//! one does not) is pinned by tests/prop_pure_parsers.rs against RFC 4231.

use libfuzzer_sys::fuzz_target;
use skilluv_backend::services::linear_sync::verify_signature;

fuzz_target!(|input: (String, Vec<u8>, String)| {
    let (secret, body, signature) = input;
    let _ = verify_signature(&secret, &body, &signature);
});
