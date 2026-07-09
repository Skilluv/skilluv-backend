//! Progression skill-par-skill d'un user.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.9 et partie G.2
//! pour la formule de proficiency.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Progression d'un user sur un skill donné.
///
/// Alimenté par le workflow G.2 : à chaque deliverable vérifié, pour chaque
/// `slice_skill` de la slice, on incrémente `proven_count` et
/// `weighted_proven_count`, puis on recalcule `proficiency_level` via
/// `min(5, ceil(log2(WPC + 1)))`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSkill {
    pub user_id: Uuid,
    pub skill_id: Uuid,

    /// Nombre brut de deliverables vérifiés touchant ce skill.
    pub proven_count: i32,

    /// Somme pondérée des `slice_skills.weight` sur les slices vérifiées.
    /// Alimente la formule proficiency.
    pub weighted_proven_count: i32,

    /// Niveau maîtrise 1-5 calculé par `min(5, ceil(log2(WPC + 1)))` :
    /// - WPC 1-2   → level 1 (débutant)
    /// - WPC 3-6   → level 2 (initié)
    /// - WPC 7-14  → level 3 (compétent)
    /// - WPC 15-30 → level 4 (avancé)
    /// - WPC 31+   → level 5 (expert)
    pub proficiency_level: i16,

    /// Top 5 preuves (IDs de deliverables triés par `fragments_awarded` DESC)
    /// affichées sur le profil public sous ce skill.
    pub top_proof_deliverable_ids: Vec<Uuid>,

    pub first_proven_at: Option<DateTime<Utc>>,
    pub last_proven_at: Option<DateTime<Utc>>,
}

impl UserSkill {
    /// Calcule le proficiency_level à partir du weighted_proven_count.
    ///
    /// Formule : `min(5, ceil(log2(WPC + 1)))` — voir partie G.2 du doc pour
    /// le rationale (progression lente au haut du spectre, récompense la profondeur
    /// autant que la répétition).
    pub fn proficiency_level_for(weighted_proven_count: i32) -> i16 {
        if weighted_proven_count <= 0 {
            return 1;
        }
        let wpc_plus_one = (weighted_proven_count + 1) as f64;
        let level = wpc_plus_one.log2().ceil() as i16;
        level.clamp(1, 5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proficiency_level_formula() {
        // Vérifications de la table de correspondance de G.2
        assert_eq!(UserSkill::proficiency_level_for(0), 1);
        assert_eq!(UserSkill::proficiency_level_for(1), 1);
        assert_eq!(UserSkill::proficiency_level_for(2), 2);
        assert_eq!(UserSkill::proficiency_level_for(3), 2);
        assert_eq!(UserSkill::proficiency_level_for(6), 3);
        assert_eq!(UserSkill::proficiency_level_for(7), 3);
        assert_eq!(UserSkill::proficiency_level_for(14), 4);
        assert_eq!(UserSkill::proficiency_level_for(15), 4);
        assert_eq!(UserSkill::proficiency_level_for(30), 5);
        assert_eq!(UserSkill::proficiency_level_for(31), 5);
        assert_eq!(UserSkill::proficiency_level_for(1000), 5);
    }
}
