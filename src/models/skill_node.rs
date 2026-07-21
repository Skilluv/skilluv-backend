//! Skill graph atomique.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.3 pour le rationale
//! et `docs/skill-nodes-seed.yaml` pour le seed initial (~290 skills atomiques).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Une compétence atomique (un "geste" mesurable/prouvable) ou une catégorie
/// parent regroupant plusieurs skills.
///
/// - Si `parent_id` est `None` → c'est une catégorie (ex: `frontend-frameworks`)
/// - Si `parent_id` est `Some(_)` → c'est un skill atomique (ex: `react-hooks`)
///
/// Les slices sont tagguées via `slice_skills` (M2M). Les users prouvent leurs
/// skills via `user_skills` mis à jour à chaque `deliverable` vérifié.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SkillNode {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub domain: String,
    pub parent_id: Option<Uuid>,
    pub aliases: Vec<String>,
    pub external_refs: Option<serde_json::Value>,
    pub is_skilluv_specific: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domaines Skilluv (miroir du CHECK constraint SQL).
///
/// Utilisé pour typer les filtres API et les enum côté service.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillDomain {
    Code,
    Design,
    Game,
    Security,
    SoftSkills,
    Ai,
    Ops,
}

impl SkillDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Design => "design",
            Self::Game => "game",
            Self::Security => "security",
            Self::SoftSkills => "soft_skills",
            Self::Ai => "ai",
            Self::Ops => "ops",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        <Self as std::str::FromStr>::from_str(s).ok()
    }
}

impl std::str::FromStr for SkillDomain {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "code" => Ok(Self::Code),
            "design" => Ok(Self::Design),
            "game" => Ok(Self::Game),
            "security" => Ok(Self::Security),
            "soft_skills" => Ok(Self::SoftSkills),
            "ai" => Ok(Self::Ai),
            "ops" => Ok(Self::Ops),
            _ => Err(()),
        }
    }
}
