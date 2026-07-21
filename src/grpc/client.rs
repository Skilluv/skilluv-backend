//! IA-A.2 — Client gRPC pour skilluv-ai v2 (package `skilluv.ai.v2`).
//!
//! Le contrat est défini par `proto/skilluv_ai.proto` (byte-identical avec
//! `skilluv-ia/proto/skilluv_ai.proto`, voir `docs/BACKEND-INTEGRATION.md`).
//!
//! Décisions :
//!   - Channel partagé, client cheap-cloné à chaque appel (recommandation tonic).
//!   - Timeout channel = 60s (MVP.md §0.3, Opus 4.7 sur ReviewCode peut prendre 40s+).
//!   - Erreurs `tonic::Status` propagées telles quelles ; le caller décide du retry.
//!   - Pas de retry auto (route synchrone = laisse remonter, worker = retry externe).

use std::time::Duration;
use tonic::transport::Channel;

use super::proto::{
    // Message types
    AnalyzePerformanceRequest,
    AnalyzePerformanceResponse,
    CareerPathRequest,
    CareerPathResponse,
    CheckPlagiarismRequest,
    CheckPlagiarismResponse,
    CodeReviewRequest,
    CodeReviewResponse,
    GenerateChallengeRequest,
    GenerateChallengeResponse,
    GenerateVariantRequest,
    // Service clients
    challenge_generation_service_client::ChallengeGenerationServiceClient,
    code_review_service_client::CodeReviewServiceClient,
    plagiarism_service_client::PlagiarismServiceClient,
    talent_detection_service_client::TalentDetectionServiceClient,
};

/// Client wrapper autour du channel gRPC. Cloneable et fire-and-forget safe
/// (clone + `tokio::spawn` friendly grâce au channel partagé).
#[derive(Clone)]
pub struct AiClient {
    channel: Channel,
}

impl AiClient {
    /// Connect to the AI service. Returns `None` if URL parse or handshake fails —
    /// le backend continue de tourner sans IA (fallback documenté dans les callers).
    pub async fn connect(url: &str) -> Option<Self> {
        let channel = Channel::from_shared(url.to_string())
            .ok()?
            .timeout(Duration::from_secs(60))
            .connect()
            .await
            .ok()?;
        Some(Self { channel })
    }

    // ─── CodeReviewService ──────────────────────────────────────────

    /// Review submitted code for quality, style, correctness.
    /// Utilisé par `services::llm_verifier` (P15.2).
    pub async fn review_code(
        &self,
        submission_id: &str,
        code: &str,
        language: &str,
        challenge_title: &str,
        challenge_instructions: &str,
        difficulty: i32,
    ) -> Result<CodeReviewResponse, tonic::Status> {
        let mut client = CodeReviewServiceClient::new(self.channel.clone());
        let resp = client
            .review_code(CodeReviewRequest {
                submission_id: submission_id.to_string(),
                code: code.to_string(),
                language: language.to_string(),
                challenge_title: challenge_title.to_string(),
                challenge_instructions: challenge_instructions.to_string(),
                difficulty,
            })
            .await?;
        Ok(resp.into_inner())
    }

    // ─── ChallengeGenerationService ────────────────────────────────

    /// Generate a fresh challenge from parameterized inputs. Contrat v2 :
    /// les champs sont typés (pas de `prompt` free-form). Les champs post-MVP
    /// (`orientation_slug`, `is_training`, `project_id`) sont acceptés par
    /// l'IA mais ignorés côté prompt jusqu'à IA-M+2/M+3 (voir doc §10).
    pub async fn generate_challenge(
        &self,
        req: GenerateChallengeRequest,
    ) -> Result<GenerateChallengeResponse, tonic::Status> {
        let mut client = ChallengeGenerationServiceClient::new(self.channel.clone());
        Ok(client.generate_challenge(req).await?.into_inner())
    }

    /// IA-A.2 : Generate a variant of an existing challenge (harder/easier/…).
    /// L'IA est stateless — le backend fournit le `original` inline (voir §6.1).
    pub async fn generate_variant(
        &self,
        req: GenerateVariantRequest,
    ) -> Result<GenerateChallengeResponse, tonic::Status> {
        let mut client = ChallengeGenerationServiceClient::new(self.channel.clone());
        Ok(client.generate_variant(req).await?.into_inner())
    }

    // ─── TalentDetectionService ─────────────────────────────────────

    /// Analyze a user's performance across submissions. Le backend agrège les
    /// snapshots (deliverables + skills + orientations + rank), l'IA ajoute
    /// la sémantique.
    pub async fn analyze_performance(
        &self,
        req: AnalyzePerformanceRequest,
    ) -> Result<AnalyzePerformanceResponse, tonic::Status> {
        let mut client = TalentDetectionServiceClient::new(self.channel.clone());
        Ok(client.analyze_performance(req).await?.into_inner())
    }

    /// Suggest career paths (orientations métier P16) matching user skills.
    pub async fn suggest_career_path(
        &self,
        req: CareerPathRequest,
    ) -> Result<CareerPathResponse, tonic::Status> {
        let mut client = TalentDetectionServiceClient::new(self.channel.clone());
        Ok(client.suggest_career_path(req).await?.into_inner())
    }

    // ─── PlagiarismService ─────────────────────────────────────────

    /// Check a submission for plagiarism (AST + embeddings côté IA).
    /// Le backend fournit `comparison_pool` (tenant-scoped, ≤ 200 candidats).
    pub async fn check_plagiarism(
        &self,
        req: CheckPlagiarismRequest,
    ) -> Result<CheckPlagiarismResponse, tonic::Status> {
        let mut client = PlagiarismServiceClient::new(self.channel.clone());
        Ok(client.check_plagiarism(req).await?.into_inner())
    }
}
