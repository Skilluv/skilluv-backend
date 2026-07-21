//! Service `tracks` — curriculums structurants + DAG des prérequis (Phase P3).
//!
//! Voir docs/challenges-target-model-and-roadmap.md section B.10-B.11 et 5.5.
//!
//! Rôle :
//! - Lister les tracks, s'enroller, calculer la progression et le "next challenge"
//! - Vérifier l'éligibilité d'un user à démarrer un challenge donné (DAG lookup)
//! - Détection de cycle avant insertion dans challenge_prerequisites

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

use crate::errors::AppError;

pub struct TracksService;

/// Ligne de tracks.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Track {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub target_domain: String,
    pub target_phase: String,
    pub estimated_hours: Option<i32>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Ligne de user_tracks.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserTrack {
    pub user_id: Uuid,
    pub track_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub current_challenge_id: Option<Uuid>,
}

/// Progression synthétique d'un user sur un track.
#[derive(Debug, Serialize)]
pub struct TrackProgress {
    pub track: Track,
    pub total_challenges: i64,
    pub completed_challenges: i64,
    pub current_challenge_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Résultat d'un check d'éligibilité pour démarrer un challenge.
#[derive(Debug, Serialize)]
pub struct EligibilityCheck {
    pub eligible: bool,
    pub missing_required_prerequisites: Vec<Uuid>,
    pub missing_recommended_prerequisites: Vec<Uuid>,
    pub reason: Option<String>,
}

impl TracksService {
    // ═══════════════════════════════════════════════════════════════════
    // Tracks : lecture + enrollment
    // ═══════════════════════════════════════════════════════════════════

    pub async fn list_active(db: &PgPool) -> Result<Vec<Track>, AppError> {
        let tracks = sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE active = TRUE
             ORDER BY target_domain, target_phase, name",
        )
        .fetch_all(db)
        .await?;
        Ok(tracks)
    }

    pub async fn get_by_slug(db: &PgPool, slug: &str) -> Result<Track, AppError> {
        sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE slug = $1")
            .bind(slug)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Track '{slug}' not found")))
    }

    /// Enrolle un user dans un track. Idempotent : si déjà enrollé, no-op.
    pub async fn enroll(
        db: &PgPool,
        user_id: Uuid,
        track_slug: &str,
    ) -> Result<UserTrack, AppError> {
        let track = Self::get_by_slug(db, track_slug).await?;
        if !track.active {
            return Err(AppError::Validation(
                "This track is no longer active".to_string(),
            ));
        }

        // Le premier challenge du track (position=0) devient current_challenge_id
        let first_challenge: Option<Uuid> = sqlx::query_scalar(
            "SELECT challenge_id FROM track_challenges
             WHERE track_id = $1 ORDER BY position ASC LIMIT 1",
        )
        .bind(track.id)
        .fetch_optional(db)
        .await?;

        let user_track = sqlx::query_as::<_, UserTrack>(
            r#"
            INSERT INTO user_tracks (user_id, track_id, current_challenge_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, track_id) DO UPDATE SET
                -- Ne rien overwrite si déjà enrollé, juste renvoyer la ligne
                user_id = user_tracks.user_id
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(track.id)
        .bind(first_challenge)
        .fetch_one(db)
        .await?;

        Ok(user_track)
    }

    /// Progression synthétique d'un user sur un track.
    pub async fn get_progress(
        db: &PgPool,
        user_id: Uuid,
        track_slug: &str,
    ) -> Result<TrackProgress, AppError> {
        let track = Self::get_by_slug(db, track_slug).await?;

        let user_track: Option<UserTrack> =
            sqlx::query_as("SELECT * FROM user_tracks WHERE user_id = $1 AND track_id = $2")
                .bind(user_id)
                .bind(track.id)
                .fetch_optional(db)
                .await?;

        let (started_at, completed_at, current_challenge_id) = match user_track {
            Some(ut) => (ut.started_at, ut.completed_at, ut.current_challenge_id),
            None => {
                return Err(AppError::Validation(
                    "User is not enrolled in this track".to_string(),
                ));
            }
        };

        let total_challenges: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM track_challenges WHERE track_id = $1")
                .bind(track.id)
                .fetch_one(db)
                .await?;

        // Combien de challenges du track a-t-il vérifié via un deliverable ?
        let completed_challenges: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT tc.challenge_id)
            FROM track_challenges tc
            WHERE tc.track_id = $1
              AND EXISTS (
                  SELECT 1 FROM deliverables d
                  WHERE d.challenge_id = tc.challenge_id
                    AND d.user_id = $2
                    AND d.verification_status = 'verified'
                    AND d.revoked_at IS NULL
              )
            "#,
        )
        .bind(track.id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        Ok(TrackProgress {
            track,
            total_challenges,
            completed_challenges,
            current_challenge_id,
            started_at,
            completed_at,
        })
    }

    /// Liste les tracks d'un user (enrolled).
    pub async fn list_user_tracks(db: &PgPool, user_id: Uuid) -> Result<Vec<UserTrack>, AppError> {
        let tracks = sqlx::query_as::<_, UserTrack>(
            "SELECT * FROM user_tracks WHERE user_id = $1 ORDER BY started_at DESC",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
        Ok(tracks)
    }

    /// Calcule le prochain challenge du track pour un user donné.
    ///
    /// Logique :
    /// - Trouve le premier challenge de `track_challenges` dans l'ordre `position`
    ///   qui n'a PAS encore de deliverable verified pour ce user
    /// - Retourne None si tous les challenges du track sont complétés
    pub async fn next_challenge_in_track(
        db: &PgPool,
        user_id: Uuid,
        track_id: Uuid,
    ) -> Result<Option<Uuid>, AppError> {
        let next: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT tc.challenge_id
            FROM track_challenges tc
            WHERE tc.track_id = $1
              AND NOT EXISTS (
                  SELECT 1 FROM deliverables d
                  WHERE d.challenge_id = tc.challenge_id
                    AND d.user_id = $2
                    AND d.verification_status = 'verified'
                    AND d.revoked_at IS NULL
              )
            ORDER BY tc.position ASC
            LIMIT 1
            "#,
        )
        .bind(track_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        Ok(next)
    }

    // ═══════════════════════════════════════════════════════════════════
    // DAG : prérequis + éligibilité
    // ═══════════════════════════════════════════════════════════════════

    /// Vérifie si un user est éligible à démarrer un challenge donné.
    ///
    /// Logique :
    /// - Récupère les `challenge_prerequisites` pour ce challenge
    /// - Pour chaque prérequis, vérifie si le user a un deliverable verified
    /// - Si TOUS les prérequis `required=TRUE` sont complétés → éligible
    /// - Les prérequis `required=FALSE` (recommandés) sont retournés séparément
    ///   pour affichage mais ne bloquent pas
    pub async fn check_eligibility(
        db: &PgPool,
        user_id: Uuid,
        challenge_id: Uuid,
    ) -> Result<EligibilityCheck, AppError> {
        let prereqs: Vec<(Uuid, bool)> = sqlx::query_as(
            "SELECT depends_on_challenge_id, required
             FROM challenge_prerequisites
             WHERE challenge_id = $1",
        )
        .bind(challenge_id)
        .fetch_all(db)
        .await?;

        let mut missing_required = Vec::new();
        let mut missing_recommended = Vec::new();

        for (dep_id, required) in prereqs {
            let has_completion: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM deliverables
                    WHERE user_id = $1
                      AND challenge_id = $2
                      AND verification_status = 'verified'
                      AND revoked_at IS NULL
                )",
            )
            .bind(user_id)
            .bind(dep_id)
            .fetch_one(db)
            .await?;

            if !has_completion {
                if required {
                    missing_required.push(dep_id);
                } else {
                    missing_recommended.push(dep_id);
                }
            }
        }

        let eligible = missing_required.is_empty();
        let reason = if eligible {
            None
        } else {
            Some(format!(
                "{} required prerequisite(s) not yet completed",
                missing_required.len()
            ))
        };

        Ok(EligibilityCheck {
            eligible,
            missing_required_prerequisites: missing_required,
            missing_recommended_prerequisites: missing_recommended,
            reason,
        })
    }

    /// Détection de cycle avant d'insérer une nouvelle arête dans le DAG.
    ///
    /// Sémantique du graphe : une ligne `(challenge_id=X, depends_on=Y)`
    /// représente l'arête Y → X (Y est prérequis de X, donc "Y débloque X").
    ///
    /// Pour vérifier si ajouter (challenge_id=A, depends_on=B), soit l'arête
    /// B → A, créerait un cycle, on cherche s'il existe déjà un chemin A → B
    /// dans le graphe existant.
    ///
    /// BFS depuis A, en suivant les arêtes sortantes (les challenges qui
    /// dépendent du noeud courant). Si on atteint B, l'ajout créerait un cycle.
    ///
    /// Retourne true si l'insertion serait sûre (pas de cycle), false sinon.
    pub async fn check_dag_would_not_create_cycle(
        db: &PgPool,
        challenge_id: Uuid,
        depends_on_challenge_id: Uuid,
    ) -> Result<bool, AppError> {
        if challenge_id == depends_on_challenge_id {
            return Ok(false); // self-reference
        }

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut queue: VecDeque<Uuid> = VecDeque::new();
        queue.push_back(challenge_id);
        visited.insert(challenge_id);

        while let Some(current) = queue.pop_front() {
            // Arêtes sortantes : les challenges qui dépendent de `current`
            let dependents: Vec<Uuid> = sqlx::query_scalar(
                "SELECT challenge_id FROM challenge_prerequisites
                 WHERE depends_on_challenge_id = $1",
            )
            .bind(current)
            .fetch_all(db)
            .await?;

            for dep in dependents {
                if dep == depends_on_challenge_id {
                    return Ok(false); // cycle : chemin A → ... → B existe déjà
                }
                if visited.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }

        Ok(true)
    }

    /// Ajoute un prérequis dans le DAG, avec vérification anti-cycle.
    pub async fn add_prerequisite(
        db: &PgPool,
        challenge_id: Uuid,
        depends_on_challenge_id: Uuid,
        required: bool,
    ) -> Result<(), AppError> {
        if !Self::check_dag_would_not_create_cycle(db, challenge_id, depends_on_challenge_id)
            .await?
        {
            return Err(AppError::Validation(
                "Adding this prerequisite would create a cycle in the DAG".to_string(),
            ));
        }

        sqlx::query(
            "INSERT INTO challenge_prerequisites (challenge_id, depends_on_challenge_id, required)
             VALUES ($1, $2, $3)
             ON CONFLICT (challenge_id, depends_on_challenge_id) DO UPDATE SET
                 required = EXCLUDED.required",
        )
        .bind(challenge_id)
        .bind(depends_on_challenge_id)
        .bind(required)
        .execute(db)
        .await?;

        Ok(())
    }
}
