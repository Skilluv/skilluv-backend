//! Everything a fresh database needs, applied by the deployment that created it.
//!
//! ## What was wrong before
//!
//! There were twelve seeds: five binaries and seven SQL scripts. Every one of
//! them was something an operator had to remember to run, in an order written
//! down nowhere, and nothing recorded whether they had. A database restored
//! from scratch had migrations and no catalogue, and the only way to find out
//! was to open the app and see empty pages.
//!
//! Four of the seven scripts could not have worked anyway. They resolved their
//! owner with `WHERE email = 'admin@skilluv.local'` while `seed_admin` creates
//! `admin@skill-uv.com`, so the CTE was empty, the `INSERT ... SELECT` inserted
//! nothing, and `psql` exited 0. Two more carried a hard-coded owner UUID from
//! one developer's laptop. A seed that reports success and does nothing is
//! worse than one nobody ran: the second gets noticed.
//!
//! ## What this is
//!
//! One ordered list, one entry point, one ledger table. [`run`] is called after
//! the migrations on every boot; it reads `seed_runs`, skips every step whose
//! version is already recorded, and applies the rest. On a database that is up
//! to date it costs one `SELECT`.
//!
//! ## The order is a dependency order
//!
//! It is not alphabetical and it is not arbitrary. The admin account owns the
//! projects; the projects are what the season deliverables attach challenges
//! to; the badge rule is what the onboarding challenges award. A step that runs
//! before what it needs does not fail loudly — it inserts nothing, which is the
//! failure this module exists to end. So the list is written once, here, in the
//! order the data depends on itself.
//!
//! ## Versions, and why editing a seed re-applies it
//!
//! Each step carries a version: the SHA-256 of its SQL, or a string the author
//! bumps for the ones written in Rust. Change the content and the version
//! moves, so the next deployment applies the step again rather than leaving the
//! database on the old content for ever. Every step is still individually
//! idempotent — the ledger saves the work, it does not make re-running safe.
//! That was already true and it stays true.
//!
//! ## What is deliberately not here
//!
//! `skilluv-seed` (fake users and submissions) and `skilluv-seed-guild` (an
//! end-to-end fixture) are not in the catalogue and must not be. They exist to
//! make a development database look busy; a production boot that ran them would
//! put invented people on the leaderboard. Both keep their own binaries and
//! both stay opt-in.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub mod admin_account;
pub mod design_canvas;
pub mod projects;

/// Where a step's data lives, which decides how its version is computed.
enum Body {
    /// SQL applied verbatim, with `$1` textually replaced by the owner's id.
    ///
    /// Textual rather than bound because several of these files hold more than
    /// one statement, and a parameterised query may not. The substituted value
    /// is a [`Uuid`], so there is nothing a caller could put in it — the type
    /// is the check, not an escaping routine.
    Sql(&'static str),
    /// A function, with a version its author maintains.
    Rust { version: &'static str, run: StepFn },
}

/// A seed step written in Rust: takes the pool and the owner, reports what it
/// did. Boxed because a `fn` pointer cannot name an `async fn`'s opaque future,
/// and the catalogue has to hold all of them in one list.
type StepFn = fn(
    &PgPool,
    Uuid,
) -> std::pin::Pin<Box<dyn Future<Output = Result<String, AppError>> + Send + '_>>;

struct Step {
    name: &'static str,
    /// One line, printed when the step runs. What it is for, not what it does.
    purpose: &'static str,
    body: Body,
    /// False for the one step that creates the owner every other step needs.
    needs_owner: bool,
}

/// Every step, in the order the data depends on itself.
fn catalogue() -> Vec<Step> {
    vec![
        Step {
            name: "admin_account",
            purpose: "the account every seeded project and challenge is owned by",
            body: Body::Rust {
                version: "1",
                run: |db, _| Box::pin(admin_account::run(db)),
            },
            needs_owner: false,
        },
        Step {
            name: "oss_partners",
            purpose: "the twelve partner repositories of Annexe F",
            body: Body::Sql(include_str!("sql/oss_partners.sql")),
            needs_owner: true,
        },
        Step {
            name: "projects",
            purpose: "our own repositories, the partners and the wider ecosystem",
            body: Body::Rust {
                version: "1",
                run: |db, owner| Box::pin(projects::run(db, owner)),
            },
            needs_owner: true,
        },
        Step {
            name: "oss_partners_ingestion",
            purpose: "which upstream labels become claimable slices",
            body: Body::Sql(include_str!("sql/oss_partners_ingestion.sql")),
            needs_owner: true,
        },
        Step {
            name: "flagships",
            purpose: "the two projects Skilluv stewards itself",
            body: Body::Sql(include_str!("sql/flagships.sql")),
            needs_owner: true,
        },
        Step {
            name: "onboarding_challenges",
            purpose: "one first challenge per starter template",
            body: Body::Sql(include_str!("sql/onboarding_challenges.sql")),
            needs_owner: true,
        },
        Step {
            name: "badge_rule_bonjour_skilluv",
            purpose: "the badge the first merged pull request awards",
            body: Body::Sql(include_str!("sql/badge_rule_bonjour_skilluv.sql")),
            needs_owner: true,
        },
        Step {
            name: "season1_deliverables",
            purpose: "season one and its ten deliverables",
            body: Body::Sql(include_str!("sql/season1_deliverables.sql")),
            needs_owner: true,
        },
        Step {
            name: "season2_deliverables",
            purpose: "season two and its deliverables",
            body: Body::Sql(include_str!("sql/season2_deliverables.sql")),
            needs_owner: true,
        },
        Step {
            name: "design_canvas",
            purpose: "design work on our own surfaces",
            body: Body::Rust {
                version: "1",
                run: |db, owner| Box::pin(design_canvas::run(db, owner)),
            },
            needs_owner: true,
        },
    ]
}

/// What one step did.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StepOutcome {
    pub name: String,
    /// False when the ledger already held this version.
    pub ran: bool,
    /// What it reported, or why it was skipped.
    pub detail: String,
}

/// What a whole run did.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub steps: Vec<StepOutcome>,
    pub applied: usize,
    pub skipped: usize,
    /// True when at least one step could not run for want of the admin
    /// account. Not an error — a deployment with no `SEED_ADMIN_PASSWORD` is a
    /// deployment that has not been told who the administrator is yet — but
    /// the caller is told, because a half-seeded database should not be
    /// discovered later.
    pub blocked_on_owner: bool,
}

fn version_of(body: &Body) -> String {
    match body {
        Body::Sql(sql) => {
            let mut hasher = Sha256::new();
            // Line endings are normalised first. A checkout with `core.autocrlf`
            // on would otherwise re-apply every SQL step on every deployment,
            // which is exactly the pointless work the ledger exists to avoid.
            hasher.update(sql.replace("\r\n", "\n").as_bytes());
            hex::encode(hasher.finalize())
        }
        // Padded to the same width as a hash, because the column is CHAR(64)
        // and a short string would be silently space-padded by Postgres and
        // then never compare equal to what was written.
        Body::Rust { version, .. } => format!("{version:0>64}"),
    }
}

/// Whether this database has been told who its administrator is.
///
/// Looked up by role rather than by a fixed address: an operator who set
/// `SEED_ADMIN_EMAIL` to their own domain has an administrator, and hard-coding
/// one address here would be the same mistake the old scripts made.
async fn owner(db: &PgPool) -> Result<Option<Uuid>, AppError> {
    let configured = std::env::var("SEED_ADMIN_EMAIL")
        .ok()
        .map(|e| e.to_lowercase());

    if let Some(email) = configured {
        let by_email: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM users WHERE email = $1 AND role = 'admin'")
                .bind(&email)
                .fetch_optional(db)
                .await?;
        if by_email.is_some() {
            return Ok(by_email);
        }
    }

    // Otherwise the oldest administrator. Oldest rather than any, so two runs
    // against the same database agree on who owns what — a seed whose owner
    // changes between deployments rewrites `owner_id` on every project it
    // touches.
    Ok(sqlx::query_scalar(
        "SELECT id FROM users WHERE role = 'admin' ORDER BY created_at, id LIMIT 1",
    )
    .fetch_optional(db)
    .await?)
}

/// Apply every step this database has not had, in order.
///
/// Safe to call on every boot and on every replica: the ledger row is written
/// with the step, so two processes starting together do the work twice at
/// worst — which every step already tolerates — and never leave a step
/// half-recorded.
pub async fn run(db: &PgPool) -> Result<Report, AppError> {
    let applied: HashMap<String, String> =
        sqlx::query_as::<_, (String, String)>("SELECT name, version FROM seed_runs")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();

    let mut report = Report {
        steps: Vec::new(),
        applied: 0,
        skipped: 0,
        blocked_on_owner: false,
    };

    // Resolved once, then again after `admin_account` — which is the step that
    // may have created it.
    let mut owner_id = owner(db).await?;

    for step in catalogue() {
        let version = version_of(&step.body);

        if applied.get(step.name) == Some(&version) {
            report.skipped += 1;
            report.steps.push(StepOutcome {
                name: step.name.to_string(),
                ran: false,
                detail: "already applied".into(),
            });
            continue;
        }

        if step.needs_owner && owner_id.is_none() {
            report.blocked_on_owner = true;
            report.steps.push(StepOutcome {
                name: step.name.to_string(),
                ran: false,
                detail: "skipped: this database has no administrator yet".into(),
            });
            tracing::warn!(
                step = step.name,
                "seed step skipped — no admin account. Set SEED_ADMIN_PASSWORD and restart, \
                 or run `skilluv-seed-admin`."
            );
            continue;
        }

        tracing::info!(step = step.name, purpose = step.purpose, "seeding");

        let detail = match &step.body {
            Body::Sql(sql) => {
                let owner = owner_id.expect("needs_owner checked above");
                // A UUID has no representation that could end the literal, so
                // the substitution cannot change the statement's shape. Bound
                // parameters are not available: several of these files hold
                // more than one statement.
                let prepared = sql.replace("$1", &format!("'{owner}'"));
                // `execute` on a multi-statement `RawSql` reports the rows of
                // the last statement only, so the number is a sign of life
                // rather than a total. That is what it is used for: zero from a
                // step that has never run is the silent-no-op these files spent
                // a year doing.
                let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(prepared))
                    .execute(db)
                    .await?
                    .rows_affected();
                format!("{rows} rows on the last statement")
            }
            Body::Rust { run, .. } => {
                // `admin_account` is the one step with nothing to own; the
                // nil id it is handed is never read.
                run(db, owner_id.unwrap_or_else(Uuid::nil)).await?
            }
        };

        sqlx::query(
            "INSERT INTO seed_runs (name, version, detail)
             VALUES ($1, $2, $3)
             ON CONFLICT (name) DO UPDATE SET
                 previous_version = seed_runs.version,
                 version = EXCLUDED.version,
                 detail = EXCLUDED.detail,
                 applied_at = NOW()",
        )
        .bind(step.name)
        .bind(&version)
        .bind(&detail)
        .execute(db)
        .await?;

        tracing::info!(step = step.name, %detail, "seeded");
        report.applied += 1;
        report.steps.push(StepOutcome {
            name: step.name.to_string(),
            ran: true,
            detail,
        });

        // The step that creates the administrator is the reason every later
        // step has an owner at all.
        if !step.needs_owner {
            owner_id = owner(db).await?;
        }
    }

    Ok(report)
}

/// Forget a step, so the next run applies it again.
///
/// The supported way to re-seed one thing without touching the other nine, and
/// the reason the ledger is a plain table rather than something clever.
pub async fn forget(db: &PgPool, name: &str) -> Result<bool, AppError> {
    let deleted = sqlx::query("DELETE FROM seed_runs WHERE name = $1")
        .bind(name)
        .execute(db)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Every step name, for a caller that wants to validate one before forgetting it.
pub fn step_names() -> Vec<&'static str> {
    catalogue().into_iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_has_a_distinct_name() {
        let names = step_names();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            names.len(),
            sorted.len(),
            "two steps share a name, so one would mask the other's ledger row"
        );
    }

    #[test]
    fn only_the_first_step_may_run_without_an_owner() {
        // Everything else writes rows owned by somebody. A step that claimed
        // otherwise would run against an empty database and insert nothing,
        // which is the failure this module was written to end.
        let steps = catalogue();
        assert!(!steps[0].needs_owner, "{}", steps[0].name);
        for step in &steps[1..] {
            assert!(step.needs_owner, "{} must wait for an owner", step.name);
        }
    }

    #[test]
    fn a_version_is_always_sixty_four_characters() {
        // The ledger column is CHAR(64). A shorter value comes back
        // space-padded and never compares equal to what was written, so the
        // step would re-run on every single boot.
        for step in catalogue() {
            assert_eq!(version_of(&step.body).len(), 64, "{}", step.name);
        }
    }

    /// The statements a step actually runs, with `--` comments removed.
    ///
    /// The assertions below are about executable SQL, not about prose. Each of
    /// these files carries a header explaining what it replaced, and that
    /// header quotes the old broken lookup verbatim — so a naive `contains`
    /// matches the explanation and fails on a file that is correct.
    fn executable_sql(sql: &str) -> String {
        sql.lines()
            .map(|line| match line.find("--") {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn a_sql_step_reads_its_owner_from_the_parameter() {
        // The address the old scripts looked up. Nothing creates it, so any
        // step still naming it seeds nothing and says it succeeded.
        for step in catalogue() {
            if let Body::Sql(sql) = &step.body {
                assert!(
                    !executable_sql(sql).contains("email = 'admin@skilluv.local'"),
                    "{} still resolves an owner that is never created",
                    step.name
                );
            }
        }
    }

    #[test]
    fn no_sql_step_carries_a_hard_coded_owner() {
        // One developer's user id, in two of the scripts. On any other
        // database it is a foreign key to nothing.
        for step in catalogue() {
            if let Body::Sql(sql) = &step.body {
                assert!(
                    !executable_sql(sql).contains("527b047b-32a2-4b7d-a623-3bacdc751578"),
                    "{} carries a hard-coded owner",
                    step.name
                );
            }
        }
    }

    #[test]
    fn a_sql_step_that_needs_an_owner_names_the_parameter() {
        for step in catalogue() {
            if let Body::Sql(sql) = &step.body {
                // `oss_partners_ingestion` and the badge rule own nothing:
                // they update rows other steps created.
                let sql = executable_sql(sql);
                let owns_rows = sql.contains("owner_id") || sql.contains("created_by");
                assert!(
                    !owns_rows || sql.contains("$1"),
                    "{} writes owned rows without taking the owner",
                    step.name
                );
            }
        }
    }
}
