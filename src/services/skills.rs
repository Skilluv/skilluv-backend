//! Service `skills` — expose le skill graph aux profils, recherche recruteur,
//! et recommandations de slices (Phase P4).
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections B.3, B.9, 8.3, 8.6.
//!
//! Le skill graph est un cœur produit : il rend visible ce qu'un contributeur
//! sait faire (profil), permet aux recruteurs de chercher précisément, et guide
//! les contributeurs vers leurs prochains slices via des recommandations
//! ciblées sur les skills proches d'un level-up.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub struct SkillsService;

/// Ordering options pour `list_user_skill_fragments_or_backfill` (P8.6).
///
/// Miroir les 3 ORDER BY différents utilisés par les consumers legacy :
/// gamification, profile, public_api.
#[derive(Debug, Clone, Copy)]
pub enum SkillFragmentOrder {
    /// gamification.rs, public_api.rs (variant 1)
    ByDomainThenSubskill,
    /// profile.rs
    ByFragmentsDesc,
    /// public_api.rs (variant 2)
    ByDomainThenFragmentsDesc,
}

/// Une skill enrichie avec la progression du user (vue "mes skills").
#[derive(Debug, Serialize)]
pub struct UserSkillEnriched {
    pub skill_id: Uuid,
    pub skill_slug: String,
    pub skill_display_name: String,
    pub skill_domain: String,
    pub skill_parent_id: Option<Uuid>,
    pub proven_count: i32,
    pub weighted_proven_count: i32,
    pub proficiency_level: i16,
    pub first_proven_at: Option<DateTime<Utc>>,
    pub last_proven_at: Option<DateTime<Utc>>,
    pub top_proof_deliverable_ids: Vec<Uuid>,
}

/// Un talent trouvé pour un skill donné (vue recruteur).
#[derive(Debug, Serialize)]
pub struct SkillTalent {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub proficiency_level: i16,
    pub proven_count: i32,
    pub weighted_proven_count: i32,
    pub last_proven_at: Option<DateTime<Utc>>,
}

/// Une recommandation de slice basée sur les skills proches d'un level-up.
#[derive(Debug, Serialize)]
pub struct SliceRecommendation {
    pub slice_id: Uuid,
    pub slice_title: String,
    pub slice_primary_domain: String,
    pub slice_difficulty: i16,
    pub project_id: Uuid,
    pub project_name: String,
    /// Skills touchés par cette slice pour lesquels le user est près d'un level-up.
    pub matched_skills: Vec<RecommendationSkillMatch>,
    pub total_match_score: i32,
}

#[derive(Debug, Serialize)]
pub struct RecommendationSkillMatch {
    pub skill_id: Uuid,
    pub skill_slug: String,
    pub skill_display_name: String,
    pub current_wpc: i32,
    pub current_level: i16,
    pub next_level_wpc_threshold: i32,
    pub weight_in_slice: i16,
}

/// Filtre pour rechercher des talents par skill.
#[derive(Debug, Clone, Default)]
pub struct TalentSearchFilter {
    pub min_level: i16,
    pub page: i64,
    pub per_page: i64,
}

impl SkillsService {
    // ═══════════════════════════════════════════════════════════════════
    // P8.6 : consumers legacy `skill_fragments` — fallback vers user_skills
    // ═══════════════════════════════════════════════════════════════════

    /// Retourne des `SkillFragment` compatibles avec l'ancien format legacy.
    ///
    /// Stratégie de fallback (P8.6) :
    /// 1. Si `skill_fragments` contient au moins une ligne pour le user → SELECT direct
    ///    (comportement historique conservé).
    /// 2. Sinon → SELECT depuis `user_skills` + JOIN `skill_nodes`, construit
    ///    des SkillFragment "synthétiques" avec :
    ///    - skill_domain = skill_nodes.domain
    ///    - sub_skill = skill_nodes.slug
    ///    - fragments = weighted_proven_count (approximation raisonnable)
    ///    - id = fresh Uuid (les consumers legacy ne persistent pas cet id)
    ///    - updated_at = user_skills.last_proven_at (fallback NOW si NULL)
    ///
    /// L'ORDER BY est appliqué côté SQL selon le paramètre. Les 3 consumers
    /// legacy (gamification, profile, public_api) ont des ORDER BY différents ;
    /// on paramètre pour préserver leur ordering historique.
    pub async fn list_user_skill_fragments_or_backfill(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        order: SkillFragmentOrder,
    ) -> Result<Vec<crate::models::SkillFragment>, AppError> {
        // Existe-t-il des skill_fragments legacy pour ce user ?
        let has_legacy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM skill_fragments WHERE user_id = $1)",
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if has_legacy {
            let order_by = match order {
                SkillFragmentOrder::ByDomainThenSubskill => "skill_domain, sub_skill",
                SkillFragmentOrder::ByFragmentsDesc => "fragments DESC",
                SkillFragmentOrder::ByDomainThenFragmentsDesc => "skill_domain, fragments DESC",
            };
            let sql = format!(
                "SELECT * FROM skill_fragments WHERE user_id = $1 ORDER BY {order_by}"
            );
            let rows = sqlx::query_as::<_, crate::models::SkillFragment>(&sql)
                .bind(user_id)
                .fetch_all(db)
                .await?;
            return Ok(rows);
        }

        // Fallback : construire depuis user_skills + skill_nodes
        use chrono::Utc;
        let rows: Vec<(String, String, i32, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            r#"
            SELECT sn.domain, sn.slug, us.weighted_proven_count, us.last_proven_at
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE us.user_id = $1 AND us.proven_count > 0
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        let mut fragments: Vec<crate::models::SkillFragment> = rows
            .into_iter()
            .map(|(domain, slug, wpc, last)| crate::models::SkillFragment {
                id: uuid::Uuid::new_v4(),
                user_id,
                skill_domain: domain,
                sub_skill: slug,
                fragments: wpc,
                updated_at: last.unwrap_or_else(Utc::now),
            })
            .collect();

        // Appliquer l'ordering côté Rust puisqu'on n'a pas persisté
        match order {
            SkillFragmentOrder::ByDomainThenSubskill => fragments.sort_by(|a, b| {
                a.skill_domain
                    .cmp(&b.skill_domain)
                    .then_with(|| a.sub_skill.cmp(&b.sub_skill))
            }),
            SkillFragmentOrder::ByFragmentsDesc => {
                fragments.sort_by(|a, b| b.fragments.cmp(&a.fragments))
            }
            SkillFragmentOrder::ByDomainThenFragmentsDesc => fragments.sort_by(|a, b| {
                a.skill_domain
                    .cmp(&b.skill_domain)
                    .then_with(|| b.fragments.cmp(&a.fragments))
            }),
        }

        Ok(fragments)
    }

    /// Top N skills d'un user au format tuple `(skill_domain, sub_skill, fragments)`.
    ///
    /// Utilisé par les endpoints talent_search + github qui affichent un aperçu
    /// compact du profil skills. Réutilise `list_user_skill_fragments_or_backfill`
    /// avec l'ordering `ByFragmentsDesc` puis coupe à `limit`.
    pub async fn list_user_top_skills(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        limit: usize,
    ) -> Result<Vec<(String, String, i32)>, AppError> {
        let fragments = Self::list_user_skill_fragments_or_backfill(
            db,
            user_id,
            SkillFragmentOrder::ByFragmentsDesc,
        )
        .await?;
        Ok(fragments
            .into_iter()
            .take(limit)
            .map(|f| (f.skill_domain, f.sub_skill, f.fragments))
            .collect())
    }

    // ═══════════════════════════════════════════════════════════════════
    // P8.5c : propagation legacy challenge → user_skills (best-effort)
    // ═══════════════════════════════════════════════════════════════════

    /// Best-effort : au succès d'un challenge legacy, tente de propager le proof
    /// vers `user_skills` en résolvant le `skill_id` depuis un slug matchant :
    ///   1. `language` (ex: "python", "rust", "typescript") — matche une catégorie
    ///      du skill graph seedé en 0057.
    ///   2. À défaut, `slug_hint` optionnel (ex: challenge tag futur).
    ///
    /// Si aucun match → skip silencieusement (log debug). Le `deliverable`
    /// créé en P8.5a reste, mais sans propagation user_skills : c'est le
    /// comportement acceptable retenu (option 3 du plan P8).
    ///
    /// Retourne `Some(skill_id)` si propagation faite, `None` sinon.
    pub async fn propagate_legacy_challenge_success_to_user_skills(
        db: &PgPool,
        user_id: Uuid,
        language: Option<&str>,
        skill_domain: &str,
        weight: i32,
    ) -> Result<Option<Uuid>, AppError> {
        if weight <= 0 {
            return Ok(None);
        }

        let mut skill_id: Option<Uuid> = None;
        if let Some(lang) = language {
            let lower = lang.to_lowercase();
            skill_id = sqlx::query_scalar(
                "SELECT id FROM skill_nodes WHERE slug = $1 LIMIT 1",
            )
            .bind(&lower)
            .fetch_optional(db)
            .await?;
        }

        let Some(skill_id) = skill_id else {
            tracing::debug!(
                user_id = %user_id, ?language, skill_domain,
                "P8.5c skip user_skills propagation (no skill_node slug match)"
            );
            return Ok(None);
        };

        sqlx::query(
            r#"
            INSERT INTO user_skills (
                user_id, skill_id, proven_count, weighted_proven_count,
                proficiency_level, first_proven_at, last_proven_at
            )
            VALUES ($1, $2, 1, $3, 1, NOW(), NOW())
            ON CONFLICT (user_id, skill_id) DO UPDATE SET
                proven_count = user_skills.proven_count + 1,
                weighted_proven_count = user_skills.weighted_proven_count + $3,
                last_proven_at = NOW(),
                first_proven_at = COALESCE(user_skills.first_proven_at, NOW())
            "#,
        )
        .bind(user_id)
        .bind(skill_id)
        .bind(weight)
        .execute(db)
        .await?;

        let wpc: i32 = sqlx::query_scalar(
            "SELECT weighted_proven_count FROM user_skills
             WHERE user_id = $1 AND skill_id = $2",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(db)
        .await?;

        let new_level = crate::models::UserSkill::proficiency_level_for(wpc);

        sqlx::query(
            "UPDATE user_skills SET proficiency_level = $1
             WHERE user_id = $2 AND skill_id = $3",
        )
        .bind(new_level)
        .bind(user_id)
        .bind(skill_id)
        .execute(db)
        .await?;

        Ok(Some(skill_id))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Consultation : profils et catalogue
    // ═══════════════════════════════════════════════════════════════════

    /// Liste toutes les skills (catégories + atomiques) filtrable par domaine.
    pub async fn list_skills(
        db: &PgPool,
        domain: Option<&str>,
    ) -> Result<Vec<crate::models::SkillNode>, AppError> {
        let skills = sqlx::query_as::<_, crate::models::SkillNode>(
            r#"
            SELECT * FROM skill_nodes
            WHERE ($1::text IS NULL OR domain = $1)
            ORDER BY domain, parent_id NULLS FIRST, slug
            "#,
        )
        .bind(domain)
        .fetch_all(db)
        .await?;
        Ok(skills)
    }

    /// Skill map du user, triée par proficiency puis récence.
    ///
    /// N'inclut que les skills où le user a au moins un proven_count > 0.
    /// Utilisé pour le profil public "voici ce que je sais faire".
    pub async fn list_user_skills(
        db: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<UserSkillEnriched>, AppError> {
        let rows: Vec<(Uuid, String, String, String, Option<Uuid>,
                       i32, i32, i16,
                       Option<DateTime<Utc>>, Option<DateTime<Utc>>,
                       Vec<Uuid>)> = sqlx::query_as(
            r#"
            SELECT
                us.skill_id,
                sn.slug,
                sn.display_name,
                sn.domain,
                sn.parent_id,
                us.proven_count,
                us.weighted_proven_count,
                us.proficiency_level,
                us.first_proven_at,
                us.last_proven_at,
                us.top_proof_deliverable_ids
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE us.user_id = $1
              AND us.proven_count > 0
            ORDER BY us.proficiency_level DESC, us.last_proven_at DESC NULLS LAST
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(skill_id, slug, display_name, domain, parent_id,
                  proven_count, wpc, level, first, last, top)| {
                    UserSkillEnriched {
                        skill_id,
                        skill_slug: slug,
                        skill_display_name: display_name,
                        skill_domain: domain,
                        skill_parent_id: parent_id,
                        proven_count,
                        weighted_proven_count: wpc,
                        proficiency_level: level,
                        first_proven_at: first,
                        last_proven_at: last,
                        top_proof_deliverable_ids: top,
                    }
                },
            )
            .collect())
    }

    // ═══════════════════════════════════════════════════════════════════
    // Recherche recruteur : talents par skill
    // ═══════════════════════════════════════════════════════════════════

    /// Trouve les talents qui maîtrisent un skill donné à un niveau minimum.
    ///
    /// Trié par proficiency DESC, puis wpc DESC (le plus prouvé d'abord).
    /// Ne retourne que les users avec `profile_active = TRUE` — les profils
    /// non activés ne sont pas exposés aux recruteurs.
    pub async fn find_talents_by_skill(
        db: &PgPool,
        skill_slug: &str,
        filter: &TalentSearchFilter,
    ) -> Result<(Vec<SkillTalent>, i64), AppError> {
        let min_level = filter.min_level.clamp(1, 5);
        let per_page = filter.per_page.clamp(1, 100);
        let page = filter.page.max(1);
        let offset = (page - 1) * per_page;

        // Resolve skill_id from slug
        let skill_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM skill_nodes WHERE slug = $1",
        )
        .bind(skill_slug)
        .fetch_optional(db)
        .await?;

        let Some(skill_id) = skill_id else {
            return Err(AppError::NotFound(format!(
                "Skill '{skill_slug}' not found"
            )));
        };

        let talents: Vec<SkillTalent> = sqlx::query_as::<_, (Uuid, String, String, i16, i32, i32, Option<DateTime<Utc>>)>(
            r#"
            SELECT
                u.id,
                u.username,
                u.display_name,
                us.proficiency_level,
                us.proven_count,
                us.weighted_proven_count,
                us.last_proven_at
            FROM user_skills us
            JOIN users u ON u.id = us.user_id
            WHERE us.skill_id = $1
              AND us.proficiency_level >= $2
              AND u.profile_active = TRUE
            ORDER BY us.proficiency_level DESC, us.weighted_proven_count DESC,
                     us.last_proven_at DESC NULLS LAST
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(skill_id)
        .bind(min_level)
        .bind(per_page)
        .bind(offset)
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|(user_id, username, display_name, level, count, wpc, last)| SkillTalent {
            user_id,
            username,
            display_name,
            proficiency_level: level,
            proven_count: count,
            weighted_proven_count: wpc,
            last_proven_at: last,
        })
        .collect();

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM user_skills us
            JOIN users u ON u.id = us.user_id
            WHERE us.skill_id = $1
              AND us.proficiency_level >= $2
              AND u.profile_active = TRUE
            "#,
        )
        .bind(skill_id)
        .bind(min_level)
        .fetch_one(db)
        .await?;

        Ok((talents, total))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Recommandations de slices basées sur les skills près d'un level-up
    // ═══════════════════════════════════════════════════════════════════

    /// Retourne le seuil WPC pour atteindre le niveau supérieur.
    ///
    /// Formule inverse de `UserSkill::proficiency_level_for` :
    ///   level 1 → WPC 3 pour passer à 2
    ///   level 2 → WPC 7 pour passer à 3
    ///   level 3 → WPC 15 pour passer à 4
    ///   level 4 → WPC 31 pour passer à 5
    ///   level 5 → déjà max, retourne None
    fn wpc_threshold_for_next_level(current_level: i16) -> Option<i32> {
        match current_level {
            1 => Some(3),
            2 => Some(7),
            3 => Some(15),
            4 => Some(31),
            _ => None,
        }
    }

    /// Un skill est "proche d'un level-up" si son WPC est à ≤ 3 points de la
    /// prochaine threshold. Utilisé pour prioriser les recommandations sur des
    /// skills que le user peut débloquer avec ~1-2 slices supplémentaires.
    const RECOMMENDATION_WPC_WINDOW: i32 = 3;

    /// Recommande des slices ouvertes pour un user, priorisées par les skills
    /// où il est proche d'un level-up.
    ///
    /// Algorithme :
    /// 1. Identifier les skills du user proches d'un level-up
    ///    (`level < 5` AND `WPC` dans [next_threshold - 3, next_threshold - 1])
    /// 2. Chercher les slices status='open' avec au moins un slice_skills sur
    ///    ces skills
    /// 3. Scorer chaque slice par la somme des weights sur les skills matchés
    /// 4. Trier par score DESC, retourner top N
    pub async fn recommend_slices_for_user(
        db: &PgPool,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<SliceRecommendation>, AppError> {
        let limit = limit.clamp(1, 50);

        // 1. Skills du user proches d'un level-up
        let user_skills = Self::list_user_skills(db, user_id).await?;

        let near_levelup: Vec<(Uuid, String, String, i32, i16, i32)> = user_skills
            .into_iter()
            .filter_map(|us| {
                let threshold = Self::wpc_threshold_for_next_level(us.proficiency_level)?;
                let gap = threshold - us.weighted_proven_count;
                if gap > 0 && gap <= Self::RECOMMENDATION_WPC_WINDOW {
                    Some((
                        us.skill_id,
                        us.skill_slug,
                        us.skill_display_name,
                        us.weighted_proven_count,
                        us.proficiency_level,
                        threshold,
                    ))
                } else {
                    None
                }
            })
            .collect();

        if near_levelup.is_empty() {
            return Ok(Vec::new());
        }

        let near_skill_ids: Vec<Uuid> = near_levelup.iter().map(|(id, ..)| *id).collect();

        // 2. Slices ouvertes touchant au moins un de ces skills
        let candidate_slices: Vec<(Uuid, String, String, i16, Uuid, String, Uuid, i16)> =
            sqlx::query_as(
                r#"
                SELECT
                    ps.id,
                    ps.title,
                    ps.primary_domain,
                    ps.difficulty,
                    ps.project_id,
                    p.name,
                    ss.skill_id,
                    ss.weight
                FROM project_slices ps
                JOIN slice_skills ss ON ss.slice_id = ps.id
                JOIN projects p ON p.id = ps.project_id
                WHERE ps.status = 'open'
                  AND ss.skill_id = ANY($1)
                "#,
            )
            .bind(&near_skill_ids)
            .fetch_all(db)
            .await?;

        // 3. Aggréger par slice
        use std::collections::HashMap;
        let mut by_slice: HashMap<Uuid, SliceRecommendation> = HashMap::new();

        for (slice_id, title, domain, difficulty, project_id, project_name, skill_id, weight) in
            candidate_slices
        {
            let matched_info = near_levelup
                .iter()
                .find(|(sid, ..)| *sid == skill_id)
                .cloned();
            let Some((_, slug, display, wpc, level, threshold)) = matched_info else {
                continue;
            };

            let entry = by_slice.entry(slice_id).or_insert(SliceRecommendation {
                slice_id,
                slice_title: title,
                slice_primary_domain: domain,
                slice_difficulty: difficulty,
                project_id,
                project_name,
                matched_skills: Vec::new(),
                total_match_score: 0,
            });

            entry.matched_skills.push(RecommendationSkillMatch {
                skill_id,
                skill_slug: slug,
                skill_display_name: display,
                current_wpc: wpc,
                current_level: level,
                next_level_wpc_threshold: threshold,
                weight_in_slice: weight,
            });
            entry.total_match_score += weight as i32;
        }

        // 4. Trier par total_match_score DESC + limiter
        let mut recommendations: Vec<SliceRecommendation> = by_slice.into_values().collect();
        recommendations.sort_by(|a, b| b.total_match_score.cmp(&a.total_match_score));
        recommendations.truncate(limit as usize);

        Ok(recommendations)
    }

}
