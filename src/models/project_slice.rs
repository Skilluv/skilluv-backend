//! Unité de travail réelle sur un projet curated.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.4 pour le rationale
//! et partie G.1 pour le workflow "PR mergée → deliverable auto-vérifié".

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A slice = a scope of work claim-able exclusively.
///
/// Generalizes the pattern established by `oss_bounties` (see migration 0042). A
/// bounty is now just a slice with `credits_reward > 0`.
///
/// Workflow (P26 v2, decision 2026-08-06 — see migration 0119):
///
/// ```text
/// draft → open → claimed → in_progress → submitted → ci_green
///                                                        ↓
///                                                pending_validation
///                                                    ↙       ↘
///                                            validated       claimed (reject)
///                                                ↓
///                                            merged (bonus)
/// ```
///
/// **Challenge success status** = `validated` (fragments distributed +
/// attestation issued). The transition to `merged` is an independent **bonus**
/// (upstream maintainer merged the PR) that adds extra fragments but is NOT
/// required for the challenge to count as a success.
///
/// Terminal statuses: `merged`, `closed`, `expired`.
// SKI-111 — `ToSchema` so the admin slice endpoints can describe what they
// return instead of an empty object. `credits_reward` is a `BigDecimal`,
// which utoipa has no built-in mapping for; it serialises as a JSON number,
// so that is what the schema declares.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct ProjectSlice {
    pub id: Uuid,
    pub project_id: Uuid,

    pub slice_type: String,
    pub external_ref: Option<String>,
    pub external_metadata: Option<serde_json::Value>,

    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,

    pub primary_domain: String,
    pub difficulty: i16,
    pub estimated_hours: Option<i32>,
    pub fragments_reward: i32,
    #[schema(value_type = f64)]
    pub credits_reward: BigDecimal,

    pub status: String,
    pub claimed_by_user_id: Option<Uuid>,
    /// P10.1 : claim par une team persistente (XOR avec claimed_by_user_id).
    pub claimed_by_team_id: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,

    /// P26 v2 SKI-77: timestamp of the challenge success (Skilluv validation).
    /// Distinct from `merged_at` (upstream bonus).
    pub validated_at: Option<DateTime<Utc>>,
    /// P26 v2 SKI-77: the Skilluv validator who approved the PR.
    pub validated_by_user_id: Option<Uuid>,

    /// P26 v2 SKI-79: if non-empty, only users with an active user_orientation
    /// matching one of these slugs may claim. Empty = no restriction. Slug
    /// shape validated at insert time by the service (no FK, so orientation
    /// renames don't orphan slices).
    #[serde(default)]
    pub required_orientation_slugs: Vec<String>,

    /// P26 v2 SKI-78: minimum rank required to claim, or NULL for no floor.
    /// Ordinal comparison: apprenti < ranger < artisan < maitre < doyen.
    #[serde(default)]
    pub min_rank: Option<String>,

    /// P26 v2 SKI-75: HTML URL of the challenger's fork of the target repo,
    /// populated best-effort on claim when the user has connected GitHub.
    #[serde(default)]
    pub fork_repo_url: Option<String>,
    /// P26 v2 SKI-75: when the fork was recorded. Set/unset in lockstep
    /// with `fork_repo_url` (CHECK-enforced at the DB level).
    #[serde(default)]
    pub fork_created_at: Option<DateTime<Utc>>,

    /// P26 v2 SKI-76: the PR URL the challenger declared. Set on the
    /// `submitted` transition; drives the CI signal correlation later.
    #[serde(default)]
    pub submitted_pr_url: Option<String>,
    /// P26 v2 SKI-76: when the PR was declared. In lockstep with
    /// `submitted_pr_url` (CHECK-enforced).
    #[serde(default)]
    pub submitted_at: Option<DateTime<Utc>>,

    /// P26 v2 SKI-83: validator currently holding this slice for review.
    /// Exclusive: at most one validator per slice at a time.
    #[serde(default)]
    pub picked_by_validator_id: Option<Uuid>,
    /// P26 v2 SKI-83: when the validator picked the slice up. In lockstep
    /// with `picked_by_validator_id` (CHECK-enforced).
    #[serde(default)]
    pub picked_at: Option<DateTime<Utc>>,
    /// P26 v2 SKI-85: last rejection reason surfaced to the challenger.
    #[serde(default)]
    pub validation_reject_reason: Option<String>,

    /// P26 v2 SKI-90: 64-char lowercase hex SHA-256 hash bound to the
    /// approval moment. Stable identifier for the attestation.
    #[serde(default)]
    pub attestation_hash: Option<String>,

    /// P26 v2 SKI-119: when the challenger opted-in public announcement
    /// of the PR (comment posted). Idempotent: the second submit-pr call
    /// on the same slice does not re-post.
    #[serde(default)]
    pub announced_at: Option<DateTime<Utc>>,

    /// The trade this slice belongs to (migration 0186). What routes it to
    /// somebody competent to review it, through `orientations.reviewer_group`.
    #[serde(default)]
    pub orientation_id: Option<Uuid>,

    // ── Per-domain shape ────────────────────────────────────────────
    // Every domain adds the columns its artefacts need and leaves the others
    // NULL. They are surfaced here because a client that cannot read them
    // cannot render a design brief, a model card or a package listing — the
    // rows existed since migrations 0214, 0181 and 0231 and nothing exposed
    // them.
    /// Code: what the finished artefact is (migration 0181).
    #[serde(default)]
    pub code_subtype: Option<String>,
    /// AI: what the finished artefact is (migration 0214).
    #[serde(default)]
    pub ai_subtype: Option<String>,
    /// AI: where the weights, dataset or paper actually live.
    #[serde(default)]
    pub ai_external_hosting_url: Option<String>,

    /// Design: what the finished artefact is (migration 0231).
    /// See [`DesignSubtype`].
    #[serde(default)]
    pub design_subtype: Option<String>,
    /// Design: where the current version lives — a Figma node, a hosted
    /// board, a published project, or a stored object.
    #[serde(default)]
    pub design_external_url: Option<String>,
    /// Design: what the author says changed since the previous version.
    /// Copied into the decision row when somebody reviews it.
    #[serde(default)]
    pub design_version_notes_md: Option<String>,
    /// Design: every tool the slice touches.
    #[serde(default)]
    pub design_tools: Vec<String>,
    /// Design: how many critique rounds the brief announces. The hard ceiling
    /// is five, and it is enforced on the decision journal, not here.
    #[serde(default)]
    pub design_expected_rounds: Option<i16>,

    pub created_by_user_id: Option<Uuid>,
    pub ingested_from: String,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Types de slice (miroir du CHECK constraint SQL).
///
/// Détermine comment l'artefact est produit et vérifié (webhook GitHub pour
/// `GithubIssue`, review humaine pour les autres).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SliceType {
    GithubIssue,
    GameLevel,
    GameAsset,
    SecTarget,
    CliTask,
    Documentation,
    CodeArtifact,
    AiArtifact,
    DesignArtifact,
    Other,
}

impl SliceType {
    /// The exact string stored in `project_slices.slice_type`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GithubIssue => "github_issue",
            Self::GameLevel => "game_level",
            Self::GameAsset => "game_asset",
            Self::SecTarget => "sec_target",
            Self::CliTask => "cli_task",
            Self::Documentation => "documentation",
            Self::CodeArtifact => "code_artifact",
            Self::AiArtifact => "ai_artifact",
            Self::DesignArtifact => "design_artifact",
            Self::Other => "other",
        }
    }

    /// Every value the SQL CHECK accepts, in the order it lists them.
    pub const ALL: &'static [SliceType] = &[
        Self::GithubIssue,
        Self::GameLevel,
        Self::GameAsset,
        Self::SecTarget,
        Self::CliTask,
        Self::Documentation,
        Self::CodeArtifact,
        Self::AiArtifact,
        Self::DesignArtifact,
        Self::Other,
    ];
}

/// What a design challenge is expected to produce, mirroring the CHECK on
/// `project_slices.design_subtype` (migration 0231).
///
/// The subtype is what lets one workflow serve twenty-six very different
/// trades: it decides which preview is worth generating, which automatic
/// checks apply, and how large the artefact is allowed to be.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesignSubtype {
    /// Screens, flows, a prototype.
    Interface,
    /// Tokens, components, their documentation.
    DesignSystem,
    /// Marks, palette, type, guidelines.
    BrandKit,
    /// A set of images and their sources.
    IllustrationSet,
    /// An icon system and its delivery formats.
    IconSet,
    /// A motion project and its rendered preview.
    Motion,
    /// A rendered video and its storyboard.
    Video,
    /// A scene, its renders, optionally a glTF.
    ThreeDScene,
    /// Audio and its metadata.
    Sound,
    /// A typeface and its production files.
    TypeFamily,
    /// UX writing, naming, verbal guidelines.
    CopyDeck,
    /// A blueprint, a journey map, an audit, a style guide.
    ResearchDocument,
}

impl DesignSubtype {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Interface => "interface",
            Self::DesignSystem => "design_system",
            Self::BrandKit => "brand_kit",
            Self::IllustrationSet => "illustration_set",
            Self::IconSet => "icon_set",
            Self::Motion => "motion",
            Self::Video => "video",
            Self::ThreeDScene => "three_d_scene",
            Self::Sound => "sound",
            Self::TypeFamily => "type_family",
            Self::CopyDeck => "copy_deck",
            Self::ResearchDocument => "research_document",
        }
    }

    pub const ALL: &'static [DesignSubtype] = &[
        Self::Interface,
        Self::DesignSystem,
        Self::BrandKit,
        Self::IllustrationSet,
        Self::IconSet,
        Self::Motion,
        Self::Video,
        Self::ThreeDScene,
        Self::Sound,
        Self::TypeFamily,
        Self::CopyDeck,
        Self::ResearchDocument,
    ];
}

#[cfg(test)]
mod enum_matches_sql {
    use super::*;

    /// Migration 0231 owns both CHECK constraints these enums mirror. Reading
    /// it at compile time is what keeps the Rust side from drifting: adding a
    /// value in SQL without the variant, or the reverse, fails here rather
    /// than at runtime on a production insert.
    const MIGRATION: &str = include_str!("../../migrations/0231_design_artifact_slices.sql");

    /// The quoted values of the `IN (...)` list that follows a marker. SQL
    /// line comments are stripped first: they contain parentheses that would
    /// otherwise close the list early.
    fn check_values(marker: &str) -> Vec<String> {
        // Normalised first: a checkout with CRLF endings must not turn a
        // schema-drift guard into a mysterious "marker not found".
        let migration = MIGRATION.replace("\r\n", "\n");
        let start = migration
            .find(marker)
            .unwrap_or_else(|| panic!("marker {marker} not found in migration 0231"));
        let uncommented: String = migration[start..]
            .lines()
            .map(|line| match line.find("--") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let open = uncommented.find("IN (").expect("no IN ( after marker") + 4;
        let mut depth = 1usize;
        let mut close = open;
        for (i, c) in uncommented[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(close > open, "unterminated IN ( after {marker}");

        let mut out = Vec::new();
        let mut rest = &uncommented[open..close];
        while let Some(a) = rest.find('\'') {
            let after = &rest[a + 1..];
            let Some(b) = after.find('\'') else { break };
            out.push(after[..b].to_string());
            rest = &after[b + 1..];
        }
        out
    }

    #[test]
    fn slice_type_variants_match_the_sql_check() {
        let sql = check_values("ADD CONSTRAINT project_slices_slice_type_check");
        let rust: Vec<String> = SliceType::ALL.iter().map(|t| t.as_str().into()).collect();
        assert_eq!(sql, rust, "slice_type drifted between SQL and Rust");
    }

    #[test]
    fn design_subtype_variants_match_the_sql_check() {
        let sql = check_values("ADD CONSTRAINT project_slices_design_subtype_values");
        let rust: Vec<String> = DesignSubtype::ALL.iter().map(|t| t.as_str().into()).collect();
        assert_eq!(sql, rust, "design_subtype drifted between SQL and Rust");
    }
}

/// Lien M2M slice ↔ skill avec poids d'exercice (1-5).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SliceSkill {
    pub slice_id: Uuid,
    pub skill_id: Uuid,
    /// Intensité de l'exercice sur ce skill par cette slice :
    /// 1 = effleuré, 3 = contribue clairement (défaut), 5 = cœur de la slice
    pub weight: i16,
    pub is_primary: bool,
}
