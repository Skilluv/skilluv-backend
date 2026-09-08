//! An em dash cannot enter the repository (SKI-370).
//!
//! They read as machine-written, and here they carried nothing a hyphen could
//! not. Around 7900 were removed at once; this is what stops the count going
//! back up, one commit at a time, in a way nobody notices until somebody
//! reads the product and hears a machine.
//!
//! The check is a test rather than a hook because a test is what this
//! repository already runs on every change, and because a hook is advice
//! while a red check is an answer.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    const EM_DASH: char = '\u{2014}';

    /// The one exemption, and it is not a preference.
    ///
    /// `sqlx::migrate!()` validates the checksum of every applied migration at
    /// boot, so changing one byte of a migration that has already run makes
    /// every existing deployment refuse to start with `VersionMismatch`.
    /// Failing a test over their contents would demand something nobody may
    /// do.
    ///
    /// Their em dashes reached the database as rows, and rows are reachable:
    /// migration 0621 rewrites every text and jsonb column. What stays is SQL
    /// comments inside files that must not change.
    const EXEMPT_PREFIX: &str = "migrations/";

    /// Asks git what is in the repository rather than walking the filesystem.
    ///
    /// The first version walked directories, and failed on a developer's own
    /// `.env`, `.server.log` and scratch notes: files that are not in the
    /// repository, that CI never sees, and that nobody should be told to edit
    /// by a test. `git ls-files` is exactly the set this guard is about.
    fn tracked_files(root: &Path) -> Vec<String> {
        let out = Command::new("git")
            .arg("ls-files")
            .current_dir(root)
            .output()
            .expect("git ls-files: this guard needs a git checkout to know what it covers");
        assert!(
            out.status.success(),
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// Binary formats: bytes, not prose.
    fn is_binary(path: &str) -> bool {
        matches!(
            path.rsplit('.').next(),
            Some(
                "ttf"
                    | "otf"
                    | "woff"
                    | "woff2"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "ico"
                    | "pdf"
                    | "zip"
                    | "gz"
                    | "bin"
                    | "wasm"
            )
        )
    }

    /// Fails naming every file and line, so the fix is mechanical rather than
    /// a hunt.
    #[test]
    fn no_em_dash_anywhere() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let files = tracked_files(root);

        assert!(
            files.len() > 500,
            "git ls-files returned {} paths, which is too few to be this \
             repository: the guard would pass on anything",
            files.len()
        );

        let mut offenders = Vec::new();
        let mut checked = 0usize;
        for rel in &files {
            if rel.starts_with(EXEMPT_PREFIX) || is_binary(rel) {
                continue;
            }
            let Ok(text) = fs::read_to_string(root.join(rel)) else {
                continue; // not UTF-8, so not prose either
            };
            checked += 1;
            for (i, line) in text.lines().enumerate() {
                if line.contains(EM_DASH) {
                    let shown: String = line.trim().chars().take(90).collect();
                    offenders.push(format!("{rel}:{}  {shown}", i + 1));
                }
            }
        }

        assert!(
            checked > 400,
            "only {checked} files were read; the filter is excluding almost \
             everything and this guard covers nothing"
        );

        assert!(
            offenders.is_empty(),
            "{} line(s) carry an em dash. A hyphen replaces it; a colon, comma \
             or full stop where one reads better. See SKI-370.\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }

    /// The exemption stays narrow, and says so out loud.
    ///
    /// If it ever widens to something that *can* be edited, the guard quietly
    /// stops covering it, which is worse than having no guard because it looks
    /// like one.
    #[test]
    fn the_only_exemption_is_the_one_that_cannot_be_edited() {
        assert_eq!(
            EXEMPT_PREFIX, "migrations/",
            "applied migrations are checksummed by sqlx; editing one stops \
             every deployment from booting. Their rows are cleaned by a sweep \
             migration instead. Nothing else has that excuse."
        );
    }
}
