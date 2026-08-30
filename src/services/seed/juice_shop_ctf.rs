//! Twenty CTF challenges, derived rather than written down (SKI-137).
//!
//! ## Why these could not simply be seeded
//!
//! A flag is checked by comparing a SHA-256. Migration 0558 refuses to seed
//! one for a reason it states plainly: a hash invented by the author of a
//! migration produces a challenge nobody can ever pass, because nobody planted
//! the flag.
//!
//! Juice Shop is the exception, and only because its flags are not arbitrary.
//! The CTF extension derives each one from a single `ctfKey` and the
//! challenge's name. Given the key, the twenty flags are computable — so they
//! can be seeded without anybody inventing anything.
//!
//! ## The trap, and what stops it
//!
//! The key is per instance. Recreate the container and it changes, and twenty
//! challenges become unpassable **silently** — the platform still accepts
//! submissions, it just never matches. That is worse than having no
//! challenges, and it is exactly the failure 0558 is about.
//!
//! Two things stop it.
//!
//! The step is [`super::Body::Configured`], whose ledger version is the key's
//! own fingerprint. Change the key and the step re-runs on the next boot,
//! re-deriving all twenty. Nobody has to remember.
//!
//! And they are seeded as **drafts**, like every other seeded challenge. A
//! person solves one, checks the flag is accepted, and publishes. If the
//! derivation below is ever wrong, it is found on the first challenge by
//! somebody who expected to check, not on the twentieth by somebody who
//! expected to win.
//!
//! ## Unset is not an error
//!
//! A deployment with no Juice Shop is a deployment with no CTF target, and the
//! rest of the catalogue still seeds. This does nothing and says so.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const ENV_URL: &str = "SKILLUV_JUICE_SHOP_URL";
pub const ENV_KEY: &str = "SKILLUV_JUICE_SHOP_CTF_KEY";

/// One challenge, as Juice Shop names it.
///
/// `name` has to be the exact challenge name the CTF extension uses, because
/// it is an input to the flag. The rest is presentation, and is ours.
struct Challenge {
    /// Juice Shop's own name for it. Do not translate or tidy.
    name: &'static str,
    title: &'static str,
    tier: &'static str,
    difficulty: i16,
    minutes: i32,
    what: &'static str,
}

/// Twenty, ordered so the first four are reachable on a first evening.
///
/// Chosen from the official set for spread rather than for count: injection,
/// broken access control, XSS, sensitive data, and a couple that need chaining
/// two findings. A catalogue of twenty easy ones teaches one thing twenty
/// times.
const CHALLENGES: &[Challenge] = &[
    Challenge {
        name: "Score Board",
        title: "Find the score board",
        tier: "trivial",
        difficulty: 1,
        minutes: 10,
        what: "Find the page the application does not link to.",
    },
    Challenge {
        name: "Login Admin",
        title: "Log in as the administrator",
        tier: "easy",
        difficulty: 2,
        minutes: 20,
        what: "The login query is built by concatenation. Get in without the password.",
    },
    Challenge {
        name: "DOM XSS",
        title: "Script injection in the search",
        tier: "easy",
        difficulty: 2,
        minutes: 20,
        what: "The search term reaches the page without being escaped.",
    },
    Challenge {
        name: "Confidential Document",
        title: "Read a document nobody linked",
        tier: "easy",
        difficulty: 2,
        minutes: 15,
        what: "A directory serves more than the page offers.",
    },
    Challenge {
        name: "Password Strength",
        title: "Log in with an obvious password",
        tier: "easy",
        difficulty: 2,
        minutes: 15,
        what: "One account was never given a password worth the name.",
    },
    Challenge {
        name: "Five-Star Feedback",
        title: "Delete a review you do not own",
        tier: "easy",
        difficulty: 3,
        minutes: 25,
        what: "The administration page checks the interface, not the request.",
    },
    Challenge {
        name: "View Basket",
        title: "Read somebody else's basket",
        tier: "easy",
        difficulty: 3,
        minutes: 25,
        what: "The identifier in the request is trusted.",
    },
    Challenge {
        name: "Forged Feedback",
        title: "Post a review as another person",
        tier: "moderate",
        difficulty: 3,
        minutes: 30,
        what: "The author is taken from the body rather than from the session.",
    },
    Challenge {
        name: "Admin Section",
        title: "Reach the administration area",
        tier: "moderate",
        difficulty: 3,
        minutes: 30,
        what: "The route is guarded in the client and nowhere else.",
    },
    Challenge {
        name: "Error Handling",
        title: "Make it tell you too much",
        tier: "easy",
        difficulty: 2,
        minutes: 15,
        what: "An unhandled error is a description of the system.",
    },
    Challenge {
        name: "Deprecated Interface",
        title: "Use an interface that was retired",
        tier: "moderate",
        difficulty: 3,
        minutes: 30,
        what: "Retired in the interface, still answering on the server.",
    },
    Challenge {
        name: "Reset Jim's Password",
        title: "Reset an account through its security question",
        tier: "moderate",
        difficulty: 4,
        minutes: 40,
        what: "A security question whose answer is public is a password everybody has.",
    },
    Challenge {
        name: "Login MC SafeSearch",
        title: "Recover a password from what its owner published",
        tier: "moderate",
        difficulty: 4,
        minutes: 40,
        what: "Somebody said it out loud, on purpose, in a video.",
    },
    Challenge {
        name: "Christmas Special",
        title: "Order a product that was withdrawn",
        tier: "moderate",
        difficulty: 4,
        minutes: 40,
        what: "Deleted rows are still rows.",
    },
    Challenge {
        name: "Product Tampering",
        title: "Change a product you do not own",
        tier: "moderate",
        difficulty: 4,
        minutes: 40,
        what: "A field the interface never sends is a field the server still reads.",
    },
    Challenge {
        name: "Upload Size",
        title: "Upload beyond the stated limit",
        tier: "moderate",
        difficulty: 3,
        minutes: 30,
        what: "The limit is enforced where the user can change it.",
    },
    Challenge {
        name: "Access Log",
        title: "Read the access log",
        tier: "hard",
        difficulty: 5,
        minutes: 60,
        what: "A path that leaves the directory it was meant to stay in.",
    },
    Challenge {
        name: "Blockchain Hype",
        title: "Find what was never published",
        tier: "hard",
        difficulty: 5,
        minutes: 60,
        what: "Chaining two small disclosures into one that is not small.",
    },
    Challenge {
        name: "NoSQL DoS",
        title: "Make a query cost more than it should",
        tier: "hard",
        difficulty: 5,
        minutes: 60,
        what: "An expression accepted as data and evaluated as code.",
    },
    Challenge {
        name: "JWT Issues",
        title: "Forge a token the server accepts",
        tier: "hard",
        difficulty: 6,
        minutes: 90,
        what: "A signature the server is willing not to check.",
    },
];

/// Both settings, or neither.
///
/// The fingerprint is the whole configuration hashed, so a changed key **or** a
/// moved instance re-runs the step. Hashed rather than returned, because this
/// value goes in a ledger row and the key must not.
pub fn declared() -> Option<String> {
    let url = std::env::var(ENV_URL)
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    let key = std::env::var(ENV_KEY)
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    let mut h = Sha256::new();
    h.update(url.trim().as_bytes());
    h.update(b"\0");
    h.update(key.trim().as_bytes());
    Some(hex::encode(h.finalize()))
}

/// The flag for one challenge, as the Juice Shop CTF extension derives it.
///
/// HMAC-SHA256 over the challenge's name, keyed by the instance's `ctfKey`,
/// hex encoded.
///
/// Isolated in one function on purpose. If this construction is ever wrong,
/// every challenge is wrong the same way, and the fix is one line rather than
/// twenty rows — which is also why the challenges are seeded as drafts, so
/// somebody checks one before anybody is asked to solve twenty.
pub fn flag_for(ctf_key: &str, challenge_name: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(ctf_key.as_bytes())
        .expect("HMAC accepts a key of any length");
    mac.update(challenge_name.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// Seed, or explain why not.
pub async fn run(db: &PgPool, owner: Uuid) -> Result<String, AppError> {
    let (Ok(url), Ok(key)) = (std::env::var(ENV_URL), std::env::var(ENV_KEY)) else {
        return Ok(format!(
            "{ENV_URL} / {ENV_KEY} not set; no CTF target, nothing seeded"
        ));
    };
    let (url, key) = (url.trim().trim_end_matches('/'), key.trim());
    if url.is_empty() || key.is_empty() {
        return Ok(format!("{ENV_URL} / {ENV_KEY} empty; nothing seeded"));
    }

    let mut written = 0usize;
    for c in CHALLENGES {
        // The flag is derived, hashed, and never stored in either form.
        let flag_hash = sha256_hex(&flag_for(key, c.name));

        // Keyed on the title so a re-run after a key change updates the hash
        // in place rather than leaving a second copy somebody could claim.
        sqlx::query(
            "INSERT INTO challenge_templates (
                 title, description, instructions, skill_domain, difficulty,
                 status, is_training, ai_policy, created_by, duration_minutes,
                 reward_fragments, security_kind, security_difficulty_tier,
                 security_flag_hash, security_flag_format, security_target_url,
                 security_attribution_md)
             VALUES ($1, $2, $3, 'security', $4,
                     'draft', TRUE, 'disclosure_required', $5, $6,
                     $7, 'ctf_flag', $8, $9, $10, $11, $12)
             ON CONFLICT (title) WHERE skill_domain = 'security' DO UPDATE
                 SET security_flag_hash = EXCLUDED.security_flag_hash,
                     security_target_url = EXCLUDED.security_target_url",
        )
        .bind(c.title)
        .bind(c.what)
        .bind(format!(
            "The target is {url}. Find the flag, then submit it here.\n\n\
             {}\n\nNothing you do to this instance affects anything else — it \
             is rebuilt regularly and it holds no real data.",
            c.what
        ))
        .bind(c.difficulty)
        .bind(owner)
        .bind(c.minutes)
        .bind(i32::from(c.difficulty) * 40)
        .bind(c.tier)
        .bind(&flag_hash)
        .bind("64 hexadecimal characters")
        .bind(url)
        .bind(
            "Target: OWASP Juice Shop, by Björn Kimminich, Apache-2.0. \
             The challenge names are theirs; the wording here is ours.",
        )
        .execute(db)
        .await?;
        written += 1;
    }

    Ok(format!(
        "{written} CTF challenges derived for {url}, all draft — solve one and \
         publish before announcing them"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_challenges_with_no_repeated_name_or_title() {
        assert_eq!(CHALLENGES.len(), 20);
        // A repeated Juice Shop name would derive the same flag twice, and a
        // repeated title would fight the upsert's conflict target.
        for (i, a) in CHALLENGES.iter().enumerate() {
            for b in &CHALLENGES[i + 1..] {
                assert_ne!(a.name, b.name, "duplicate challenge name {}", a.name);
                assert_ne!(a.title, b.title, "duplicate title {}", a.title);
            }
        }
    }

    #[test]
    fn the_difficulty_actually_spreads() {
        // Twenty easy challenges teach one thing twenty times. The catalogue
        // has to reach somebody on their first evening and still have
        // somewhere for them to go.
        let tiers: std::collections::HashSet<&str> = CHALLENGES.iter().map(|c| c.tier).collect();
        assert!(tiers.len() >= 4, "only {} tiers: {tiers:?}", tiers.len());
        assert!(CHALLENGES.iter().any(|c| c.difficulty <= 2));
        assert!(CHALLENGES.iter().any(|c| c.difficulty >= 5));
    }

    #[test]
    fn a_flag_depends_on_both_the_key_and_the_challenge() {
        // The property the whole design rests on: two instances derive
        // different flags, so a key rotation invalidates every one of them —
        // which is why the ledger version is the key's fingerprint.
        let a = flag_for("key-one", "Login Admin");
        let b = flag_for("key-two", "Login Admin");
        let c = flag_for("key-one", "DOM XSS");
        assert_ne!(a, b, "the key does not change the flag");
        assert_ne!(a, c, "the challenge does not change the flag");
        assert_eq!(a.len(), 64, "not a hex SHA-256: {a}");
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn the_fingerprint_moves_with_the_configuration() {
        // Not a test of `declared()` — it reads process environment that every
        // other test shares. This is the same computation, on values.
        let fp = |url: &str, key: &str| {
            let mut h = Sha256::new();
            h.update(url.as_bytes());
            h.update(b"\0");
            h.update(key.as_bytes());
            hex::encode(h.finalize())
        };
        assert_ne!(
            fp("https://ctf.example", "k1"),
            fp("https://ctf.example", "k2"),
            "a rotated key must re-run the step"
        );
        assert_ne!(
            fp("https://ctf.example", "k1"),
            fp("https://other.example", "k1"),
            "a moved instance must re-run the step"
        );
        // And the separator does its job: url+key must not collide with a
        // different split of the same characters.
        assert_ne!(fp("ab", "c"), fp("a", "bc"));
    }
}
