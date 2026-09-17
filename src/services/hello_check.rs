//! Reading a HELLO.md without a person, and knowing when that is enough.
//!
//! ## Why this rite and not the other eleven
//!
//! Every rite here goes through the same human review, and that is the
//! platform's whole claim: somebody read the work and said so. This one is
//! the exception, and the exception is narrow on purpose.
//!
//! The entrance rite makes no claim about competence. What it asserts is "I
//! can fork a repository, edit a file, and open a pull request" - and that is
//! mechanically true or mechanically false. A reviewer opening a diff that
//! adds one sentence under a heading is not exercising judgement; they are
//! acknowledging receipt, and charging somebody a wait for an acknowledgement
//! is the wrong first impression for a platform whose subject is doing.
//!
//! So: the entrance is decided here, and everything after it is decided by a
//! person. The moment a rite's artifact *is* the work, this module has no
//! business near it.
//!
//! ## Why the diff and not the file
//!
//! The obvious check reads the submitted file and looks for a sentence. That
//! answers the wrong question, and it answered it badly: the Hello Wall used
//! to store the whole file, so every entry on it was ninety-five per cent the
//! same instructions, repeated once per member.
//!
//! The template is ours. We wrote it, it sits in `skilluv-community`, and we
//! can read it. So a person's contribution is exactly what their file has
//! that ours does not - which settles both questions at once:
//!
//!   * "did they fill in their part, or delete everything and paste?" - a
//!     deletion shows up as a structural line of ours that is no longer
//!     there, and that is arithmetic rather than opinion;
//!   * "what do we put on the wall?" - their lines, not our instructions.
//!
//! ## Why the structural check is loose
//!
//! It asks that the headings and the placeholder marker survive, not that
//! every byte matches. Somebody who fixes a typo in our template, or improves
//! the French, would fail a byte-exact check - and being refused by a machine
//! for improving the thing is the false positive that teaches a community to
//! work around the check. `design_auto_checks` makes the same argument about
//! blocking checks, and it is right.

use sha2::{Digest, Sha256};

/// Below this, an introduction is a keystroke rather than a sentence.
///
/// Deliberately low. The rite is the gesture, not the prose, and somebody
/// writing in a second language should not be refused for being brief.
pub const MIN_INTRODUCTION_CHARS: usize = 30;

/// The lines that have to still be there.
///
/// Headings and the placeholder marker: the shape of the file, not its words.
/// A person may correct our text; they may not replace the file.
const STRUCTURAL_MARKERS: &[&str] = &["# Bonjour Skilluv", "## My introduction / Ma présentation"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelloVerdict {
    /// Everything checks out; the string is what goes on the wall.
    Accepted { introduction: String },
    /// Something is mechanically wrong, said in words the person can act on.
    Refused { reason: String },
}

impl HelloVerdict {
    pub fn accepted(&self) -> bool {
        matches!(self, HelloVerdict::Accepted { .. })
    }
}

/// What a person added to our template.
///
/// Line-wise rather than word-wise: the unit somebody writes in, and the unit
/// the wall renders. Blank lines and the HTML comment that marks the spot are
/// dropped - they are ours, not theirs, and a wall entry that is three empty
/// lines is not an introduction.
pub fn extract_introduction(template: &str, submitted: &str) -> String {
    let ours: Vec<&str> = template.lines().map(str::trim).collect();
    submitted
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("<!--"))
        .filter(|line| !ours.contains(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Decide the entrance rite from the two files and the facts around them.
///
/// Pure: every input is already in hand by the time this is called, and no
/// check here needs the network. That is what makes it testable, and what
/// makes each refusal reproducible from the strings alone.
pub fn judge(
    template: &str,
    submitted: &str,
    files_changed: &[String],
    pr_author: &str,
    rite_owner_github_login: &str,
) -> HelloVerdict {
    // 1. The template survived.
    //
    // This is the "did they delete everything" check, and it is the reason
    // the whole thing reads the diff rather than the file.
    for marker in STRUCTURAL_MARKERS {
        if !submitted.contains(marker) {
            return HelloVerdict::Refused {
                reason: format!(
                    "HELLO.md no longer contains `{marker}`. Add your line to the \
                     file rather than replacing it - the template is how the next \
                     person finds their way."
                ),
            };
        }
    }

    // 2. One file, and it is that one.
    //
    // An entrance pull request that also edits `package.json` is doing
    // something else, and something else is not what this decides.
    let touched: Vec<&String> = files_changed.iter().collect();
    if !touched.iter().any(|f| f.as_str() == "HELLO.md") {
        return HelloVerdict::Refused {
            reason: "this pull request does not touch HELLO.md.".into(),
        };
    }
    if touched.len() > 1 {
        let others: Vec<&str> = touched
            .iter()
            .map(|f| f.as_str())
            .filter(|f| *f != "HELLO.md")
            .collect();
        return HelloVerdict::Refused {
            reason: format!(
                "the entrance rite changes HELLO.md and nothing else; this also \
                 changes {}. Open a separate pull request for that.",
                others.join(", ")
            ),
        };
    }

    // 3. It is their own.
    //
    // Compared case-insensitively because GitHub logins are, and a rite
    // credited to the wrong account is worse than one not credited at all.
    if !pr_author.eq_ignore_ascii_case(rite_owner_github_login) {
        return HelloVerdict::Refused {
            reason: format!(
                "the pull request was opened by `{pr_author}`, and this rite belongs \
                 to `{rite_owner_github_login}`."
            ),
        };
    }

    // 4. There is something there.
    let introduction = extract_introduction(template, submitted);
    if introduction.is_empty() {
        return HelloVerdict::Refused {
            reason: "HELLO.md is unchanged - the introduction is still the template.".into(),
        };
    }
    let chars = introduction.chars().count();
    if chars < MIN_INTRODUCTION_CHARS {
        return HelloVerdict::Refused {
            reason: format!(
                "the introduction is {chars} characters; {MIN_INTRODUCTION_CHARS} is \
                 the least this asks for. Say who you are and what you want to build."
            ),
        };
    }

    HelloVerdict::Accepted { introduction }
}

/// SHA-256 of what a person wrote, for deduplication.
pub fn introduction_hash(introduction: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(introduction.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "# Bonjour Skilluv\n\
\n\
Welcome to your first Skilluv project!\n\
\n\
Edit this file by adding a line under `## My introduction` with:\n\
- Your first name or handle\n\
\n\
---\n\
\n\
## My introduction / Ma présentation\n\
\n\
<!-- Add your line here / Ajoute ta ligne ici -->\n";

    fn with_intro(line: &str) -> String {
        format!("{TEMPLATE}\n{line}\n")
    }

    fn files() -> Vec<String> {
        vec!["HELLO.md".to_string()]
    }

    #[test]
    fn an_ordinary_introduction_is_accepted_and_extracted_alone() {
        let submitted = with_intro(
            "Ama, Accra. I want to build tools that outlive the job that paid for them.",
        );
        let v = judge(TEMPLATE, &submitted, &files(), "ama", "ama");
        match v {
            HelloVerdict::Accepted { introduction } => {
                assert!(introduction.starts_with("Ama, Accra"));
                // The wall gets their sentence, not our instructions.
                assert!(!introduction.contains("Welcome to your first"));
                assert!(!introduction.contains("Bonjour Skilluv"));
                assert!(!introduction.contains("<!--"));
            }
            other => panic!("expected acceptance, got {other:?}"),
        }
    }

    /// The question this module was written for.
    #[test]
    fn deleting_the_template_and_pasting_your_own_is_refused() {
        let submitted = "# Moi\n\nAma, Accra. Je construis des outils.\n";
        let v = judge(TEMPLATE, submitted, &files(), "ama", "ama");
        match v {
            HelloVerdict::Refused { reason } => {
                assert!(reason.contains("Bonjour Skilluv"), "{reason}");
                assert!(reason.contains("rather than replacing"), "{reason}");
            }
            other => panic!("a wiped template must be refused, got {other:?}"),
        }
    }

    #[test]
    fn an_untouched_template_is_refused_rather_than_passed() {
        let v = judge(TEMPLATE, TEMPLATE, &files(), "ama", "ama");
        assert!(!v.accepted(), "{v:?}");
    }

    /// The placeholder comment is ours. Copying it is not writing.
    #[test]
    fn the_placeholder_alone_does_not_count_as_an_introduction() {
        let submitted = format!("{TEMPLATE}\n<!-- Add your line here -->\n");
        let v = judge(TEMPLATE, &submitted, &files(), "ama", "ama");
        assert!(!v.accepted(), "{v:?}");
    }

    #[test]
    fn a_keystroke_is_not_a_sentence() {
        let v = judge(TEMPLATE, &with_intro("ok"), &files(), "ama", "ama");
        match v {
            HelloVerdict::Refused { reason } => assert!(reason.contains("characters"), "{reason}"),
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_rite_credited_to_the_wrong_account_is_refused() {
        let submitted = with_intro("Ama, Accra. I want to build tools that last a while.");
        let v = judge(TEMPLATE, &submitted, &files(), "someone-else", "ama");
        match v {
            HelloVerdict::Refused { reason } => {
                assert!(reason.contains("someone-else"), "{reason}")
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    /// GitHub logins are case-insensitive, and a rite lost to capitalisation
    /// would be a refusal nobody could act on.
    #[test]
    fn the_author_comparison_ignores_case() {
        let submitted = with_intro("Ama, Accra. I want to build tools that last a while.");
        assert!(judge(TEMPLATE, &submitted, &files(), "AMA", "ama").accepted());
    }

    #[test]
    fn a_pull_request_that_changes_more_than_hello_is_refused() {
        let submitted = with_intro("Ama, Accra. I want to build tools that last a while.");
        let touched = vec!["HELLO.md".to_string(), "package.json".to_string()];
        match judge(TEMPLATE, &submitted, &touched, "ama", "ama") {
            HelloVerdict::Refused { reason } => {
                assert!(reason.contains("package.json"), "{reason}")
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_pull_request_that_misses_hello_entirely_is_refused() {
        let submitted = with_intro("Ama, Accra. I want to build tools that last a while.");
        let touched = vec!["README.md".to_string()];
        assert!(!judge(TEMPLATE, &submitted, &touched, "ama", "ama").accepted());
    }

    /// Improving our template is not replacing it.
    ///
    /// The structural check reads headings, not bytes, exactly so that the
    /// first person who fixes a typo is not refused by a machine for making
    /// the thing better.
    #[test]
    fn correcting_the_template_is_allowed() {
        let fixed = TEMPLATE.replace(
            "Welcome to your first Skilluv project!",
            "Welcome to your first Skilluv project.",
        );
        let submitted = format!("{fixed}\nAma, Accra. I build tools that outlive their job.\n");
        let v = judge(TEMPLATE, &submitted, &files(), "ama", "ama");
        assert!(v.accepted(), "{v:?}");
        // Their correction rides along in the extract, which is honest: they
        // did write it.
        match v {
            HelloVerdict::Accepted { introduction } => assert!(introduction.contains("Ama, Accra")),
            _ => unreachable!(),
        }
    }

    #[test]
    fn the_hash_is_stable_and_distinguishes() {
        assert_eq!(introduction_hash("a"), introduction_hash("a"));
        assert_ne!(introduction_hash("a"), introduction_hash("b"));
    }
}
