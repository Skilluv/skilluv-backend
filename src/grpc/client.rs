use tonic::transport::Channel;

use super::proto::{
    AnalyzePerformanceRequest, AnalyzePerformanceResponse, CareerPathRequest, CareerPathResponse,
    CodeReviewRequest, CodeReviewResponse, GenerateChallengeRequest, GenerateChallengeResponse,
    PlagiarismRequest, PlagiarismResponse,
    challenge_generation_service_client::ChallengeGenerationServiceClient,
    code_review_service_client::CodeReviewServiceClient,
    plagiarism_service_client::PlagiarismServiceClient,
    talent_detection_service_client::TalentDetectionServiceClient,
};

/// gRPC client wrapper for the Skilluv AI service.
/// All methods are fire-and-forget safe (clone + tokio::spawn friendly).
#[derive(Clone)]
pub struct AiClient {
    code_review: CodeReviewServiceClient<Channel>,
    challenge_gen: ChallengeGenerationServiceClient<Channel>,
    talent: TalentDetectionServiceClient<Channel>,
    plagiarism: PlagiarismServiceClient<Channel>,
}

impl AiClient {
    /// Connect to the AI service. Returns None if connection fails.
    pub async fn connect(url: &str) -> Option<Self> {
        let channel = Channel::from_shared(url.to_string())
            .ok()?
            .connect()
            .await
            .ok()?;

        Some(Self {
            code_review: CodeReviewServiceClient::new(channel.clone()),
            challenge_gen: ChallengeGenerationServiceClient::new(channel.clone()),
            talent: TalentDetectionServiceClient::new(channel.clone()),
            plagiarism: PlagiarismServiceClient::new(channel),
        })
    }

    /// Review submitted code for quality, style, and correctness.
    pub async fn review_code(
        &self,
        submission_id: &str,
        code: &str,
        language: &str,
        challenge_title: &str,
        challenge_instructions: &str,
        difficulty: i32,
    ) -> Result<CodeReviewResponse, tonic::Status> {
        let mut client = self.code_review.clone();
        let response = client
            .review_code(CodeReviewRequest {
                submission_id: submission_id.to_string(),
                code: code.to_string(),
                language: language.to_string(),
                challenge_title: challenge_title.to_string(),
                challenge_instructions: challenge_instructions.to_string(),
                difficulty,
            })
            .await?;
        Ok(response.into_inner())
    }

    /// Generate a new challenge from a prompt.
    pub async fn generate_challenge(
        &self,
        prompt: &str,
        skill_domain: &str,
        difficulty: i32,
        language: &str,
        tags: Vec<String>,
        tone: &str,
    ) -> Result<GenerateChallengeResponse, tonic::Status> {
        let mut client = self.challenge_gen.clone();
        let response = client
            .generate_challenge(GenerateChallengeRequest {
                prompt: prompt.to_string(),
                skill_domain: skill_domain.to_string(),
                difficulty,
                language: language.to_string(),
                tags,
                tone: tone.to_string(),
            })
            .await?;
        Ok(response.into_inner())
    }

    /// Analyze a user's performance across submissions.
    pub async fn analyze_performance(
        &self,
        request: AnalyzePerformanceRequest,
    ) -> Result<AnalyzePerformanceResponse, tonic::Status> {
        let mut client = self.talent.clone();
        let response = client.analyze_performance(request).await?;
        Ok(response.into_inner())
    }

    /// Get career path suggestions based on skills.
    pub async fn suggest_career_path(
        &self,
        request: CareerPathRequest,
    ) -> Result<CareerPathResponse, tonic::Status> {
        let mut client = self.talent.clone();
        let response = client.suggest_career_path(request).await?;
        Ok(response.into_inner())
    }

    /// Check a submission for plagiarism.
    pub async fn check_plagiarism(
        &self,
        submission_id: &str,
        code: &str,
        language: &str,
        challenge_id: &str,
        user_id: &str,
    ) -> Result<PlagiarismResponse, tonic::Status> {
        let mut client = self.plagiarism.clone();
        let response = client
            .check_plagiarism(PlagiarismRequest {
                submission_id: submission_id.to_string(),
                code: code.to_string(),
                language: language.to_string(),
                challenge_id: challenge_id.to_string(),
                user_id: user_id.to_string(),
            })
            .await?;
        Ok(response.into_inner())
    }
}
