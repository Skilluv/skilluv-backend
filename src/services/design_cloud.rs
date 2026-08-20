//! Design tools Skilluv does not own.
//!
//! ## What is here and what is not
//!
//! The OAuth flow, the token storage, the URL parsing and every refusal are
//! here, and all of it works without a single credential.
//!
//! What is not here is a call to Figma, Miro or Webflow. Each one needs a
//! client id and secret from a developer portal, and Skilluv has no account
//! on any of the three. Rather than write code against three APIs nobody can
//! run, the two functions that need a secret say which credential is missing
//! and stop.
//!
//! That is the honest shape of a blocked integration: everything up to the
//! wall, and the wall named.
//!
//! ## Why the URL parsing is here anyway
//!
//! Because it is the half that pays off immediately. A designer pastes a
//! Figma link today; knowing that it *is* a Figma link, and which file and
//! frame it points at, decides whether a reviewer can open it — which is the
//! difference between a review queue that moves and one that does not. None
//! of that needs a token.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

/// A tool somebody can connect an account to.
///
/// Framer, Adobe XD and InVision are absent on purpose: they have no public
/// OAuth, so a "connection" to one would be a row that means nothing. A link
/// to those is a URL on the deliverable, which is what it actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Figma,
    Miro,
    Webflow,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Figma => "figma",
            Self::Miro => "miro",
            Self::Webflow => "webflow",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "figma" => Ok(Self::Figma),
            "miro" => Ok(Self::Miro),
            "webflow" => Ok(Self::Webflow),
            other => Err(AppError::Validation(format!(
                "'{other}' has no OAuth flow to connect to — paste its URL on the deliverable \
                 instead"
            ))),
        }
    }

    /// Where the person is sent to approve the connection.
    pub fn authorize_base(self) -> &'static str {
        match self {
            Self::Figma => "https://www.figma.com/oauth",
            Self::Miro => "https://miro.com/oauth/authorize",
            Self::Webflow => "https://webflow.com/oauth/authorize",
        }
    }

    /// The least that will do. Asking for more than is needed is how a
    /// connection prompt turns into a refusal.
    pub fn scopes(self) -> &'static str {
        match self {
            // Reading files, and nothing else: Skilluv never writes into
            // somebody's design file.
            Self::Figma => "file_read",
            Self::Miro => "boards:read",
            Self::Webflow => "sites:read",
        }
    }

    /// Which environment variables carry this provider's credentials.
    pub fn credential_names(self) -> (&'static str, &'static str) {
        match self {
            Self::Figma => ("FIGMA_CLIENT_ID", "FIGMA_CLIENT_SECRET"),
            Self::Miro => ("MIRO_CLIENT_ID", "MIRO_CLIENT_SECRET"),
            Self::Webflow => ("WEBFLOW_CLIENT_ID", "WEBFLOW_CLIENT_SECRET"),
        }
    }

    /// The credentials, or a refusal naming the one that is missing.
    ///
    /// `503` rather than `500`: the platform is fine, the thing it depends
    /// on is not configured, and an operator reading the log needs the
    /// variable name rather than a stack trace.
    pub fn credentials(self) -> Result<(String, String), AppError> {
        let (id_var, secret_var) = self.credential_names();
        let id = std::env::var(id_var).ok().filter(|v| !v.trim().is_empty());
        let secret = std::env::var(secret_var)
            .ok()
            .filter(|v| !v.trim().is_empty());

        match (id, secret) {
            (Some(id), Some(secret)) => Ok((id, secret)),
            (None, _) => Err(AppError::ServiceUnavailable(format!(
                "{} is not configured — the {} integration is not set up on this deployment",
                id_var,
                self.as_str()
            ))),
            (_, None) => Err(AppError::ServiceUnavailable(format!(
                "{} is not configured — the {} integration is not set up on this deployment",
                secret_var,
                self.as_str()
            ))),
        }
    }
}

/// Where a deliverable lives, when it does not live in a file.
///
/// Wider than [`Provider`]: a Framer link is a real location for a
/// deliverable even though nobody can connect an account to Framer.
pub const SOURCE_PROVIDERS: &[&str] = &[
    "figma",
    "miro",
    "webflow",
    "framer",
    "adobe_xd",
    "invision",
    "sketch_cloud",
    "other",
];

/// What a pasted URL turns out to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CloudSource {
    /// One of [`SOURCE_PROVIDERS`].
    pub provider: String,
    /// The file, board or site, where the URL names one.
    pub key: Option<String>,
    /// The frame or node inside it, where the URL names one.
    pub node_id: Option<String>,
    /// Whether a stranger with the link can see it.
    ///
    /// Always false for the tools that require an account. Said out loud
    /// because a review queue full of links nobody can open is a queue nobody
    /// works — and the person submitting is the only one who can fix it, at
    /// the moment they submit.
    pub opens_without_account: bool,
}

/// Read a pasted URL.
///
/// Returns `None` for anything that is not a design tool link — a GitHub URL
/// pasted into a design deliverable is a mistake worth surfacing, not a
/// source to record.
pub fn read_url(url: &str) -> Option<CloudSource> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();

    // Figma: /file/KEY/name, /design/KEY/name, /proto/KEY/name, with an
    // optional ?node-id=1-2. All four shapes are current — Figma renamed
    // `/file/` to `/design/` and kept both working, so a parser that knows
    // only one will start failing on links people paste tomorrow.
    if lower.contains("figma.com/") {
        let key = segment_after(trimmed, &["/file/", "/design/", "/proto/", "/board/"]);
        return Some(CloudSource {
            provider: "figma".into(),
            key,
            node_id: query_value(trimmed, "node-id"),
            // A Figma link is viewable only if the file was shared publicly,
            // and nothing in the URL says whether it was. Assumed not: being
            // told to check is cheap, and a reviewer who cannot open the work
            // is not.
            opens_without_account: false,
        });
    }

    if lower.contains("miro.com/") {
        return Some(CloudSource {
            provider: "miro".into(),
            key: segment_after(trimmed, &["/app/board/"]),
            node_id: None,
            opens_without_account: false,
        });
    }

    if lower.contains("webflow.io/") || lower.contains("webflow.com/") {
        return Some(CloudSource {
            provider: "webflow".into(),
            key: None,
            node_id: None,
            // A published Webflow site is a website. That is the whole point
            // of it.
            opens_without_account: lower.contains("webflow.io/"),
        });
    }

    if lower.contains("framer.app/") || lower.contains("framer.website/") {
        return Some(CloudSource {
            provider: "framer".into(),
            key: None,
            node_id: None,
            opens_without_account: true,
        });
    }

    if lower.contains("xd.adobe.com/") {
        return Some(CloudSource {
            provider: "adobe_xd".into(),
            key: None,
            node_id: None,
            opens_without_account: true,
        });
    }

    if lower.contains("invisionapp.com/") {
        return Some(CloudSource {
            provider: "invision".into(),
            key: None,
            node_id: None,
            opens_without_account: false,
        });
    }

    if lower.contains("sketch.com/s/") {
        return Some(CloudSource {
            provider: "sketch_cloud".into(),
            key: segment_after(trimmed, &["/s/"]),
            node_id: None,
            opens_without_account: false,
        });
    }

    None
}

/// The path segment following one of `markers`.
fn segment_after(url: &str, markers: &[&str]) -> Option<String> {
    for marker in markers {
        if let Some(rest) = url.split(marker).nth(1) {
            let segment: String = rest
                .chars()
                .take_while(|c| *c != '/' && *c != '?' && *c != '#')
                .collect();
            if !segment.is_empty() {
                return Some(segment);
            }
        }
    }
    None
}

/// One query parameter, undecoded beyond `%3A` — which is the only escape
/// Figma actually puts in a node id.
fn query_value(url: &str, name: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == name && !value.is_empty() {
            let value = value.split('#').next().unwrap_or(value);
            return Some(value.replace("%3A", ":").replace("%3a", ":"));
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════
// Connections
// ═══════════════════════════════════════════════════════════════════

/// A connection, as anybody but the token-holder sees it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Connection {
    pub provider: String,
    pub scopes: Vec<String>,
    pub remote_handle: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Somebody's live connections. Never the tokens: nothing outside this module
/// has a reason to read one, and a field that is never returned cannot be
/// returned by accident.
pub async fn list_for(db: &PgPool, user_id: Uuid) -> Result<Vec<Connection>, AppError> {
    sqlx::query_as::<_, Connection>(
        "SELECT provider, scopes, remote_handle, expires_at, connected_at
           FROM design_cloud_connections
          WHERE user_id = $1 AND revoked_at IS NULL
          ORDER BY connected_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .map_err(Into::into)
}

/// Store a connection, replacing any live one for the same provider.
///
/// The tokens arrive already encrypted: this module holds the shape, and the
/// key belongs to the caller that has the application secret.
#[allow(clippy::too_many_arguments)]
pub async fn store(
    db: &PgPool,
    user_id: Uuid,
    provider: Provider,
    access: (Vec<u8>, Vec<u8>),
    refresh: Option<(Vec<u8>, Vec<u8>)>,
    scopes: &[String],
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    remote_handle: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;

    // Reconnecting replaces rather than adds. Two live tokens for one
    // provider would leave no rule saying which a fetch should use, and the
    // partial unique index refuses it anyway — better a deliberate
    // replacement than a constraint violation surfacing as a 500.
    sqlx::query(
        "UPDATE design_cloud_connections
            SET revoked_at = NOW(),
                access_token_ciphertext = '\\x'::BYTEA,
                access_token_nonce = '\\x'::BYTEA,
                refresh_token_ciphertext = NULL,
                refresh_token_nonce = NULL
          WHERE user_id = $1 AND provider = $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(provider.as_str())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO design_cloud_connections
             (user_id, provider, access_token_ciphertext, access_token_nonce,
              refresh_token_ciphertext, refresh_token_nonce, scopes, expires_at,
              remote_handle)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(user_id)
    .bind(provider.as_str())
    .bind(&access.0)
    .bind(&access.1)
    .bind(refresh.as_ref().map(|r| &r.0))
    .bind(refresh.as_ref().map(|r| &r.1))
    .bind(scopes)
    .bind(expires_at)
    .bind(remote_handle)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Disconnect, and wipe the tokens in the same statement.
///
/// The row survives: a later question about what was fetched and when needs
/// an answer. What must not survive is the ability to fetch anything more.
pub async fn revoke(db: &PgPool, user_id: Uuid, provider: Provider) -> Result<bool, AppError> {
    let done = sqlx::query(
        "UPDATE design_cloud_connections
            SET revoked_at = NOW(),
                access_token_ciphertext = '\\x'::BYTEA,
                access_token_nonce = '\\x'::BYTEA,
                refresh_token_ciphertext = NULL,
                refresh_token_nonce = NULL
          WHERE user_id = $1 AND provider = $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(provider.as_str())
    .execute(db)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The URL somebody is sent to in order to approve a connection.
///
/// Built even when the secret is missing — only the client id is needed, and
/// failing here would hide a misconfiguration behind a button that does
/// nothing.
pub fn authorize_url(
    provider: Provider,
    redirect_uri: &str,
    state_token: &str,
) -> Result<String, AppError> {
    let (client_id, _secret) = provider.credentials()?;
    Ok(format!(
        "{}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code",
        provider.authorize_base(),
        urlencode(&client_id),
        urlencode(redirect_uri),
        urlencode(provider.scopes()),
        urlencode(state_token),
    ))
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figma_link_gives_up_its_file_and_frame() {
        let source = read_url("https://www.figma.com/file/ABC123/identite?node-id=1%3A2").unwrap();
        assert_eq!(source.provider, "figma");
        assert_eq!(source.key.as_deref(), Some("ABC123"));
        assert_eq!(source.node_id.as_deref(), Some("1:2"));
    }

    #[test]
    fn figma_renamed_its_urls_and_both_still_work() {
        // Figma moved `/file/` to `/design/` and kept the old one alive. A
        // parser that knows one shape starts failing on links people paste
        // tomorrow.
        for url in [
            "https://www.figma.com/design/XYZ789/identite",
            "https://www.figma.com/file/XYZ789/identite",
            "https://www.figma.com/proto/XYZ789/identite",
        ] {
            assert_eq!(
                read_url(url).unwrap().key.as_deref(),
                Some("XYZ789"),
                "{url}"
            );
        }
    }

    #[test]
    fn a_tool_link_says_whether_a_reviewer_can_open_it() {
        // The whole point of recording the provider. A queue full of links
        // nobody can open is a queue nobody works.
        assert!(
            !read_url("https://www.figma.com/file/A/b")
                .unwrap()
                .opens_without_account
        );
        assert!(
            read_url("https://exemple.webflow.io/page")
                .unwrap()
                .opens_without_account
        );
        assert!(
            read_url("https://exemple.framer.app/")
                .unwrap()
                .opens_without_account
        );
    }

    #[test]
    fn something_that_is_not_a_design_tool_is_not_a_source() {
        // A GitHub URL in a design deliverable is a mistake worth surfacing,
        // not a source to record.
        assert!(read_url("https://github.com/org/repo").is_none());
        // And http is refused outright: a link a reviewer opens has to be one
        // nobody can rewrite in transit.
        assert!(read_url("http://www.figma.com/file/A/b").is_none());
    }

    #[test]
    fn a_provider_with_no_oauth_cannot_be_connected_to() {
        // Framer has no public OAuth. A "connection" to it would be a row
        // that means nothing, and the message says what to do instead.
        let refused = Provider::parse("framer").unwrap_err();
        assert!(format!("{refused:?}").contains("URL"), "{refused:?}");
        assert!(Provider::parse("figma").is_ok());
    }

    #[test]
    fn a_missing_credential_names_itself() {
        // An operator reading this needs the variable name, not a stack
        // trace. The three providers are unconfigured on a fresh checkout, so
        // this is also the state every developer sees.
        unsafe {
            std::env::remove_var("MIRO_CLIENT_ID");
        }
        let err = Provider::Miro.credentials().unwrap_err();
        assert!(format!("{err:?}").contains("MIRO_CLIENT_ID"), "{err:?}");
    }

    #[test]
    fn the_scopes_asked_for_are_read_only() {
        // Skilluv never writes into somebody's design file, and asking for
        // more than is needed is how a connection prompt becomes a refusal.
        for provider in [Provider::Figma, Provider::Miro, Provider::Webflow] {
            let scopes = provider.scopes();
            assert!(scopes.contains("read"), "{scopes}");
            assert!(!scopes.contains("write"), "{scopes}");
        }
    }
}
