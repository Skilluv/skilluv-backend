//! GitHub OAuth + sync - Phase 2 Sprint 5.
//!
//! - OAuth tokens are stored encrypted at-rest using ChaCha20-Poly1305 with a key derived
//!   from `JWT_SECRET` via HMAC-SHA256 (so rotating JWT_SECRET also invalidates tokens - a
//!   feature, not a bug).
//! - Sync pulls public repos only (scope `read:user`, `public_repo`).

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, Generate, KeyInit},
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

const GITHUB_API: &str = "https://api.github.com";
const GITHUB_OAUTH: &str = "https://github.com/login/oauth";
pub const USER_AGENT: &str = "skilluv-backend";

// ─── Encryption ───────────────────────────────────────────────────

/// Derive a 32-byte symmetric key from the JWT secret. Domain-separated so that the same
/// JWT_SECRET cannot be misused as both signing key and encryption key.
fn derive_token_key(jwt_secret: &str) -> [u8; 32] {
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(jwt_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"skilluv-github-token-v1");
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

pub fn encrypt_token(jwt_secret: &str, token: &str) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let key_bytes = derive_token_key(jwt_secret);
    let key = Key::try_from(key_bytes.as_slice())
        .map_err(|_| AppError::Internal("github token key size invalid".into()))?;
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = Nonce::generate();
    let ciphertext = cipher
        .encrypt(&nonce, token.as_bytes())
        .map_err(|_| AppError::Internal("github token encryption failed".into()))?;
    Ok((ciphertext, nonce.to_vec()))
}

pub fn decrypt_token(
    jwt_secret: &str,
    ciphertext: &[u8],
    nonce_bytes: &[u8],
) -> Result<String, AppError> {
    if nonce_bytes.len() != 12 {
        return Err(AppError::Internal("invalid nonce length".into()));
    }
    let key_bytes = derive_token_key(jwt_secret);
    let key = Key::try_from(key_bytes.as_slice())
        .map_err(|_| AppError::Internal("github token key size invalid".into()))?;
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = Nonce::try_from(nonce_bytes)
        .map_err(|_| AppError::Internal("nonce parse failed".into()))?;
    let plain = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| AppError::Internal("github token decryption failed".into()))?;
    String::from_utf8(plain).map_err(|_| AppError::Internal("github token utf8 invalid".into()))
}

// ─── OAuth state ─────────────────────────────────────────────────

/// Build the authorization URL for the GitHub OAuth flow.
pub fn build_authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    let qs = format!(
        // `admin:repo_hook` is what lets the rite finish.
        //
        // A person opens their pull request from their own fork's `main` to
        // their own fork's `showcase`, so it lives entirely inside their
        // account. `skilluv-community` never sees it, and a webhook on the
        // upstream repository cannot fire for it - there is nothing for it to
        // fire on. The rite could not complete for anybody.
        //
        // So the fork gets its own webhook, created with the token this scope
        // buys, immediately after the fork exists. See
        // `create_fork_webhook` below.
        "client_id={}&redirect_uri={}&scope=read:user%20public_repo%20admin:repo_hook&state={}",
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(state)
    );
    format!("{GITHUB_OAUTH}/authorize?{qs}")
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
    pub name: Option<String>,
}

pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<(String, Option<String>), AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{GITHUB_OAUTH}/access_token"))
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github token exchange failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github token exchange status {}",
            resp.status()
        )));
    }
    let body: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github token decode failed: {e}")))?;
    if let Some(err) = body.error {
        return Err(AppError::Validation(format!(
            "github oauth error: {} - {}",
            err,
            body.error_description.unwrap_or_default()
        )));
    }
    let token = body
        .access_token
        .ok_or_else(|| AppError::Internal("github returned no access_token".into()))?;
    Ok((token, body.scope))
}

pub async fn fetch_user(access_token: &str) -> Result<GitHubUser, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{GITHUB_API}/user"))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github user fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github /user status {}",
            resp.status()
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("github /user decode failed: {e}")))
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    id: i64,
    name: String,
    full_name: String,
    description: Option<String>,
    html_url: String,
    homepage: Option<String>,
    language: Option<String>,
    stargazers_count: i32,
    forks_count: i32,
    open_issues_count: i32,
    archived: bool,
    fork: bool,
    pushed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

/// P26 v2 SKI-75 - fork `{owner}/{repo}` to the authenticated user's account.
/// Idempotent on GitHub's side: forking a repo the user already forked
/// returns the existing fork (HTTP 202). Returns the HTML URL of the fork.
pub async fn fork_repo_for_user(
    access_token: &str,
    owner: &str,
    repo: &str,
) -> Result<String, AppError> {
    #[derive(serde::Deserialize)]
    struct ForkResp {
        html_url: String,
    }
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/forks");
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        // Empty JSON body; GitHub accepts optional fork-org / default-branch
        // parameters but we deliberately take the defaults so an existing
        // fork on the user account is returned as-is.
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github fork call failed: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "github fork {owner}/{repo} returned {status}: {text}"
        )));
    }
    let parsed: ForkResp = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github fork decode failed: {e}")))?;
    Ok(parsed.html_url)
}

pub async fn fetch_public_repos(
    access_token: &str,
    github_login: &str,
) -> Result<Vec<GitHubRepo>, AppError> {
    let client = reqwest::Client::new();
    let mut all = Vec::new();
    for page in 1..=5 {
        // Cap at 5 pages (500 repos) to bound work per sync.
        let resp = client
            .get(format!(
                "{GITHUB_API}/users/{github_login}/repos?type=owner&sort=pushed&per_page=100&page={page}"
            ))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("github repos fetch failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "github /repos status {}",
                resp.status()
            )));
        }
        let batch: Vec<GitHubRepo> = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("github /repos decode failed: {e}")))?;
        let was_full = batch.len() == 100;
        all.extend(batch);
        if !was_full {
            break;
        }
    }
    Ok(all)
}

// ─── Fork API (Bonjour Skilluv onboarding) ───────────────────────

/// Result of forking a `skilluv-community/starter-*` template onto the user's
/// GitHub account. Used by the Bonjour Skilluv onboarding flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkResult {
    pub id: i64,
    pub full_name: String, // e.g. "amina/starter-fullstack-rust"
    pub html_url: String,  // e.g. "https://github.com/amina/starter-fullstack-rust"
    pub default_branch: String,
}

/// Metadata about a single file changed in a Pull Request.
#[derive(Debug, Clone, Deserialize)]
pub struct PrFile {
    pub filename: String,
    pub status: String, // "added" | "modified" | "removed" | "renamed"
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
}

/// List the files a fork changed on top of the upstream branch it came from.
///
/// Used by the Bonjour Skilluv webhook to decide what the person actually
/// touched. The pull request's own file list is the wrong question: the PR
/// goes `main` -> `showcase` on the fork, so its diff also carries every
/// upstream commit `showcase` had not caught up with when the fork was made.
/// On 2026-09-17 every starter's `showcase` was 2 to 8 commits behind `main`,
/// and a PR adding one line to HELLO.md listed twelve files.
///
/// A three-dot compare against the upstream starts from the merge base, so
/// it lists only the fork's own commits, whatever state either branch is in
/// and however far upstream `main` has moved since. GitHub caps the file list
/// at 300, far beyond anything an entrance PR should touch.
///
/// See https://docs.github.com/en/rest/commits/commits#compare-two-commits
pub async fn files_changed_since_upstream(
    access_token: &str,
    upstream_full_name: &str,
    upstream_ref: &str,
    fork_owner: &str,
    head_sha: &str,
) -> Result<Vec<PrFile>, AppError> {
    #[derive(Debug, Deserialize)]
    struct Comparison {
        #[serde(default)]
        files: Vec<PrFile>,
    }
    let resp = reqwest::Client::new()
        .get(format!(
            "{GITHUB_API}/repos/{upstream_full_name}/compare/{upstream_ref}...{fork_owner}:{head_sha}"
        ))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github compare failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github compare status {}",
            resp.status()
        )));
    }
    let comparison: Comparison = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github compare decode failed: {e}")))?;
    Ok(comparison.files)
}

/// Fetch the raw content of a file at a specific commit ref from GitHub.
///
/// Used to snapshot the HELLO.md content of a tracked Bonjour Skilluv PR
/// for archival in the Hello Wall repository.
///
/// Returns the plain-text content decoded from base64 (GitHub API default).
pub async fn fetch_file_content(
    access_token: &str,
    fork_full_name: &str,
    path: &str,
    git_ref: &str,
) -> Result<String, AppError> {
    #[derive(Debug, Deserialize)]
    struct ContentResponse {
        content: String,
        encoding: String,
    }
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{GITHUB_API}/repos/{fork_full_name}/contents/{path}?ref={git_ref}"
        ))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github content fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github content status {}",
            resp.status()
        )));
    }
    let body: ContentResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github content decode failed: {e}")))?;
    if body.encoding != "base64" {
        return Err(AppError::Internal(format!(
            "unexpected github content encoding: {}",
            body.encoding
        )));
    }
    // GitHub base64-encodes with line breaks; strip whitespace first.
    let cleaned: String = body
        .content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .map_err(|e| AppError::Internal(format!("github content base64 decode failed: {e}")))?;
    String::from_utf8(decoded)
        .map_err(|e| AppError::Internal(format!("github content utf8 decode failed: {e}")))
}

/// Create or update a file in a repository via GitHub API.
///
/// GitHub `PUT /repos/{owner}/{repo}/contents/{path}` semantics :
/// - Si le fichier n'existe pas -> le cree (avec le commit_message donne).
/// - Si le fichier existe -> il faut fournir le sha du blob actuel via `sha`
///   dans le body, sinon 409. On fetch d'abord pour recuperer le sha.
///
/// Utilise par le hello_wall_mirror pour publier `entries/{username}.md` sur
/// `skilluv-community/hello-wall` avec un service-account token
/// (`SKILLUV_BOT_GITHUB_TOKEN`).
pub async fn create_or_update_file(
    access_token: &str,
    repo_full_name: &str,
    path: &str,
    content_utf8: &str,
    commit_message: &str,
) -> Result<String, AppError> {
    use base64::Engine;

    // 1. Try to fetch existing file's sha (needed for update).
    let client = reqwest::Client::new();
    let existing_sha: Option<String> = {
        let resp = client
            .get(format!(
                "{GITHUB_API}/repos/{repo_full_name}/contents/{path}"
            ))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("github file fetch failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            None
        } else if resp.status().is_success() {
            #[derive(Deserialize)]
            struct ShaResponse {
                sha: String,
            }
            let body: ShaResponse = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("github file sha decode: {e}")))?;
            Some(body.sha)
        } else {
            return Err(AppError::Internal(format!(
                "github file fetch status {}",
                resp.status()
            )));
        }
    };

    // 2. PUT with base64-encoded content.
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(content_utf8.as_bytes());
    let mut body = serde_json::json!({
        "message": commit_message,
        "content": content_b64,
    });
    if let Some(sha) = existing_sha {
        body["sha"] = serde_json::Value::String(sha);
    }

    let resp = client
        .put(format!(
            "{GITHUB_API}/repos/{repo_full_name}/contents/{path}"
        ))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github file put failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "github file put status {status}: {body}"
        )));
    }

    #[derive(Deserialize)]
    struct PutResponse {
        content: ContentRef,
    }
    #[derive(Deserialize)]
    struct ContentRef {
        html_url: String,
    }
    let parsed: PutResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github put response decode: {e}")))?;
    Ok(parsed.content.html_url)
}

/// Fork a repository via GitHub API. The fork is created on the authenticated
/// user's account (determined by the access_token owner).
///
/// GitHub's fork endpoint is idempotent: calling it on an existing fork
/// returns the existing fork rather than erroring. This is the desired
/// behavior for our onboarding flow (a user can re-trigger without side
/// effects).
///
/// See https://docs.github.com/en/rest/repos/forks#create-a-fork
pub async fn fork_repo(access_token: &str, source_full_name: &str) -> Result<ForkResult, AppError> {
    #[derive(Debug, Deserialize)]
    struct ForkResponse {
        id: i64,
        full_name: String,
        html_url: String,
        default_branch: String,
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{GITHUB_API}/repos/{source_full_name}/forks"))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        // Empty body: default settings (fork to authenticated user, keep repo name)
        .body("{}")
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github fork request failed: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        // 202 Accepted is actually valid - GitHub returned the fork data before
        // it's fully created. But `is_success()` covers 200-299, so we should
        // never enter this branch on 202. This handles 4xx/5xx.

        // One refusal is worth naming, because the raw text does not say what
        // to do and it cost two days of onboarding to work out.
        //
        // "Resource not accessible by integration" is what GitHub answers a
        // **GitHub App** token. This whole flow is written for a classic OAuth
        // App: `build_authorize_url` asks for `read:user public_repo
        // admin:repo_hook`, which a GitHub App ignores entirely - its
        // permissions come from the App's own
        // configuration, not from the authorize URL. So the token is valid, the
        // call is correct, and the permission was never grantable this way.
        //
        // Which is a configuration answer, not a code one: either register a
        // classic OAuth App (client id is 20 hex characters; `Iv23li...` and
        // `Iv1.` are GitHub App ids), or keep the App, give it the permissions
        // a fork needs, and have every user install it first.
        if body.contains("not accessible by integration") {
            return Err(AppError::Internal(format!(
                "github refused the fork with \"not accessible by integration\", which is \
                 what it answers a GitHub App token. This flow is built for a classic \
                 OAuth App - check whether GITHUB_CLIENT_ID is a GitHub App id \
                 (`Iv23li...` or `Iv1.`) rather than a 20-character hex OAuth App id. \
                 Raw: {}",
                &body[..body.len().min(200)]
            )));
        }

        return Err(AppError::Internal(format!(
            "github fork status {status}: {}",
            &body[..body.len().min(200)]
        )));
    }

    let fork: ForkResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github fork decode failed: {e}")))?;

    Ok(ForkResult {
        id: fork.id,
        full_name: fork.full_name,
        html_url: fork.html_url,
        default_branch: fork.default_branch,
    })
}

/// Put a webhook on a freshly created fork.
///
/// The Bonjour Skilluv rite ends in a pull request from the fork's `main` to
/// the fork's `showcase`, both inside the person's own account. Nothing about
/// that reaches `skilluv-community`, so the organisation's webhook has no
/// event to deliver and the platform would never learn the rite was done.
///
/// This is the missing half: the fork tells us directly. It is created with
/// the person's own token, which is why `build_authorize_url` asks for
/// `admin:repo_hook`.
///
/// Best effort by design. A fork that exists with no webhook is a rite that
/// completes late rather than one that cannot start, and the caller has
/// already spent the expensive half of the operation. The error is returned
/// so the caller can record it, not so it can abort.
///
/// Idempotent: GitHub answers 422 when a hook with the same config already
/// exists on the repository, which is the right outcome for a retry and not a
/// failure worth reporting.
pub async fn create_fork_webhook(
    access_token: &str,
    fork_full_name: &str,
    callback_url: &str,
    secret: &str,
) -> Result<(), AppError> {
    let body = serde_json::json!({
        "name": "web",
        "active": true,
        // Only the event the rite turns on. A fork is somebody's personal
        // repository and this is the least it can be asked to report.
        "events": ["pull_request"],
        "config": {
            "url": callback_url,
            "content_type": "json",
            "secret": secret,
            "insecure_ssl": "0",
        }
    });

    let resp = reqwest::Client::new()
        .post(format!("{GITHUB_API}/repos/{fork_full_name}/hooks"))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github hook request failed: {e}")))?;

    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }

    let text = resp.text().await.unwrap_or_default();

    // 422 with "Hook already exists" is a retry landing on its own earlier
    // success.
    if status.as_u16() == 422 && text.contains("already exists") {
        return Ok(());
    }

    // The other refusal worth naming. A token minted before the scope was
    // added carries `read:user public_repo` and nothing else, so this call
    // fails for every account that linked GitHub earlier - and the fix is for
    // that person to link again, not for anybody to debug.
    if status.as_u16() == 404 || text.contains("not accessible by integration") {
        return Err(AppError::Internal(format!(
            "github refused the webhook on {fork_full_name} ({status}). The token \
             most likely predates the `admin:repo_hook` scope - reconnecting \
             GitHub mints one that carries it. Raw: {}",
            &text[..text.len().min(200)]
        )));
    }

    Err(AppError::Internal(format!(
        "github hook status {status}: {}",
        &text[..text.len().min(200)]
    )))
}

// ─── Storage ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GithubConnection {
    pub user_id: Uuid,
    pub github_user_id: i64,
    pub github_login: String,
    pub scopes: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn upsert_connection(
    db: &PgPool,
    user_id: Uuid,
    github_user_id: i64,
    github_login: &str,
    scopes: Option<&str>,
    encrypted_token: &[u8],
    nonce: &[u8],
) -> Result<GithubConnection, AppError> {
    // `github_connections` carries two unique constraints, and the upsert below
    // names only one of them.
    //
    // `ON CONFLICT (user_id)` handles the ordinary case - the same person
    // reconnecting, a token being refreshed. It cannot handle the other one:
    // `github_user_id` is UNIQUE too, so a *second* Skilluv account finishing
    // the OAuth dance with a GitHub identity that already belongs to a first
    // one produces no conflict on `user_id` at all, and Postgres rejects the
    // insert on `github_connections_github_user_id_key` instead. That reached
    // the person as a 500 DATABASE_ERROR, at the end of a browser redirect,
    // inside onboarding where the frontend removes the navbar.
    //
    // The constraint is right and it stays. One GitHub account proves one
    // person; the code rite, the contribution graph and every rank derived
    // from them would mean nothing if two profiles could claim the same
    // commits. What was missing is the sentence that says so.
    crate::services::oauth::refuse_if_claimed(db, user_id, "github", &github_user_id.to_string())
        .await?;

    let conn: GithubConnection = sqlx::query_as(
        r#"
        INSERT INTO github_connections
            (user_id, github_user_id, github_login, access_token_encrypted, access_token_nonce, scopes)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id) DO UPDATE SET
            github_user_id = EXCLUDED.github_user_id,
            github_login = EXCLUDED.github_login,
            access_token_encrypted = EXCLUDED.access_token_encrypted,
            access_token_nonce = EXCLUDED.access_token_nonce,
            scopes = EXCLUDED.scopes,
            updated_at = NOW()
        RETURNING user_id, github_user_id, github_login, scopes, last_synced_at, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(github_user_id)
    .bind(github_login)
    .bind(encrypted_token)
    .bind(nonce)
    .bind(scopes)
    .fetch_one(db)
    .await?;
    Ok(conn)
}

pub async fn load_token(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT access_token_encrypted, access_token_nonce FROM github_connections WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    match row {
        Some((ct, nonce)) => Ok(Some(decrypt_token(jwt_secret, &ct, &nonce)?)),
        None => Ok(None),
    }
}

pub async fn disconnect(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM github_connections WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM github_repos WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

// ─── Repos sync ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub repos_total: usize,
    pub repos_upserted: usize,
}

pub async fn sync_repos_for(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
) -> Result<SyncReport, AppError> {
    let conn: Option<(String,)> =
        sqlx::query_as("SELECT github_login FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let (github_login,) = conn.ok_or(AppError::NotFound("user has not connected GitHub".into()))?;
    let token = load_token(db, jwt_secret, user_id)
        .await?
        .ok_or(AppError::Internal("token missing".into()))?;
    let repos = fetch_public_repos(&token, &github_login).await?;
    let total = repos.len();

    let mut upserted = 0usize;
    for r in &repos {
        let res = sqlx::query(
            r#"
            INSERT INTO github_repos
                (id, user_id, full_name, name, description, html_url, homepage, language,
                 stargazers_count, forks_count, open_issues_count, archived, fork,
                 pushed_at, created_at_github, synced_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15, NOW())
            ON CONFLICT (id) DO UPDATE SET
                full_name = EXCLUDED.full_name,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                html_url = EXCLUDED.html_url,
                homepage = EXCLUDED.homepage,
                language = EXCLUDED.language,
                stargazers_count = EXCLUDED.stargazers_count,
                forks_count = EXCLUDED.forks_count,
                open_issues_count = EXCLUDED.open_issues_count,
                archived = EXCLUDED.archived,
                fork = EXCLUDED.fork,
                pushed_at = EXCLUDED.pushed_at,
                synced_at = NOW()
            "#,
        )
        .bind(r.id)
        .bind(user_id)
        .bind(&r.full_name)
        .bind(&r.name)
        .bind(&r.description)
        .bind(&r.html_url)
        .bind(&r.homepage)
        .bind(&r.language)
        .bind(r.stargazers_count)
        .bind(r.forks_count)
        .bind(r.open_issues_count)
        .bind(r.archived)
        .bind(r.fork)
        .bind(r.pushed_at)
        .bind(r.created_at)
        .execute(db)
        .await;
        if res.is_ok() {
            upserted += 1;
        }
    }

    sqlx::query("UPDATE github_connections SET last_synced_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;

    Ok(SyncReport {
        repos_total: total,
        repos_upserted: upserted,
    })
}

pub async fn top_repos_for_user(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<RepoSummary>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, full_name, description, html_url, language, stargazers_count, pushed_at
        FROM github_repos
        WHERE user_id = $1 AND archived = FALSE AND fork = FALSE
        ORDER BY stargazers_count DESC, pushed_at DESC NULLS LAST
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit.clamp(1, 50))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RepoSummary {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub language: Option<String>,
    pub stargazers_count: i32,
    pub pushed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let (ct, nonce) = encrypt_token("super-secret", "gho_abc123").unwrap();
        let plain = decrypt_token("super-secret", &ct, &nonce).unwrap();
        assert_eq!(plain, "gho_abc123");
    }

    #[test]
    fn decrypt_fails_with_wrong_secret() {
        let (ct, nonce) = encrypt_token("secret-a", "gho_abc").unwrap();
        assert!(decrypt_token("secret-b", &ct, &nonce).is_err());
    }

    #[test]
    fn decrypt_fails_with_tampered_ciphertext() {
        let (mut ct, nonce) = encrypt_token("secret", "gho_abc").unwrap();
        ct[0] ^= 0xFF;
        assert!(decrypt_token("secret", &ct, &nonce).is_err());
    }

    #[test]
    fn authorize_url_includes_required_params() {
        let url = build_authorize_url("CID", "https://app.skilluv.com/cb", "rand-state");
        assert!(url.contains("client_id=CID"));
        assert!(url.contains("state=rand-state"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp.skilluv.com%2Fcb"));
    }

    /// Every scope, named, and the exact string.
    ///
    /// This asserted `scope=read:user%20public_repo` as a substring, which
    /// kept passing when `admin:repo_hook` was appended - a test that agrees
    /// with whatever it is shown is not holding anything. Each scope is named
    /// separately, and the whole value is pinned, so adding or dropping one is
    /// a decision somebody makes here rather than a diff nobody reads.
    #[test]
    fn the_authorize_url_asks_for_the_three_scopes_the_rite_needs() {
        let url = build_authorize_url("CID", "https://app.skilluv.com/cb", "s");

        // Reading who somebody is, so the callback can name the account.
        assert!(url.contains("read:user"), "{url}");
        // Forking a public starter.
        assert!(url.contains("public_repo"), "{url}");
        // Putting a webhook on that fork. Without it the rite's pull request -
        // which lives entirely inside the person's own account - reaches
        // nothing, and the platform never learns it happened.
        assert!(url.contains("admin:repo_hook"), "{url}");

        assert!(
            url.contains("scope=read:user%20public_repo%20admin:repo_hook"),
            "the scope string changed shape: {url}"
        );
    }
}
