//! An em dash cannot enter the repository (SKI-370).
//!
//! They read as machine-written, and in this codebase they carried nothing a
//! hyphen could not. Seven thousand nine hundred of them were removed at once;
//! this is what stops the count going back up, one commit at a time, in a way
//! nobody notices until somebody reads the product and hears a machine.
//!
//! The check is a test rather than a lint or a hook because a test is the
//! thing this repository already runs on every change, and because a hook is
//! advice while a red check is an answer.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    const EM_DASH: char = '\u{2014}';

    /// Directories walked. `migrations/` is deliberately absent, and that is
    /// the one exemption in this file.
    ///
    /// `sqlx::migrate!()` validates the checksum of every applied migration at
    /// boot, so changing one byte of a migration that has already run makes
    /// every existing deployment refuse to start with `VersionMismatch`.
    /// Editing them is not a thing anyone may do, so failing a test over their
    /// contents would be asking for something impossible.
    ///
    /// The em dashes that were *in* those files reached the database as rows,
    /// and rows are reachable: a sweep migration rewrites them. What stays is
    /// the SQL comments, inside files that must not change.
    const ROOTS: [&str; 5] = ["src", "docs", "locales", "assets", "proto"];

    /// Skipped wholesale: not ours, or not text.
    fn is_skipped(path: &Path) -> bool {
        let s = path.to_string_lossy().replace('\\', "/");
        s.contains("/target/")
            || s.contains("/node_modules/")
            || s.contains("/.git/")
            // Font binaries and images carry bytes that are not prose.
            || matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("ttf" | "otf" | "woff" | "woff2" | "png" | "jpg" | "jpeg" | "webp" | "ico"
                    | "pdf" | "zip" | "gz" | "bin")
            )
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_skipped(&path) {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    /// Fails naming every file and line, so the fix is mechanical rather than
    /// a hunt.
    #[test]
    fn no_em_dash_anywhere() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        for r in ROOTS {
            walk(&root.join(r), &mut files);
        }
        assert!(
            files.len() > 100,
            "the walk found {} files, which means it is not walking anything \
             and this test would pass on a repository full of em dashes",
            files.len()
        );

        let mut offenders = Vec::new();
        for path in &files {
            let Ok(text) = fs::read_to_string(path) else {
                continue; // not UTF-8: not prose either
            };
            for (i, line) in text.lines().enumerate() {
                if line.contains(EM_DASH) {
                    let rel = path.strip_prefix(root).unwrap_or(path);
                    let shown: String = line.trim().chars().take(90).collect();
                    offenders.push(format!("{}:{}  {}", rel.display(), i + 1, shown));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "{} line(s) carry an em dash. A hyphen replaces it; a colon, comma \
             or full stop where one reads better. See SKI-370.\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }

    /// The exemption is narrow on purpose, and this says so out loud.
    ///
    /// If `migrations` ever appears in `ROOTS`, the test above starts
    /// demanding an edit that breaks every deployment. If the exemption ever
    /// widens to a directory that *can* be edited, the guard quietly stops
    /// covering it.
    #[test]
    fn the_only_exemption_is_the_one_that_cannot_be_edited() {
        assert!(
            !ROOTS.contains(&"migrations"),
            "applied migrations are checksummed by sqlx; editing one stops \
             every deployment from booting. Their rows are fixed by a sweep \
             migration instead."
        );
        assert_eq!(
            ROOTS.len(),
            5,
            "a root was added or removed: check that the new set still covers \
             everything editable"
        );
    }
}
