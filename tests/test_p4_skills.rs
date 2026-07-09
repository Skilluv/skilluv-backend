//! Tests d'intégration Phase P4 : skills service.
//!
//! Couvre :
//! - list_skills catalogue (avec filtre domaine)
//! - list_user_skills : profil enrichi
//! - find_talents_by_skill : ranking + min_level + profile_active gate
//! - recommend_slices_for_user : détection near-levelup + scoring par weight

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{SkillsService, TalentSearchFilter};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p4_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
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
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    ))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\""))
        .execute(&admin_pool)
        .await;
    admin_pool.close().await;
}

async fn insert_test_user(db: &PgPool, user_id: Uuid, profile_active: bool) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind("Test")
    .bind("User")
    .bind("Test User")
    .bind("dummy_hash")
    .bind(profile_active)
    .execute(db)
    .await
    .expect("insert user");
}

async fn insert_user_skill(
    db: &PgPool,
    user_id: Uuid,
    skill_id: Uuid,
    proven_count: i32,
    wpc: i32,
    proficiency_level: i16,
) {
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(proven_count)
    .bind(wpc)
    .bind(proficiency_level)
    .execute(db)
    .await
    .expect("insert user_skill");
}

async fn get_two_code_skills(db: &PgPool) -> (Uuid, Uuid) {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 2",
    )
    .fetch_all(db)
    .await
    .expect("skills");
    (ids[0], ids[1])
}

async fn get_skill_slug(db: &PgPool, skill_id: Uuid) -> String {
    sqlx::query_scalar("SELECT slug FROM skill_nodes WHERE id = $1")
        .bind(skill_id)
        .fetch_one(db)
        .await
        .expect("slug")
}

// ═══════════════════════════════════════════════════════════════════
// list_skills : catalogue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_skills_returns_all_seeded_skills() {
    let (db, db_name) = setup_test_db().await;

    let all_skills = SkillsService::list_skills(&db, None)
        .await
        .expect("list all");
    assert!(all_skills.len() >= 300, "seed contains ~337 skills");

    let code_only = SkillsService::list_skills(&db, Some("code"))
        .await
        .expect("list code");
    assert!(
        code_only.iter().all(|s| s.domain == "code"),
        "domain filter should only return code"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// list_user_skills : profil
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_user_skills_returns_only_proven_ones_sorted_by_level() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, true).await;

    let (skill_a, skill_b) = get_two_code_skills(&db).await;

    // Skill A : niveau 3 (WPC 10), skill B : niveau 1 (WPC 2)
    insert_user_skill(&db, user_id, skill_a, 4, 10, 3).await;
    insert_user_skill(&db, user_id, skill_b, 1, 2, 1).await;

    // Skill sans proven (proven_count = 0) → ne doit pas apparaître
    let (skill_c, _) = get_two_code_skills(&db).await;
    if skill_c != skill_a && skill_c != skill_b {
        insert_user_skill(&db, user_id, skill_c, 0, 0, 1).await;
    }

    let skills = SkillsService::list_user_skills(&db, user_id)
        .await
        .expect("list");

    assert_eq!(skills.len(), 2, "only 2 skills with proven_count > 0");
    // Trié par proficiency DESC → skill A (level 3) avant skill B (level 1)
    assert_eq!(skills[0].skill_id, skill_a);
    assert_eq!(skills[0].proficiency_level, 3);
    assert_eq!(skills[1].skill_id, skill_b);
    assert_eq!(skills[1].proficiency_level, 1);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// find_talents_by_skill : recherche recruteur
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_talents_ranks_by_level_then_wpc() {
    let (db, db_name) = setup_test_db().await;
    let (skill_id, _) = get_two_code_skills(&db).await;
    let slug = get_skill_slug(&db, skill_id).await;

    let expert = Uuid::new_v4();
    insert_test_user(&db, expert, true).await;
    insert_user_skill(&db, expert, skill_id, 10, 30, 5).await;

    let advanced = Uuid::new_v4();
    insert_test_user(&db, advanced, true).await;
    insert_user_skill(&db, advanced, skill_id, 5, 20, 4).await;

    let junior = Uuid::new_v4();
    insert_test_user(&db, junior, true).await;
    insert_user_skill(&db, junior, skill_id, 1, 2, 1).await;

    let filter = TalentSearchFilter {
        min_level: 3,
        page: 1,
        per_page: 20,
    };
    let (talents, total) = SkillsService::find_talents_by_skill(&db, &slug, &filter)
        .await
        .expect("search");

    // Junior (level 1) exclu par min_level 3
    assert_eq!(total, 2);
    assert_eq!(talents.len(), 2);
    assert_eq!(talents[0].user_id, expert, "level 5 first");
    assert_eq!(talents[1].user_id, advanced);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn find_talents_excludes_inactive_profiles() {
    let (db, db_name) = setup_test_db().await;
    let (skill_id, _) = get_two_code_skills(&db).await;
    let slug = get_skill_slug(&db, skill_id).await;

    let inactive = Uuid::new_v4();
    insert_test_user(&db, inactive, false).await;
    insert_user_skill(&db, inactive, skill_id, 10, 30, 5).await;

    let active = Uuid::new_v4();
    insert_test_user(&db, active, true).await;
    insert_user_skill(&db, active, skill_id, 5, 15, 4).await;

    let filter = TalentSearchFilter {
        min_level: 1,
        page: 1,
        per_page: 20,
    };
    let (talents, total) = SkillsService::find_talents_by_skill(&db, &slug, &filter)
        .await
        .expect("search");

    assert_eq!(total, 1);
    assert_eq!(talents[0].user_id, active);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn find_talents_returns_404_for_unknown_skill() {
    let (db, db_name) = setup_test_db().await;

    let res = SkillsService::find_talents_by_skill(
        &db,
        "totally-made-up-skill-xyz",
        &TalentSearchFilter {
            min_level: 1,
            page: 1,
            per_page: 20,
        },
    )
    .await;
    assert!(res.is_err());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// recommend_slices_for_user : recommandations near-levelup
// ═══════════════════════════════════════════════════════════════════

/// Helpers pour créer un projet + une slice ouverte taggée avec un skill donné.
async fn setup_project_with_open_slice_touching_skill(
    db: &PgPool,
    skill_id: Uuid,
    weight: i16,
) -> Uuid {
    let owner = Uuid::new_v4();
    insert_test_user(db, owner, true).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Rec Project', 'user', $2) RETURNING id",
    )
    .bind(format!("rp-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("proj");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Recommended slice', 'X', 'code', 3, 'open')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(db)
    .await
    .expect("slice");

    sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(slice_id)
    .bind(skill_id)
    .bind(weight)
    .execute(db)
    .await
    .expect("slice_skills");

    slice_id
}

#[tokio::test]
async fn recommendations_prioritize_skills_close_to_levelup() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, true).await;

    let (skill_near, skill_far) = get_two_code_skills(&db).await;

    // skill_near : level 2 (WPC 6), à 1 point de level 3 (threshold 7) → recommandable
    insert_user_skill(&db, user_id, skill_near, 3, 6, 2).await;
    // skill_far : level 1 (WPC 0 après compensation) → il est à 3 points du threshold 3 → juste dans la fenêtre 3
    // Note : notre fenêtre est ≤ 3, donc les deux devraient être eligible
    // Pour ce test on veut vraiment prioriser skill_near : rendons skill_far loin
    insert_user_skill(&db, user_id, skill_far, 5, 20, 4).await;
    // skill_far : level 4 (WPC 20), à 11 points de level 5 (threshold 31) → HORS fenêtre 3 → pas dans les recos

    setup_project_with_open_slice_touching_skill(&db, skill_near, 4).await;
    setup_project_with_open_slice_touching_skill(&db, skill_far, 5).await;

    let recs = SkillsService::recommend_slices_for_user(&db, user_id, 10)
        .await
        .expect("recs");

    // Only the slice touching skill_near should be recommended
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].matched_skills[0].skill_id, skill_near);
    assert_eq!(recs[0].matched_skills[0].current_level, 2);
    assert_eq!(recs[0].matched_skills[0].next_level_wpc_threshold, 7);
    assert_eq!(recs[0].total_match_score, 4);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn recommendations_empty_when_no_skill_near_levelup() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, true).await;

    let (skill_id, _) = get_two_code_skills(&db).await;

    // WPC = 1 → level 1, threshold 3, gap = 2 (dans fenêtre 3)
    // Pour tester "empty when no near-levelup", utilisons un skill VIDE :
    // pas d'entrée user_skills → la fonction retourne vide

    // Sans slice ouverte non plus, mais surtout sans user_skill : le user n'a rien commencé
    let recs = SkillsService::recommend_slices_for_user(&db, user_id, 10)
        .await
        .expect("recs");
    assert!(recs.is_empty(), "user with no skills → no recos");

    // Maintenant, insérons un skill à level 5 (max) → pas de threshold suivant
    insert_user_skill(&db, user_id, skill_id, 20, 100, 5).await;
    setup_project_with_open_slice_touching_skill(&db, skill_id, 3).await;

    let recs = SkillsService::recommend_slices_for_user(&db, user_id, 10)
        .await
        .expect("recs after level 5");
    assert!(recs.is_empty(), "level 5 skill has no next threshold → no recos");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn recommendations_dedupe_and_score_slices_hitting_multiple_skills() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, true).await;

    let (skill_a, skill_b) = get_two_code_skills(&db).await;

    // Both skills near level-up (level 1, WPC 2, threshold 3, gap 1)
    insert_user_skill(&db, user_id, skill_a, 1, 2, 1).await;
    insert_user_skill(&db, user_id, skill_b, 1, 2, 1).await;

    // Une slice qui touche les DEUX skills → score total = weight_a + weight_b
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner, true).await;
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Dual Project', 'user', $2) RETURNING id",
    )
    .bind(format!("dp-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(&db)
    .await
    .expect("proj");
    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Dual slice', 'X', 'code', 3, 'open') RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&db)
    .await
    .expect("slice");
    sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary)
         VALUES ($1, $2, 3, TRUE), ($1, $3, 4, FALSE)",
    )
    .bind(slice_id)
    .bind(skill_a)
    .bind(skill_b)
    .execute(&db)
    .await
    .expect("slice_skills");

    let recs = SkillsService::recommend_slices_for_user(&db, user_id, 10)
        .await
        .expect("recs");

    assert_eq!(recs.len(), 1);
    let rec = &recs[0];
    assert_eq!(rec.matched_skills.len(), 2, "both skills matched");
    assert_eq!(rec.total_match_score, 7, "3 + 4 = 7");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
