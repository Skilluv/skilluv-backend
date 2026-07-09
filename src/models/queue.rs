use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Queue Message (envelope) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    pub job_id: String,
    pub job_type: JobType,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub retry_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    PlagiarismCheck,
    TalentMatch,
    ReplayGenerate,
    ClipGenerate,
}

impl JobType {
    pub fn queue_name(&self) -> &'static str {
        match self {
            Self::PlagiarismCheck => "skilluv:queue:plagiarism",
            Self::TalentMatch => "skilluv:queue:matching",
            Self::ReplayGenerate | Self::ClipGenerate => "skilluv:queue:media",
        }
    }
}

// ── Payloads ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlagiarismPayload {
    pub submission_id: String,
    pub challenge_id: String,
    pub source_code: String,
    pub language: String,
    pub compare_with: Vec<PlagiarismSubmission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlagiarismSubmission {
    pub submission_id: String,
    pub user_id: String,
    pub source_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TalentMatchPayload {
    pub enterprise_id: String,
    pub criteria: MatchCriteria,
    pub candidates: Vec<TalentSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCriteria {
    #[serde(default)]
    pub skill_domains: Vec<String>,
    #[serde(default)]
    pub min_fragments: i32,
    pub min_title: Option<String>,
    pub country: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TalentSnapshot {
    pub user_id: String,
    pub username: String,
    pub skill_domains: Vec<String>,
    pub total_fragments: i32,
    pub title: String,
    pub country: Option<String>,
    #[serde(default)]
    pub top_languages: Vec<String>,
    #[serde(default)]
    pub trust_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayPayload {
    pub submission_id: String,
    pub challenge_id: String,
    pub user_id: String,
    pub events: Vec<serde_json::Value>,
    pub stats: SubmissionStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionStats {
    pub duration_seconds: i32,
    pub keystrokes: i32,
    pub tests_passed: i32,
    pub tests_total: i32,
    pub fragments_earned: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipPayload {
    pub submission_id: String,
    pub challenge_id: String,
    pub user_id: String,
    pub clip_type: String,
    pub replay_key: String,
    pub highlight_start_seconds: i32,
    #[serde(default = "default_clip_duration")]
    pub highlight_duration_seconds: i32,
}

fn default_clip_duration() -> i32 {
    30
}

// ── Job Results ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub status: JobStatus,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: i64,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlagiarismResult {
    pub submission_id: String,
    pub challenge_id: String,
    pub matches: Vec<PlagiarismMatch>,
    pub highest_score: f64,
    pub flagged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlagiarismMatch {
    pub compared_submission_id: String,
    pub compared_user_id: String,
    pub ast_similarity: f64,
    pub embedding_similarity: f64,
    pub combined_score: f64,
    pub is_plagiarism: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TalentMatchResult {
    pub enterprise_id: String,
    pub matched_talents: Vec<MatchedTalent>,
    pub total_candidates: i32,
    pub total_matched: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedTalent {
    pub user_id: String,
    pub username: String,
    pub relevance_score: f64,
    pub matching_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaResult {
    pub submission_id: String,
    pub media_type: String,
    pub minio_key: String,
    pub file_size_bytes: i64,
    pub duration_seconds: f64,
}

// ── Notification (Redis Pub/Sub) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobNotification {
    pub job_id: String,
    pub job_type: JobType,
    pub status: JobStatus,
}
