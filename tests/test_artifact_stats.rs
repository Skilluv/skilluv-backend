//! Storing what a registry said — including when it said nothing.

mod common;
use common::TestApp;
use skilluv_backend::services::package_registry::{PackageRef, PackageStats, record};
use uuid::Uuid;

async fn a_published_library(app: &TestApp, url: &str) -> Uuid {
    app.register_user("pkg_owner").await;
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'pkg_owner'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(format!("pkg-{}", Uuid::new_v4().simple()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, code_subtype,
             code_package_registry_url, difficulty)
         VALUES ($1, 'Biblio', 'x', 'code', 'code_artifact', 'library_published', $2, 3)
         RETURNING id",
    )
    .bind(project)
    .bind(url)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn figures_are_stored_with_the_date_they_were_read() {
    let app = TestApp::spawn().await;
    let slice = a_published_library(&app, "https://crates.io/crates/exemple").await;

    record(
        &app.db,
        slice,
        &PackageRef {
            registry: "crates_io",
            name: "exemple".into(),
        },
        Ok(PackageStats {
            latest_version: Some("1.2.3".into()),
            downloads_total: Some(12_000),
            downloads_recent: Some(400),
            dependents_count: None,
        }),
    )
    .await
    .expect("record");

    let (version, total, fetched): (Option<String>, Option<i64>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT latest_version, downloads_total, fetched_at
               FROM code_package_stats WHERE slice_id = $1",
        )
        .bind(slice)
        .fetch_one(&app.db)
        .await
        .unwrap();

    assert_eq!(version.as_deref(), Some("1.2.3"));
    assert_eq!(total, Some(12_000));
    // A download count with no date is a number nobody can situate.
    assert!(fetched <= chrono::Utc::now());
}

#[tokio::test]
async fn a_registry_that_publishes_nothing_stores_nothing_rather_than_zero() {
    let app = TestApp::spawn().await;
    let slice = a_published_library(&app, "https://pkg.go.dev/github.com/x/y").await;

    // Go modules report no download count. Zero would claim nobody uses it.
    record(
        &app.db,
        slice,
        &PackageRef {
            registry: "go_modules",
            name: "github.com/x/y".into(),
        },
        Ok(PackageStats::default()),
    )
    .await
    .unwrap();

    let total: Option<i64> =
        sqlx::query_scalar("SELECT downloads_total FROM code_package_stats WHERE slice_id = $1")
            .bind(slice)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(total, None, "unmeasured is not zero");
}

#[tokio::test]
async fn a_failed_fetch_keeps_the_previous_figures() {
    let app = TestApp::spawn().await;
    let slice = a_published_library(&app, "https://crates.io/crates/exemple").await;
    let package = PackageRef {
        registry: "crates_io",
        name: "exemple".into(),
    };

    record(
        &app.db,
        slice,
        &package,
        Ok(PackageStats {
            latest_version: Some("1.0.0".into()),
            downloads_total: Some(999),
            downloads_recent: None,
            dependents_count: None,
        }),
    )
    .await
    .unwrap();

    record(
        &app.db,
        slice,
        &package,
        Err(skilluv_backend::errors::AppError::Internal(
            "crates.io down".into(),
        )),
    )
    .await
    .unwrap();

    let (total, error): (Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT downloads_total, last_error FROM code_package_stats WHERE slice_id = $1",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // An old figure with a visible date beats no figure, and beats a zero
    // that reads as "nobody uses this".
    assert_eq!(total, Some(999));
    assert!(error.unwrap_or_default().contains("down"));
}

#[tokio::test]
async fn one_slice_can_publish_to_two_registries() {
    let app = TestApp::spawn().await;
    let slice = a_published_library(&app, "https://crates.io/crates/exemple").await;

    for registry in ["crates_io", "npm"] {
        record(
            &app.db,
            slice,
            &PackageRef {
                registry,
                name: "exemple".into(),
            },
            Ok(PackageStats::default()),
        )
        .await
        .unwrap();
    }

    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM code_package_stats WHERE slice_id = $1")
            .bind(slice)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(rows, 2, "two registries is two rows, which is true");
}

#[tokio::test]
async fn recording_twice_updates_rather_than_duplicates() {
    let app = TestApp::spawn().await;
    let slice = a_published_library(&app, "https://crates.io/crates/exemple").await;
    let package = PackageRef {
        registry: "crates_io",
        name: "exemple".into(),
    };

    for downloads in [10, 20, 30] {
        record(
            &app.db,
            slice,
            &package,
            Ok(PackageStats {
                downloads_total: Some(downloads),
                ..PackageStats::default()
            }),
        )
        .await
        .unwrap();
    }

    let (rows, total): (i64, Option<i64>) = sqlx::query_as(
        "SELECT count(*), max(downloads_total) FROM code_package_stats WHERE slice_id = $1",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(rows, 1);
    assert_eq!(total, Some(30));
}
