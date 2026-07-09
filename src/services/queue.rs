use chrono::Utc;
use futures_util::StreamExt;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::models::queue::{
    ClipPayload, JobNotification, JobResult, JobType, PlagiarismPayload, QueueMessage,
    ReplayPayload, TalentMatchPayload,
};

const NOTIFICATIONS_CHANNEL: &str = "skilluv:notifications";
const RESULT_KEY_PREFIX: &str = "skilluv:result:";

/// Service for communicating with the Python AI workers via Redis queues.
#[derive(Clone)]
pub struct QueueService {
    redis: ConnectionManager,
    notifications_tx: broadcast::Sender<JobNotification>,
}

impl QueueService {
    pub fn new(redis: ConnectionManager) -> Self {
        let (notifications_tx, _) = broadcast::channel(256);
        Self {
            redis,
            notifications_tx,
        }
    }

    /// Subscribe to real-time job completion notifications.
    pub fn subscribe_notifications(&self) -> broadcast::Receiver<JobNotification> {
        self.notifications_tx.subscribe()
    }

    /// Start the background Redis Pub/Sub listener.
    /// Call this once at startup.
    pub fn start_listener(&self, redis_url: &str) {
        let tx = self.notifications_tx.clone();
        let url = redis_url.to_string();

        tokio::spawn(async move {
            if let Err(e) = run_subscriber(url, tx).await {
                tracing::error!("Redis Pub/Sub listener crashed: {e}");
            }
        });
    }

    // ── Push jobs ───────────────────────────────────────────────────

    /// Enqueue a plagiarism check job. Returns the job_id.
    pub async fn push_plagiarism(
        &self,
        payload: PlagiarismPayload,
    ) -> Result<String, QueueError> {
        self.push_job(JobType::PlagiarismCheck, &payload).await
    }

    /// Enqueue a talent matching job. Returns the job_id.
    pub async fn push_matching(
        &self,
        payload: TalentMatchPayload,
    ) -> Result<String, QueueError> {
        self.push_job(JobType::TalentMatch, &payload).await
    }

    /// Enqueue a replay generation job. Returns the job_id.
    pub async fn push_replay(
        &self,
        payload: ReplayPayload,
    ) -> Result<String, QueueError> {
        self.push_job(JobType::ReplayGenerate, &payload).await
    }

    /// Enqueue a clip generation job. Returns the job_id.
    pub async fn push_clip(
        &self,
        payload: ClipPayload,
    ) -> Result<String, QueueError> {
        self.push_job(JobType::ClipGenerate, &payload).await
    }

    // ── Retrieve results ────────────────────────────────────────────

    /// Get the result for a completed job. Returns None if not ready yet.
    pub async fn get_result(&self, job_id: &str) -> Result<Option<JobResult>, QueueError> {
        let key = format!("{RESULT_KEY_PREFIX}{job_id}");
        let raw: Option<String> = self.redis.clone().get(&key).await?;

        match raw {
            Some(json) => {
                let result: JobResult = serde_json::from_str(&json)?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    // ── Internal ────────────────────────────────────────────────────

    async fn push_job<P: serde::Serialize>(
        &self,
        job_type: JobType,
        payload: &P,
    ) -> Result<String, QueueError> {
        let job_id = Uuid::new_v4().to_string();
        let queue = job_type.queue_name();

        let message = QueueMessage {
            job_id: job_id.clone(),
            job_type,
            payload: serde_json::to_value(payload)?,
            created_at: Utc::now(),
            retry_count: 0,
        };

        let json = serde_json::to_string(&message)?;
        self.redis.clone().lpush::<_, _, ()>(queue, &json).await?;

        tracing::info!(job_id = %job_id, queue = %queue, "Job enqueued");
        Ok(job_id)
    }
}

/// Background task: listen to Redis Pub/Sub for job notifications.
async fn run_subscriber(
    redis_url: String,
    tx: broadcast::Sender<JobNotification>,
) -> Result<(), QueueError> {
    let client = Client::open(redis_url.as_str())?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(NOTIFICATIONS_CHANNEL).await?;

    tracing::info!("Redis Pub/Sub listener started on {NOTIFICATIONS_CHANNEL}");

    loop {
        let msg = pubsub.on_message().next().await;
        match msg {
            Some(msg) => {
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("Invalid Pub/Sub message: {e}");
                        continue;
                    }
                };

                match serde_json::from_str::<JobNotification>(&payload) {
                    Ok(notification) => {
                        tracing::debug!(job_id = %notification.job_id, "Received job notification");
                        let _ = tx.send(notification);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse notification: {e}");
                    }
                }
            }
            None => {
                tracing::warn!("Redis Pub/Sub stream ended, reconnecting...");
                break;
            }
        }
    }

    Ok(())
}

// ── Error type ──────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
