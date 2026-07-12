//! P10.2 — Rôles multidisciplinaires sur les teams.
//!
//! Un `team_role_slot` = une case à pourvoir dans une team (ex: "musicien",
//! "animateur 3D", "coder Godot"), potentiellement contrainte par un skill
//! prérequis. Voir `docs/roadmap-p10-p15.md` phase P10.2 pour le rationale.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TeamRoleSlot {
    pub id: Uuid,
    pub team_id: Uuid,
    pub role_slug: String,
    pub role_display_name: Option<String>,
    pub required_skill_id: Option<Uuid>,
    pub min_proficiency_level: i16,
    pub filled_by_user_id: Option<Uuid>,
    pub filled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
