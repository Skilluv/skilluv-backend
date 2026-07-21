//! Tests d'intégration P11.3 : trait SliceIngestor + dispatch générique.
//!
//! Vérifie que :
//! - `FigmaIngestor` (stub) retourne un rapport vide sans crash.
//! - `dispatch_ingestors` appelle tous les ingestors passés en argument.
//! - Un ingestor custom (défini localement dans le test) est composable
//!   sans coupler le service à sa provenance.

use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::errors::AppError;
use skilluv_backend::services::{FigmaIngestor, IngestReport, SliceIngestor, dispatch_ingestors};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p11_3_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await
    .expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("connect");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await;
    admin_pool.close().await;
}

// Un ingestor factice qui rapporte simplement un nombre paramétrable — prouve
// que le trait accepte des impls custom sans coupler le dispatcher.
struct FakeIngestor {
    to_report: u32,
}

#[async_trait]
impl SliceIngestor for FakeIngestor {
    fn name(&self) -> &'static str {
        "fake"
    }
    async fn ingest_for_project(
        &self,
        _db: &PgPool,
        project_id: Uuid,
    ) -> Result<IngestReport, AppError> {
        Ok(IngestReport {
            project_id,
            slices_created: self.to_report,
            ..Default::default()
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// FigmaIngestor stub renvoie report vide sans crasher
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn figma_ingestor_stub_returns_empty_report() {
    let (db, name) = setup_test_db().await;
    let project_id = Uuid::new_v4();

    let report = FigmaIngestor
        .ingest_for_project(&db, project_id)
        .await
        .expect("figma stub");

    assert_eq!(report.project_id, project_id);
    assert_eq!(report.slices_created, 0);
    assert_eq!(report.errors, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// dispatch_ingestors appelle tous les ingestors passés
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dispatch_calls_every_ingestor_in_order() {
    let (db, name) = setup_test_db().await;
    let project_id = Uuid::new_v4();

    let ingestors: Vec<Box<dyn SliceIngestor>> = vec![
        Box::new(FakeIngestor { to_report: 3 }),
        Box::new(FakeIngestor { to_report: 7 }),
        Box::new(FigmaIngestor),
    ];

    let results = dispatch_ingestors(&ingestors, &db, project_id).await;
    assert_eq!(results.len(), 3);

    let created: u32 = results
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|r| r.slices_created))
        .sum();
    assert_eq!(created, (3 + 7));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Le nom du trait est bien exposé pour logs / metrics
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ingestor_names_are_stable() {
    // Ces noms sont utilisés dans les logs + labels de métriques.
    // Les changer casse les dashboards Prometheus/Grafana.
    assert_eq!(FigmaIngestor.name(), "figma");
    assert_eq!(FakeIngestor { to_report: 0 }.name(), "fake");
}
