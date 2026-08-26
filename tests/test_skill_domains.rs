//! The domain list, now that there is only one of it.
//!
//! Migration 0400 turned ten CHECK constraints into foreign keys onto
//! `skill_domains`. These tests hold the two things that can still drift: the
//! Rust mirror used by the request-path guards, and the assumption that every
//! column holding a domain actually points at the table.

mod common;
use common::TestApp;

use skilluv_backend::validators::SKILL_DOMAINS;

/// The mirror in `validators.rs` and the active rows must say the same thing.
///
/// This is the whole reason the constant is allowed to exist: eight modules
/// used to keep a list of their own, three had gone stale, and nothing said
/// so. One list plus one assertion is the trade.
#[tokio::test]
async fn the_rust_domain_list_matches_the_table() {
    let app = TestApp::spawn().await;

    let active: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM skill_domains WHERE is_active ORDER BY slug")
            .fetch_all(&app.db)
            .await
            .unwrap();

    let mut mirrored: Vec<String> = SKILL_DOMAINS.iter().map(|s| s.to_string()).collect();
    mirrored.sort();

    assert_eq!(
        active, mirrored,
        "validators::SKILL_DOMAINS and the active rows of skill_domains disagree — \
         one of them was updated and the other was not"
    );
}

/// Every column that holds a domain points at the table.
///
/// Written as a query over the catalogue rather than as a list of tables,
/// because a list of tables is the thing this migration exists to stop
/// maintaining: a new table with a `skill_domain` column and no foreign key
/// fails here rather than a year later, when somebody inserts `'Audio'` into
/// it and every query looking for `audio` quietly misses the row.
#[tokio::test]
async fn every_domain_column_references_the_table() {
    let app = TestApp::spawn().await;

    // The columns that name a domain, by naming convention, minus the ones
    // that hold something else entirely (a DNS name, an e-mail domain).
    let unreferenced: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT c.table_name::TEXT, c.column_name::TEXT
          FROM information_schema.columns c
          JOIN information_schema.tables t
            ON t.table_name = c.table_name
           AND t.table_schema = 'public'
           AND t.table_type = 'BASE TABLE'
         WHERE c.table_schema = 'public'
           AND c.data_type = 'character varying'
           AND (c.column_name = 'skill_domain'
                OR c.column_name = 'primary_domain'
                OR c.column_name = 'target_domain'
                OR c.column_name = 'desired_domain'
                OR (c.column_name = 'domain' AND c.table_name <> 'tenants'))
           AND NOT EXISTS (
                SELECT 1
                  FROM pg_constraint con
                  JOIN pg_class rel ON rel.oid = con.conrelid
                  JOIN pg_attribute att
                    ON att.attrelid = con.conrelid
                   AND att.attnum = ANY (con.conkey)
                 WHERE con.contype = 'f'
                   AND rel.relname = c.table_name
                   AND att.attname = c.column_name
                   AND con.confrelid = 'skill_domains'::REGCLASS
           )
         ORDER BY 1, 2
        "#,
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        unreferenced.is_empty(),
        "these columns hold a domain and nothing checks it — \
         add a foreign key onto skill_domains: {unreferenced:?}"
    );
}

/// A domain nobody declared is refused, wherever it is written.
#[tokio::test]
async fn an_undeclared_domain_is_refused_by_the_database() {
    let app = TestApp::spawn().await;

    let refused = sqlx::query(
        "INSERT INTO review_grids (domain, display_name, criteria)
         VALUES ('sorcery', 'Sorcellerie', '[{\"criterion\": \"x\", \"looks_like\": \"y\"}]')",
    )
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a review grid was filed under a domain that does not exist"
    );
}

/// A declared domain that is not open yet still accepts rows.
///
/// This is the distinction the `is_active` flag carries: `craft_score_tiers`
/// has held rows for domains that were not open since migration 0204, and a
/// foreign key that refused them would have had to delete them.
///
/// The test used to point at whichever domain happened to be closed — first
/// `quality`, then `communication`. That made it a tripwire: it failed every
/// time a domain opened, and each failure was a correct assertion about a
/// stale fact, which is the worst kind to leave in a suite. Every declared
/// domain is open now, so there is no closed one to point at and the property
/// is asserted directly instead: declare one here, and check the keys accept
/// its rows.
#[tokio::test]
async fn a_declared_but_inactive_domain_still_holds_its_rows() {
    let app = TestApp::spawn().await;

    sqlx::query(
        "INSERT INTO skill_domains (slug, name, description, display_category, is_active)
         VALUES ('notopenyet', 'Not open yet',
                 'Declared so its rows have somewhere to live, and closed so nobody \
                  is offered a domain with no catalogue behind it.',
                 'craft', FALSE)",
    )
    .execute(&app.db)
    .await
    .expect("a domain may be declared before it opens");

    // The rows a closed domain is allowed to carry. This is the whole point:
    // the tiers are seeded long before the trades exist, and a key that
    // refused them would have forced the seed to wait for the domain.
    let inserted = sqlx::query(
        "INSERT INTO craft_score_tiers
             (skill_domain, slug, name, min_score, max_score, description, sort_order)
         VALUES ('notopenyet', 'apprentice', 'Apprentice', 0, 99, 'The first tier.', 1)",
    )
    .execute(&app.db)
    .await
    .expect("a closed domain still holds its tiers");
    assert_eq!(inserted.rows_affected(), 1);

    // And it is not offered while it is closed.
    let offered: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'notopenyet'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!offered, "a domain with no catalogue must not be offered");

    // The other half of the same key: a domain that was never declared is
    // refused, so `is_active` is the only thing separating the two cases.
    let undeclared = sqlx::query(
        "INSERT INTO craft_score_tiers
             (skill_domain, slug, name, min_score, max_score, description, sort_order)
         VALUES ('nosuchdomain', 'apprentice', 'Apprentice', 0, 99, 'x', 1)",
    )
    .execute(&app.db)
    .await;
    assert!(
        undeclared.is_err(),
        "tiers were filed under a domain that does not exist"
    );
}

/// The reading category of a skill comes from its domain's row.
#[tokio::test]
async fn a_skill_takes_the_reading_category_of_its_domain() {
    let app = TestApp::spawn().await;

    let category: String = sqlx::query_scalar(
        "INSERT INTO skill_nodes (slug, display_name, domain)
         VALUES ('probe-spatial-audio', 'Probe', 'ai')
         RETURNING display_category",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        category, "understand",
        "the trigger no longer reads the category from skill_domains"
    );
}

/// A slug column uses one separator, whichever it picked.
///
/// The repository has both, and settled them per table rather than globally:
/// `orientations` and `badge_rules` are kebab, `tournament_kinds`,
/// `mission_types`, `slice_types` and `portfolio_platforms` are snake. Either
/// is fine and neither is worth a rename. What is not fine is one column
/// holding both, because a slug goes in a URL and a consumer cannot guess:
/// `award_categories` had thirty kebab rows and sixteen snake ones, seeded by
/// three different domains, so the same page linked
/// `/awards/best-docs-contribution` beside `/awards/quality_bug_of_the_year`.
/// Migration 0538 settled it.
///
/// `badge_rules` carries the one exemption: its nine snake rows are all
/// `legacy_*`, deprecated and named so deliberately, and renaming them would
/// hide what the prefix is there to say.
#[tokio::test]
async fn a_slug_column_uses_one_separator() {
    let app = TestApp::spawn().await;

    for (table, exempt_prefix) in [
        ("award_categories", None),
        ("orientations", None),
        ("tournament_kinds", None),
        ("badge_rules", Some("legacy_")),
        ("terrain_proposals", None),
        ("portfolio_platforms", None),
        ("mission_types", None),
        ("slice_types", None),
    ] {
        // The table name is one of the eight literals above, never input.
        let (snake, kebab): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FILTER (WHERE strpos(slug, '_') > 0),
                    count(*) FILTER (WHERE strpos(slug, '-') > 0)
               FROM {table}
              WHERE $1::TEXT IS NULL OR strpos(slug, $1) <> 1"
        )))
        .bind(exempt_prefix)
        .fetch_one(&app.db)
        .await
        .unwrap();

        assert!(
            snake == 0 || kebab == 0,
            "{table} holds {snake} snake_case slugs and {kebab} kebab-case ones,              and a reader of a URL cannot tell which one a given row will be"
        );
    }
}
