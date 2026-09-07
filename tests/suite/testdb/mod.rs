//! Shared connection string for the integration suites that open their own
//! database connection (rather than going through `common::TestApp`).
//!
//! These suites used to hardcode `localhost:5433`. That port is easy to
//! shadow: anything else bound to `127.0.0.1:5433` — an SSH tunnel to a
//! remote database, most plausibly — silently wins over the Docker
//! container, and every one of these suites issues `CREATE DATABASE` /
//! `DROP DATABASE` on whatever answers. Pointing the suite somewhere
//! explicit must not require editing source.
//!
//! Override with `TEST_DATABASE_BASE_URL`, e.g.
//! `postgres://skilluv:skilluv_secret@localhost:5439`.
//!
//! Kept dependency-free on purpose: `common::mod` drags in the whole HTTP
//! harness (Redis, MinIO, mock OIDC), which these suites do not need.

#![allow(dead_code)]

/// Base connection string, without a database name and without a trailing
/// slash.
pub fn base() -> String {
    std::env::var("TEST_DATABASE_BASE_URL")
        .unwrap_or_else(|_| "postgres://skilluv:skilluv_secret@localhost:5433".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Connection string for one database on the test server.
pub fn url(db_name: &str) -> String {
    format!("{}/{db_name}", base())
}

/// Connection string for the maintenance database, used to `CREATE` and
/// `DROP` the per-test databases.
pub fn admin_url() -> String {
    url("skilluv")
}
