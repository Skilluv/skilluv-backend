//! Service `portfolio` — pont sortant vers le monde extérieur (Phase P7).
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections 8.6 et 9.6.
//!
//! Sert deux vecteurs de diffusion externe :
//! 1. **Portfolio JSON** au format schema.org Person + JSON-LD, exportable
//!    pour LinkedIn (import de compétences), CV automatique, ATS recruteurs.
//! 2. **Badge SVG** dynamique (style shields.io) intégrable dans un README
//!    GitHub. Vecteur de marketing organique gratuit.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub struct PortfolioService;

/// Snapshot public d'un user, agrégé pour les vues portfolio.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PublicUserSnapshot {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub first_name: String,
    pub last_name: String,
    pub title: Option<String>,
    pub total_fragments: i32,
    pub golden_stars: i32,
    pub streak_current: i32,
    pub profile_active: bool,
}

impl PortfolioService {
    /// Récupère le user public par username. Retourne 404 si profile_active=FALSE.
    pub async fn get_user_by_username(
        db: &PgPool,
        username: &str,
    ) -> Result<PublicUserSnapshot, AppError> {
        let snapshot = sqlx::query_as::<_, PublicUserSnapshot>(
            r#"
            SELECT id, username, display_name, first_name, last_name,
                   title, total_fragments, golden_stars, streak_current, profile_active
            FROM users
            WHERE username = $1
              AND profile_active = TRUE
            "#,
        )
        .bind(username)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User '{username}' not found or not public")))?;
        Ok(snapshot)
    }

    /// Agrège le portfolio JSON-LD complet pour un user.
    ///
    /// Format : schema.org Person avec extension "skilluv:" pour les artefacts
    /// spécifiques (attestations, tracks). Le résultat est directement importable
    /// dans LinkedIn Skills Assessments ou dans un ATS moderne.
    pub async fn build_portfolio_json(
        db: &PgPool,
        username: &str,
        base_url: &str,
    ) -> Result<Value, AppError> {
        let user = Self::get_user_by_username(db, username).await?;

        // Skills : top 20 par proficiency
        let skills: Vec<(String, String, String, i16)> = sqlx::query_as(
            r#"
            SELECT sn.slug, sn.display_name, sn.domain, us.proficiency_level
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE us.user_id = $1 AND us.proven_count > 0
            ORDER BY us.proficiency_level DESC, us.last_proven_at DESC NULLS LAST
            LIMIT 20
            "#,
        )
        .bind(user.id)
        .fetch_all(db)
        .await?;

        // Attestations publiques
        let attestations: Vec<(Uuid, String, String, String, String, chrono::DateTime<chrono::Utc>)> =
            sqlx::query_as(
                r#"
                SELECT id, attestation_type, title, description, verification_code, issued_at
                FROM attestations
                WHERE user_id = $1 AND public = TRUE AND revoked_at IS NULL
                ORDER BY issued_at DESC
                "#,
            )
            .bind(user.id)
            .fetch_all(db)
            .await?;

        // Deliverables publics vérifiés récents (top 20)
        let deliverables: Vec<(Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            r#"
            SELECT id, artifact_type, artifact_url, submitted_at
            FROM deliverables
            WHERE user_id = $1
              AND public = TRUE
              AND revoked_at IS NULL
              AND verification_status = 'verified'
            ORDER BY submitted_at DESC
            LIMIT 20
            "#,
        )
        .bind(user.id)
        .fetch_all(db)
        .await?;

        // Enrolled tracks
        let tracks: Vec<(String, String, String, Option<chrono::DateTime<chrono::Utc>>)> =
            sqlx::query_as(
                r#"
                SELECT t.slug, t.name, t.target_domain, ut.completed_at
                FROM user_tracks ut
                JOIN tracks t ON t.id = ut.track_id
                WHERE ut.user_id = $1
                ORDER BY ut.started_at DESC
                "#,
            )
            .bind(user.id)
            .fetch_all(db)
            .await?;

        let profile_url = format!("{base_url}/@{}", user.username);

        let skills_ld: Vec<Value> = skills
            .into_iter()
            .map(|(slug, display, domain, level)| {
                json!({
                    "@type": "DefinedTerm",
                    "identifier": slug,
                    "name": display,
                    "inDefinedTermSet": format!("skilluv:{domain}"),
                    "skilluv:proficiency_level": level,
                })
            })
            .collect();

        let attestations_ld: Vec<Value> = attestations
            .into_iter()
            .map(|(id, att_type, title, description, code, issued_at)| {
                json!({
                    "@type": "EducationalOccupationalCredential",
                    "identifier": id.to_string(),
                    "name": title,
                    "description": description,
                    "credentialCategory": att_type,
                    "recognizedBy": {
                        "@type": "Organization",
                        "name": "Skilluv",
                        "url": base_url,
                    },
                    "url": format!("{base_url}/attestations/verify/{code}"),
                    "dateCreated": issued_at.to_rfc3339(),
                })
            })
            .collect();

        let deliverables_ld: Vec<Value> = deliverables
            .into_iter()
            .map(|(id, artifact_type, url, submitted_at)| {
                json!({
                    "@type": "CreativeWork",
                    "identifier": id.to_string(),
                    "additionalType": artifact_type,
                    "url": url,
                    "dateCreated": submitted_at.to_rfc3339(),
                })
            })
            .collect();

        let tracks_ld: Vec<Value> = tracks
            .into_iter()
            .map(|(slug, name, domain, completed_at)| {
                json!({
                    "@type": "Course",
                    "identifier": slug,
                    "name": name,
                    "educationalLevel": domain,
                    "skilluv:completed": completed_at.is_some(),
                })
            })
            .collect();

        let portfolio = json!({
            "@context": {
                "@vocab": "https://schema.org/",
                "skilluv": format!("{base_url}/schema#"),
            },
            "@type": "Person",
            "identifier": user.id.to_string(),
            "alternateName": user.username,
            "givenName": user.first_name,
            "familyName": user.last_name,
            "name": user.display_name,
            "url": profile_url,
            "skilluv:title": user.title,
            "skilluv:total_fragments": user.total_fragments,
            "skilluv:golden_stars": user.golden_stars,
            "skilluv:streak_current": user.streak_current,
            "knowsAbout": skills_ld,
            "hasCredential": attestations_ld,
            "workExample": deliverables_ld,
            "alumniOf": tracks_ld,
        });

        Ok(portfolio)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Badge SVG dynamique (style shields.io)
    // ═══════════════════════════════════════════════════════════════════

    /// Génère un SVG badge dynamique pour un user.
    ///
    /// Format shields.io minimaliste : 2 sections (label + value) avec couleur
    /// dépendant du titre. Intégrable dans un README GitHub via :
    ///   ![Skilluv Badge](https://skilluv.com/api/users/{username}/badge.svg)
    pub async fn build_badge_svg(
        db: &PgPool,
        username: &str,
    ) -> Result<String, AppError> {
        let user = Self::get_user_by_username(db, username).await?;

        let title = user.title.as_deref().unwrap_or("apprenti");
        let title_display = match title {
            "apprenti" => "Apprenti",
            "artisan" => "Artisan",
            "maitre" => "Maître",
            "legende" => "Légende",
            _ => "Skilluv",
        };
        // Couleurs shields.io par titre — gradient de rareté
        let color = match title {
            "legende" => "#f39c12",  // gold
            "maitre" => "#8e44ad",   // purple
            "artisan" => "#3498db",  // blue
            _ => "#95a5a6",           // grey (apprenti)
        };

        let value = if user.golden_stars > 0 {
            format!("{title_display} ★{}", user.golden_stars)
        } else {
            format!("{title_display} · {} frags", user.total_fragments)
        };

        Ok(Self::render_badge_svg("Skilluv", &value, color))
    }

    /// Rendu minimaliste d'un badge à deux sections (style shields.io).
    ///
    /// Approximation en largeur : 6 pixels par caractère (ok pour glyphs
    /// latin standard). Le rendu final peut être imparfait pour de très
    /// longs textes, à raffiner en Phase P8 si besoin.
    fn render_badge_svg(label: &str, value: &str, value_color: &str) -> String {
        let label_width = (label.len() as u32 * 7).max(50);
        let value_width = (value.len() as u32 * 7).max(70);
        let total_width = label_width + value_width;

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="20" role="img" aria-label="{label}: {value}">
  <title>{label}: {value}</title>
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <clipPath id="r">
    <rect width="{total_width}" height="20" rx="3" fill="#fff"/>
  </clipPath>
  <g clip-path="url(#r)">
    <rect width="{label_width}" height="20" fill="#555"/>
    <rect x="{label_width}" width="{value_width}" height="20" fill="{value_color}"/>
    <rect width="{total_width}" height="20" fill="url(#s)"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11">
    <text aria-hidden="true" x="{label_x}" y="15" fill="#010101" fill-opacity=".3">{label}</text>
    <text x="{label_x}" y="14">{label}</text>
    <text aria-hidden="true" x="{value_x}" y="15" fill="#010101" fill-opacity=".3">{value}</text>
    <text x="{value_x}" y="14">{value}</text>
  </g>
</svg>"##,
            total_width = total_width,
            label_width = label_width,
            value_width = value_width,
            label_x = label_width / 2,
            value_x = label_width + value_width / 2,
            label = xml_escape(label),
            value = xml_escape(value),
            value_color = value_color,
        )
    }
}

/// Escape minimal des caractères XML pour l'injection dans un SVG.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
