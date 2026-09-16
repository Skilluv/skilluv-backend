//! Every cookie that carries a session says `SameSite=Lax`, and says it once.
//!
//! They did not. `routes/auth.rs` set `Strict`, `routes/oauth.rs` set `Lax`,
//! and `routes/oauth.rs` set the refresh token `Strict` in the same response
//! as its own `Lax` access token. Nothing held them together, so the value
//! drifted per file and per author.
//!
//! What that cost: a browser withholds a `Strict` cookie on a navigation
//! another site began, and every OAuth provider return is exactly that. The
//! link succeeded, the session survived, and the person landed signed out -
//! which reads as "linking GitHub does not work" and is a different sentence
//! from "GitHub linking is broken". Two days went to the difference.
//!
//! `Lax` still refuses a cross-site POST, which is the classic CSRF path. The
//! part it no longer refuses is a cross-site top-level GET, and
//! `middleware::csrf` is what stands in for that - see its header for why
//! `CSRF_ENFORCE` matters more now than it did.
//!
//! A test rather than a convention, for the reason `no_em_dashes` is one:
//! this repository runs its tests on every change, and a convention is advice
//! while a red check is an answer.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// Walk `src/`, return every line that names a SameSite attribute.
    fn same_site_lines() -> Vec<(String, usize, String)> {
        fn walk(dir: &Path, out: &mut Vec<(String, usize, String)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let Ok(text) = fs::read_to_string(&path) else {
                        continue;
                    };
                    for (i, line) in text.lines().enumerate() {
                        if line.contains("SameSite=") {
                            out.push((
                                path.to_string_lossy().replace('\\', "/"),
                                i + 1,
                                line.trim().to_string(),
                            ));
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(Path::new("src"), &mut out);
        out
    }

    #[test]
    fn no_session_cookie_is_same_site_strict() {
        let offenders: Vec<String> = same_site_lines()
            .into_iter()
            // This file documents the rule, so it names the word it forbids.
            .filter(|(file, _, _)| !file.ends_with("no_strict_session_cookies.rs"))
            .filter(|(_, _, line)| line.contains("SameSite=Strict"))
            .map(|(file, n, line)| format!("  {file}:{n}: {line}"))
            .collect();

        assert!(
            offenders.is_empty(),
            "a `Strict` cookie is not sent on a navigation another site began, \
             and every OAuth provider return is one - the person lands signed \
             out of a session that worked. Use `SameSite=Lax`:\n{}",
            offenders.join("\n")
        );
    }

    /// The check above only means something while there are cookies to check.
    ///
    /// A refactor that moved cookie construction behind a helper this test
    /// cannot see would leave it passing over nothing, which is the failure
    /// mode of every grep-shaped test.
    #[test]
    fn the_cookies_are_still_where_this_can_see_them() {
        let found = same_site_lines()
            .into_iter()
            .filter(|(file, _, _)| !file.ends_with("no_strict_session_cookies.rs"))
            .count();
        assert!(
            found >= 10,
            "only {found} SameSite attributes found in src/ - either the cookies \
             moved somewhere this test cannot read, or it is now guarding nothing"
        );
    }
}
