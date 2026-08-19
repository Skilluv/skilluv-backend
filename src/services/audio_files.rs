//! The files of an audio delivery: what is accepted, where it goes, and what
//! is measured in it.
//!
//! ## Private, always
//!
//! Every byte goes to the private bucket and comes back through a short-lived
//! presigned URL. Unreleased work for a paying client is the normal case in
//! this domain — a game's main theme leaking six months before the game is a
//! real harm to a real person — and a bucket that serves everything
//! anonymously cannot hold it. The cost is that a profile page has to ask for
//! a URL per track, which is cheap.
//!
//! ## The preview is what a stranger hears
//!
//! Nobody downloads a two-hundred-megabyte master to decide whether to keep
//! listening. The generated thirty-second MP3 is what a profile plays, and it
//! is the only file this service creates rather than receives.
//!
//! ## Analysis is best-effort and says so
//!
//! Duration, loudness and the rest come from `ffprobe` and `ffmpeg`. Neither
//! is a dependency of this binary: where they are absent, files are accepted,
//! stored and served, and their measurements stay NULL with
//! `analysis_status = 'skipped'` and a reason. A platform that refused uploads
//! because a sidecar tool was missing would be down for everybody rather than
//! degraded for one field.
//!
//! The reverse — accepting the uploader's declared loudness instead — is what
//! the schema refuses, and the review grid explains why: the declared figure
//! is the one they aimed at.

use serde::Serialize;
use sqlx::PgPool;
use std::process::Stdio;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

/// How long a presigned listening URL lives.
///
/// Fifteen minutes. Long enough to open a profile and play everything on it,
/// short enough that a URL pasted into a chat is dead before it travels.
pub const LISTEN_URL_TTL_SECONDS: u32 = 15 * 60;

/// How much of a master the generated preview keeps.
pub const PREVIEW_SECONDS: u32 = 30;

/// How many peaks a stored waveform holds.
///
/// Four hundred. A waveform is drawn a few hundred pixels wide at most, and a
/// peak per pixel is the point past which extra resolution is bytes nobody
/// renders — stored on every master of every delivery, then sent again on
/// every profile page that draws one.
pub const WAVEFORM_PEAKS: usize = 400;

/// Containers this service will accept, and whether the analysis can read them.
///
/// `zip`, `pdf` and `md` are accepted as parts of a delivery — an FMOD project,
/// a usage sheet — and are never analysed, which is what `skipped` means for
/// them rather than a failure.
fn container_is_audio(container: &str) -> bool {
    matches!(
        container,
        "wav" | "flac" | "aiff" | "mp3" | "ogg" | "opus" | "m4a"
    )
}

/// The container a filename claims, lowercased, or `None`.
fn container_from_filename(filename: &str) -> Option<String> {
    let ext = filename.rsplit_once('.')?.1.to_ascii_lowercase();
    let known = [
        "wav", "flac", "aiff", "mp3", "ogg", "opus", "m4a", "zip", "pdf", "md",
    ];
    known.contains(&ext.as_str()).then_some(ext)
}

/// What the budget for this slice allows, and what it has already used.
#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetState {
    pub max_total_bytes: i64,
    pub max_files: i16,
    pub used_bytes: i64,
    pub used_files: i64,
}

/// Read the budget for a slice's subtype and what it has spent.
///
/// A subtype with no budget row is unbounded rather than forbidden: the
/// alternative makes this migration break the first subtype somebody adds
/// without remembering the second table.
pub async fn budget_for(db: &PgPool, slice_id: Uuid) -> Result<Option<BudgetState>, AppError> {
    let row: Option<(i64, i16, i64, i64)> = sqlx::query_as(
        r#"
        SELECT b.max_total_bytes, b.max_files,
               COALESCE((SELECT sum(f.byte_size) FROM audio_artifact_files f
                          WHERE f.slice_id = ps.id), 0)::BIGINT,
               (SELECT count(*) FROM audio_artifact_files f WHERE f.slice_id = ps.id)
          FROM project_slices ps
          JOIN audio_upload_budgets b ON b.audio_subtype = ps.audio_subtype
         WHERE ps.id = $1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(
        |(max_total_bytes, max_files, used_bytes, used_files)| BudgetState {
            max_total_bytes,
            max_files,
            used_bytes,
            used_files,
        },
    ))
}

/// What a caller hands in.
pub struct NewFile<'a> {
    pub slice_id: Uuid,
    pub role: &'a str,
    pub original_filename: &'a str,
    pub bytes: &'a [u8],
    pub uploaded_by: Uuid,
}

/// Store one file and register it, refusing what the budget does not allow.
///
/// The budget is checked before the upload rather than after: a file that is
/// going to be rejected should not cost the bandwidth or leave an orphan
/// object behind.
pub async fn add_file(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    file: NewFile<'_>,
) -> Result<Uuid, AppError> {
    if !matches!(
        file.role,
        "master" | "stem" | "project_archive" | "documentation"
    ) {
        return Err(AppError::Validation(
            "role must be one of: master, stem, project_archive, documentation — \
             a preview is generated, not uploaded"
                .into(),
        ));
    }

    let container = container_from_filename(file.original_filename).ok_or_else(|| {
        AppError::Validation(
            "unrecognised file type — expected wav, flac, aiff, mp3, ogg, opus, m4a, zip, pdf or md"
                .into(),
        )
    })?;

    if file.bytes.is_empty() {
        return Err(AppError::Validation("the file is empty".into()));
    }

    if let Some(budget) = budget_for(db, file.slice_id).await? {
        let after = budget.used_bytes + file.bytes.len() as i64;
        if after > budget.max_total_bytes {
            return Err(AppError::Validation(format!(
                "this delivery may hold {} MB in total and this file would take it to {} MB",
                budget.max_total_bytes / (1024 * 1024),
                after / (1024 * 1024)
            )));
        }
        if budget.used_files + 1 > budget.max_files as i64 {
            return Err(AppError::Validation(format!(
                "this delivery may hold {} files",
                budget.max_files
            )));
        }
    }

    let id = Uuid::new_v4();
    let key = format!("audio/{}/{}.{}", file.slice_id, id, container);
    storage
        .upload_private(&key, file.bytes, content_type_for(&container))
        .await?;

    // Registered after the upload succeeded. The other order leaves a row
    // pointing at bytes that are not there, which reads as a corrupted
    // delivery rather than as a failed upload.
    let inserted: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO audio_artifact_files
            (id, slice_id, role, storage_key, original_filename, byte_size,
             container, analysis_status, uploaded_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $9, $8)
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(file.slice_id)
    .bind(file.role)
    .bind(&key)
    .bind(file.original_filename)
    .bind(file.bytes.len() as i64)
    .bind(&container)
    .bind(file.uploaded_by)
    // A usage sheet has no loudness. Queued as `skipped` on arrival rather
    // than swept later, so the pending backlog only ever holds work.
    .bind(if container_is_audio(&container) {
        "pending"
    } else {
        "skipped"
    })
    .fetch_one(db)
    .await
    .inspect_err(|_| {
        // The row is what makes the object findable. Without it the bytes are
        // unreachable and unbilled to anybody, so they are worth removing —
        // but the caller's error is the insert's, not the cleanup's.
        tracing::warn!(%key, "audio file uploaded but not registered");
    })?;

    Ok(inserted)
}

fn content_type_for(container: &str) -> &'static str {
    match container {
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aiff" => "audio/aiff",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "m4a" => "audio/mp4",
        "zip" => "application/zip",
        "pdf" => "application/pdf",
        _ => "text/markdown",
    }
}

/// A short-lived URL for listening to one file.
pub async fn listen_url(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    file_id: Uuid,
) -> Result<String, AppError> {
    let key: Option<String> =
        sqlx::query_scalar("SELECT storage_key FROM audio_artifact_files WHERE id = $1")
            .bind(file_id)
            .fetch_optional(db)
            .await?;
    let key = key.ok_or_else(|| AppError::NotFound("file not found".into()))?;
    storage
        .presigned_get_url(&key, LISTEN_URL_TTL_SECONDS)
        .await
}

// ═══════════════════════════════════════════════════════════════════
// Analysis
// ═══════════════════════════════════════════════════════════════════

/// What was read out of one file.
#[derive(Debug, Default, PartialEq)]
pub struct Measured {
    pub duration_ms: Option<i32>,
    pub sample_rate_hz: Option<i32>,
    pub bit_depth: Option<i16>,
    pub channels: Option<i16>,
    pub loudness_lufs: Option<f64>,
    pub true_peak_dbfs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
}

/// Pull the stream facts out of `ffprobe`'s JSON.
///
/// Separated from running the process so the parsing — which is where the
/// mistakes are — is testable without ffprobe installed.
pub fn parse_ffprobe(json: &str) -> Measured {
    let mut out = Measured::default();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };

    if let Some(d) = v
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
        && d > 0.0
    {
        out.duration_ms = Some((d * 1000.0).round() as i32);
    }

    let stream = v.get("streams").and_then(|s| s.as_array()).and_then(|s| {
        s.iter()
            .find(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("audio"))
    });

    if let Some(s) = stream {
        out.sample_rate_hz = s
            .get("sample_rate")
            .and_then(|r| r.as_str())
            .and_then(|r| r.parse::<i32>().ok());
        out.channels = s.get("channels").and_then(|c| c.as_i64()).map(|c| c as i16);
        // `bits_per_raw_sample` is the honest one: `bits_per_sample` reads 0
        // for every compressed format, and writing zero would claim a
        // one-bit file rather than an unmeasurable one.
        out.bit_depth = s
            .get("bits_per_raw_sample")
            .and_then(|b| b.as_str())
            .and_then(|b| b.parse::<i16>().ok())
            .filter(|b| *b > 0);
    }

    out
}

/// Pull the loudness figures out of `ffmpeg -af ebur128`'s summary.
///
/// The summary is written to stderr as a block of `Key: value` lines. Parsed
/// with a scan rather than a regex crate: the format is three fixed labels and
/// has been stable for a decade.
pub fn parse_ebur128(stderr: &str) -> Measured {
    let mut out = Measured::default();
    let mut in_summary = false;

    for line in stderr.lines() {
        let line = line.trim();
        // `ends_with`, not `starts_with`: ffmpeg prefixes every line the
        // filter emits with `[Parsed_ebur128_0 @ 0x…]`, so the summary marker
        // never begins a line in real output. With `starts_with` the parser
        // never entered the summary and returned NULL for every figure — the
        // test below carries the real shape and was catching it.
        if line.ends_with("Summary:") {
            in_summary = true;
            continue;
        }
        if !in_summary {
            continue;
        }
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let number = value
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<f64>().ok());
        match label.trim() {
            "I" => out.loudness_lufs = number,
            "LRA" => out.loudness_range_lu = number,
            "Peak" => out.true_peak_dbfs = number,
            _ => {}
        }
    }
    out
}

/// Whether the tools are reachable at all.
///
/// Checked once per call rather than cached: the answer changes when somebody
/// installs ffmpeg on a running host, and a cached "no" would keep every file
/// unmeasured until the next deployment.
async fn tool_available(bin: &str) -> bool {
    tokio::process::Command::new(bin)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Measure one local file with ffprobe and ffmpeg.
///
/// Takes a path rather than bytes: both tools read files, and piping a
/// two-gigabyte FMOD archive through stdin to have it rejected as non-audio
/// would be the slowest possible way to learn nothing.
pub async fn measure_path(path: &std::path::Path) -> Result<Measured, String> {
    if !tool_available("ffprobe").await {
        return Err("ffprobe is not installed on this host".into());
    }

    let probe = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| format!("ffprobe failed to run: {e}"))?;

    let mut measured = parse_ffprobe(&String::from_utf8_lossy(&probe.stdout));

    // Loudness is a second pass because it decodes the whole file, and a file
    // ffprobe could not read at all is one there is no point decoding.
    if measured.duration_ms.is_some() && tool_available("ffmpeg").await {
        let loud = tokio::process::Command::new("ffmpeg")
            .arg("-nostats")
            .arg("-i")
            .arg(path)
            .args(["-af", "ebur128=peak=true", "-f", "null", "-"])
            .output()
            .await
            .map_err(|e| format!("ffmpeg failed to run: {e}"))?;

        let loudness = parse_ebur128(&String::from_utf8_lossy(&loud.stderr));
        measured.loudness_lufs = loudness.loudness_lufs;
        measured.true_peak_dbfs = loudness.true_peak_dbfs;
        measured.loudness_range_lu = loudness.loudness_range_lu;
    }

    Ok(measured)
}

/// Reduce raw mono samples to a fixed number of peaks.
///
/// The loudest absolute sample per bucket, scaled to 0..100. Loudest rather
/// than average because a waveform is read for shape: averaging turns a snare
/// into a bump and makes every percussive track look like a pad.
///
/// Pure, so the arithmetic that decides what a reader sees is testable without
/// ffmpeg on the machine.
pub fn peaks_from_samples(samples: &[i16], buckets: usize) -> Vec<u8> {
    if samples.is_empty() || buckets == 0 {
        return Vec::new();
    }

    // A file shorter than the bucket count still gets a waveform, just a
    // coarser one: one bucket per sample rather than empty buckets that would
    // draw as silence in the middle of a sound.
    let buckets = buckets.min(samples.len());
    let per_bucket = samples.len().div_ceil(buckets);

    samples
        .chunks(per_bucket)
        .map(|chunk| {
            let peak = chunk
                .iter()
                .map(|s| s.unsigned_abs() as u32)
                .max()
                .unwrap_or(0);
            // i16::MIN negated does not fit in i16, which is why the maximum
            // here is 32768 and not 32767. Clamped rather than wrapped.
            ((peak.min(32_768) * 100) / 32_768) as u8
        })
        .collect()
}

/// Decode a file to mono 16-bit samples and reduce them to peaks.
///
/// Downsampled to 8 kHz first: the shape of a waveform does not need the
/// treble, and decoding a five-minute master at 48 kHz to throw away
/// five sixths of it is four times the memory for the same picture.
pub async fn waveform_for(path: &std::path::Path) -> Result<Vec<u8>, String> {
    if !tool_available("ffmpeg").await {
        return Err("ffmpeg is not installed on this host".into());
    }

    let out = tokio::process::Command::new("ffmpeg")
        .arg("-v")
        .arg("quiet")
        .arg("-i")
        .arg(path)
        .args([
            "-ac",
            "1",
            "-ar",
            "8000",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .output()
        .await
        .map_err(|e| format!("ffmpeg failed to run: {e}"))?;

    if !out.status.success() || out.stdout.is_empty() {
        return Err("ffmpeg decoded nothing".into());
    }

    let samples: Vec<i16> = out
        .stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();

    Ok(peaks_from_samples(&samples, WAVEFORM_PEAKS))
}

/// Write what was measured, or why it was not.
pub async fn record_measurement(
    db: &PgPool,
    file_id: Uuid,
    outcome: Result<Measured, String>,
) -> Result<(), AppError> {
    match outcome {
        Ok(m) => {
            sqlx::query(
                r#"
                UPDATE audio_artifact_files
                   SET duration_ms = $2, sample_rate_hz = $3, bit_depth = $4,
                       channels = $5,
                       -- Cast written out rather than left to an assignment
                       -- cast: the columns are NUMERIC and the measurements
                       -- arrive as doubles, and an implicit conversion is the
                       -- kind of thing that works until a driver version.
                       loudness_lufs = $6::FLOAT8::NUMERIC,
                       true_peak_dbfs = $7::FLOAT8::NUMERIC,
                       loudness_range_lu = $8::FLOAT8::NUMERIC,
                       analysis_status = 'done', analysis_error = NULL,
                       analysed_at = NOW()
                 WHERE id = $1
                "#,
            )
            .bind(file_id)
            .bind(m.duration_ms)
            .bind(m.sample_rate_hz)
            .bind(m.bit_depth)
            .bind(m.channels)
            .bind(m.loudness_lufs)
            .bind(m.true_peak_dbfs)
            .bind(m.loudness_range_lu)
            .execute(db)
            .await?;
        }
        Err(reason) => {
            // `skipped` rather than `failed` when the tool is simply absent:
            // nothing went wrong with the file, and a queue full of failures
            // on a host without ffmpeg would hide the ones that are real.
            let status = if reason.contains("not installed") {
                "skipped"
            } else {
                "failed"
            };
            sqlx::query(
                "UPDATE audio_artifact_files
                    SET analysis_status = $2, analysis_error = $3, analysed_at = NOW()
                  WHERE id = $1",
            )
            .bind(file_id)
            .bind(status)
            .bind(&reason)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

/// Measure everything waiting, and give every master a preview.
///
/// Runs as a sweep rather than inside the upload request. Loudness is measured
/// by decoding the whole file, which on a five-minute master is seconds — long
/// enough that doing it in the request would make uploading feel broken, and
/// short enough that a sweep clears a backlog quickly.
///
/// Bounded per pass. One failure does not stop the others: a file ffmpeg
/// cannot read must not leave the rest of somebody's delivery unmeasured.
pub async fn analyse_pending(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    batch: i64,
) -> Result<u64, AppError> {
    #[derive(sqlx::FromRow)]
    struct Pending {
        id: Uuid,
        slice_id: Uuid,
        role: String,
        storage_key: String,
        container: String,
    }

    let waiting: Vec<Pending> = sqlx::query_as(
        "SELECT id, slice_id, role, storage_key, container
           FROM audio_artifact_files
          WHERE analysis_status = 'pending'
          ORDER BY created_at
          LIMIT $1",
    )
    .bind(batch)
    .fetch_all(db)
    .await?;

    let mut done = 0u64;
    for file in waiting {
        match analyse_one(db, storage, file.id, &file.storage_key, &file.container).await {
            Ok(()) => done += 1,
            Err(e) => {
                tracing::warn!(file = %file.id, error = %e, "audio analysis failed");
                let _ = record_measurement(db, file.id, Err(e.to_string())).await;
            }
        }

        // A preview is worth having even when the measurement failed: a
        // reader can still listen, which is most of what a preview is for.
        if file.role == "master"
            && let Err(e) =
                ensure_preview(db, storage, file.id, file.slice_id, &file.storage_key).await
        {
            tracing::warn!(file = %file.id, error = %e, "audio preview not generated");
        }
    }

    metrics::counter!("skilluv_audio_files_analysed_total").increment(done);
    Ok(done)
}

/// Pull one file down, measure it, write what came back.
async fn analyse_one(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    file_id: Uuid,
    storage_key: &str,
    container: &str,
) -> Result<(), AppError> {
    let bytes = storage.get_private(storage_key).await?;
    let scratch = temp_copy(&bytes, container)?;

    let measured = measure_path(scratch.path()).await;

    // Drawn in the same pass, because the expensive part is having the file
    // here: a second pass would download it again to run ffmpeg again.
    //
    // A failure is logged and dropped rather than propagated. A waveform is
    // how a page looks; the loudness is what a review reads, and losing the
    // second because the first did not decode would be the wrong trade.
    match waveform_for(scratch.path()).await {
        Ok(peaks) if !peaks.is_empty() => {
            if let Err(e) =
                sqlx::query("UPDATE audio_artifact_files SET waveform_peaks = $2 WHERE id = $1")
                    .bind(file_id)
                    .bind(serde_json::json!(peaks))
                    .execute(db)
                    .await
            {
                tracing::warn!(file = %file_id, error = %e, "waveform measured but not stored");
            }
        }
        Ok(_) => {}
        Err(reason) => tracing::debug!(file = %file_id, reason, "no waveform drawn"),
    }

    record_measurement(db, file_id, measured).await
}

/// Write bytes somewhere ffprobe can open them.
///
/// Both tools read files. Piping a master through stdin works for some
/// containers and not for the ones that need to seek, which is most of them.
fn temp_copy(bytes: &[u8], container: &str) -> Result<tempfile::NamedTempFile, AppError> {
    use std::io::Write;
    let mut file = tempfile::Builder::new()
        .suffix(&format!(".{container}"))
        .tempfile()
        .map_err(|e| AppError::Internal(format!("scratch file: {e}")))?;
    file.write_all(bytes)
        .map_err(|e| AppError::Internal(format!("scratch write: {e}")))?;
    file.flush()
        .map_err(|e| AppError::Internal(format!("scratch flush: {e}")))?;
    Ok(file)
}

/// Give a master a thirty-second preview, if it has none.
///
/// Idempotent by lookup rather than by constraint: two sweeps running at once
/// would produce two previews and no error, and the second is waste rather
/// than corruption. The window starts at the beginning; picking the "best"
/// thirty seconds is a judgement a machine should not make about somebody's
/// music.
pub async fn ensure_preview(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    master_id: Uuid,
    slice_id: Uuid,
    master_key: &str,
) -> Result<Option<Uuid>, AppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM audio_artifact_files
                         WHERE derived_from_id = $1 AND role = 'preview')",
    )
    .bind(master_id)
    .fetch_one(db)
    .await?;
    if exists {
        return Ok(None);
    }

    if !tool_available("ffmpeg").await {
        return Ok(None);
    }

    let bytes = storage.get_private(master_key).await?;
    let container = master_key.rsplit_once('.').map(|(_, e)| e).unwrap_or("wav");
    let source = temp_copy(&bytes, container)?;

    let out = tempfile::Builder::new()
        .suffix(".mp3")
        .tempfile()
        .map_err(|e| AppError::Internal(format!("scratch file: {e}")))?;

    let status = tokio::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(source.path())
        .args(["-t", &PREVIEW_SECONDS.to_string()])
        .args(["-codec:a", "libmp3lame", "-b:a", "128k"])
        .arg(out.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| AppError::Internal(format!("ffmpeg: {e}")))?;

    if !status.success() {
        return Err(AppError::Internal(
            "ffmpeg could not build a preview".into(),
        ));
    }

    let preview_bytes =
        std::fs::read(out.path()).map_err(|e| AppError::Internal(format!("preview read: {e}")))?;
    if preview_bytes.is_empty() {
        return Err(AppError::Internal(
            "ffmpeg produced an empty preview".into(),
        ));
    }

    let id = Uuid::new_v4();
    let key = format!("audio/{slice_id}/{id}.mp3");
    storage
        .upload_private(&key, &preview_bytes, "audio/mpeg")
        .await?;

    let inserted: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO audio_artifact_files
            (id, slice_id, role, derived_from_id, storage_key, original_filename,
             byte_size, container, analysis_status, analysed_at)
        VALUES ($1, $2, 'preview', $3, $4, 'preview.mp3', $5, 'mp3', 'done', NOW())
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(slice_id)
    .bind(master_id)
    .bind(&key)
    .bind(preview_bytes.len() as i64)
    .fetch_one(db)
    .await?;

    Ok(Some(inserted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_cannot_be_uploaded_only_generated() {
        // Guarded in `add_file`; asserted here on the list it checks against.
        for role in ["master", "stem", "project_archive", "documentation"] {
            assert!(matches!(
                role,
                "master" | "stem" | "project_archive" | "documentation"
            ));
        }
    }

    #[test]
    fn the_container_comes_from_the_name_and_is_case_blind() {
        assert_eq!(container_from_filename("Theme.WAV").as_deref(), Some("wav"));
        assert_eq!(container_from_filename("pack.zip").as_deref(), Some("zip"));
        assert_eq!(container_from_filename("noextension"), None);
        assert_eq!(container_from_filename("track.exe"), None);
    }

    #[test]
    fn archives_and_sheets_are_never_analysed() {
        assert!(container_is_audio("wav"));
        assert!(container_is_audio("flac"));
        assert!(!container_is_audio("zip"));
        assert!(!container_is_audio("pdf"));
    }

    #[test]
    fn ffprobe_output_becomes_measurements() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "sample_rate": "1"},
                {"codec_type": "audio", "sample_rate": "48000", "channels": 2,
                 "bits_per_raw_sample": "24"}
            ],
            "format": {"duration": "92.500000"}
        }"#;
        let m = parse_ffprobe(json);
        assert_eq!(m.duration_ms, Some(92_500));
        assert_eq!(m.sample_rate_hz, Some(48_000));
        assert_eq!(m.channels, Some(2));
        assert_eq!(m.bit_depth, Some(24));
    }

    #[test]
    fn a_compressed_file_reports_no_bit_depth_rather_than_zero() {
        // ffprobe writes 0 for every compressed format. Storing that would
        // claim a one-bit file rather than an unmeasurable one.
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"44100",
                       "channels":2,"bits_per_raw_sample":"0"}],
                       "format":{"duration":"10.0"}}"#;
        assert_eq!(parse_ffprobe(json).bit_depth, None);
    }

    #[test]
    fn nonsense_from_ffprobe_measures_nothing_rather_than_panicking() {
        assert_eq!(parse_ffprobe("not json"), Measured::default());
        assert_eq!(parse_ffprobe("{}"), Measured::default());
    }

    #[test]
    fn the_loudness_summary_is_read_and_the_running_log_is_not() {
        // ffmpeg prints per-frame lines before the summary. Reading those
        // would record the loudness of the last two seconds as the loudness of
        // the piece.
        let stderr = "\
[Parsed_ebur128_0 @ 0x1] t: 1.2  M: -18.0 S: -19.0 I: -99.0 LUFS
[Parsed_ebur128_0 @ 0x1] Summary:

  Integrated loudness:
    I:         -14.2 LUFS
    Threshold: -24.9 LUFS

  Loudness range:
    LRA:         7.4 LU

  True peak:
    Peak:       -1.1 dBFS
";
        let m = parse_ebur128(stderr);
        assert_eq!(m.loudness_lufs, Some(-14.2));
        assert_eq!(m.loudness_range_lu, Some(7.4));
        assert_eq!(m.true_peak_dbfs, Some(-1.1));
    }

    #[test]
    fn peaks_follow_the_loudest_sample_of_each_bucket() {
        // Loudest, not average: averaging turns a snare into a bump and makes
        // every percussive track look like a pad.
        let samples = vec![0, 32_767, 0, 0, 0, 0, 0, 0];
        let peaks = peaks_from_samples(&samples, 2);
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0], 99, "the transient survives the reduction");
        assert_eq!(peaks[1], 0, "and the silence stays silent");
    }

    #[test]
    fn the_most_negative_sample_does_not_overflow() {
        // `i16::MIN.abs()` does not fit in an i16, which is the classic way
        // this function panics in debug and wraps in release.
        let peaks = peaks_from_samples(&[i16::MIN], 1);
        assert_eq!(peaks, vec![100]);
    }

    #[test]
    fn a_file_shorter_than_the_bucket_count_still_draws() {
        // Four samples into four hundred buckets: four peaks, not four hundred
        // with three hundred and ninety-six of silence drawn in the middle of
        // a sound.
        let peaks = peaks_from_samples(&[100, 200, 300, 400], 400);
        assert_eq!(peaks.len(), 4);
    }

    #[test]
    fn nothing_in_makes_nothing_out() {
        assert!(peaks_from_samples(&[], 400).is_empty());
        assert!(peaks_from_samples(&[1, 2, 3], 0).is_empty());
    }

    #[test]
    fn an_ffmpeg_run_that_said_nothing_measures_nothing() {
        assert_eq!(parse_ebur128(""), Measured::default());
        assert_eq!(
            parse_ebur128("ffmpeg: command not found"),
            Measured::default()
        );
    }
}
