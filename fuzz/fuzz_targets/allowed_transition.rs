#![no_main]
//! Fuzz the finding-state transition table with arbitrary state names. It is a
//! total function over strings and must never panic; the privilege-boundary
//! invariants (a reporter only reaches `withdrawn`, only an admin publishes, a
//! triager never confirms) are asserted exhaustively in
//! tests/prop_pure_parsers.rs.

use libfuzzer_sys::fuzz_target;
use skilluv_backend::services::security_findings::{allowed_transition, Actor};

fuzz_target!(|input: (u8, String, String)| {
    let (actor_pick, from, to) = input;
    let actor = match actor_pick % 4 {
        0 => Actor::Reporter,
        1 => Actor::Triager,
        2 => Actor::Reviewer,
        _ => Actor::Admin,
    };
    let _ = allowed_transition(actor, &from, &to);
});
