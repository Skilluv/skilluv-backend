//! One way to notify someone, across every channel.
//!
//! Before this, a caller wrote a French title by hand, called a service that
//! only knew how to insert a row, and got in-app plus WebSocket plus a mobile
//! push whether or not the recipient wanted them. Email lived somewhere else
//! entirely, with its own three-category preference table that answers "may
//! we send the digest" and never "does this person want to know, and how".
//!
//! Here, a caller says *what happened* and *to whom*. This module decides the
//! language, the channels, and whether the recipient agreed — then delivers.
//!
//! ```ignore
//! notify::send(&state, Recipient::User(mentor), "payout.sent")
//!     .arg("amount", "42,50 €")
//!     .payload(json!({ "transaction_id": id }))
//!     .await?;
//! ```
//!
//! ## Why the text is not in the call
//!
//! A title passed as an argument is a title in one language. Every caller
//! becomes a place a translator has to find, and adding a language means
//! editing every call site. The kind is the key: `payout.sent` reads
//! `notification.payout.sent.title` from the catalogue, in whichever
//! language the recipient reads.
//!
//! ## Failure
//!
//! Channels are independent. A push that fails must not lose the in-app
//! record, and an email that bounces must not fail the request that caused
//! it. Each channel's outcome is reported separately in [`Delivery`], and a
//! transactional notification that reaches nobody is logged at error level —
//! that one is a lost obligation, not a missed nudge.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::i18n;

/// What delivering a notification needs.
///
/// Built from an `AppState` where there is one, and from its parts where
/// there is not — the proof engine and the mention recorder run with a
/// database handle and little else, and requiring the whole application
/// state there would mean threading it through call chains that have no
/// other use for it.
///
/// `email` is optional for that reason. A context without it cannot send
/// email, and says so loudly when a notification wanted to: silently
/// dropping a payout receipt because the caller happened not to carry an
/// email service is exactly the class of failure this module exists to
/// remove.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub db: &'a sqlx::PgPool,
    pub redis: Option<&'a redis::aio::ConnectionManager>,
    pub ws: Option<&'a crate::websocket::WsManager>,
    pub email: Option<&'a crate::services::EmailService>,
    /// Where the application lives, for building the button's destination.
    pub frontend_url: Option<&'a str>,
    /// Signs the one-click unsubscribe link. Without it a declinable email
    /// goes out with no way to decline, which is not acceptable and, for
    /// bulk senders, not legal either.
    pub jwt_secret: Option<&'a str>,
}

impl<'a> Ctx<'a> {
    /// Database only. Writes the durable row; no live push, no email.
    pub fn db_only(db: &'a sqlx::PgPool) -> Self {
        Self {
            db,
            redis: None,
            ws: None,
            email: None,
            frontend_url: None,
            jwt_secret: None,
        }
    }
}

impl<'a> From<&'a crate::AppState> for Ctx<'a> {
    fn from(state: &'a crate::AppState) -> Self {
        Self {
            db: &state.db,
            redis: Some(&state.redis),
            ws: Some(&state.ws),
            email: Some(&state.email),
            frontend_url: Some(&state.config.frontend_url),
            jwt_secret: Some(&state.config.jwt_secret),
        }
    }
}

/// Who is being notified.
///
/// An enterprise or an admin audience fans out to people, because only
/// people have a language, a phone and an inbox.
#[derive(Debug, Clone)]
pub enum Recipient {
    User(Uuid),
    /// Every member of an enterprise. Used for billing and talent-search
    /// events, which belong to the organisation rather than to whoever
    /// happened to click.
    Enterprise(Uuid),
    /// Everyone holding a capability — `admin`, `kyc_reviewer`, … Used for
    /// queues that someone has to work.
    Capability(&'static str),
}

/// A delivery channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    InApp,
    Push,
    Email,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::InApp => "in_app",
            Channel::Push => "push",
            Channel::Email => "email",
        }
    }
}

/// What each channel did, per recipient.
#[derive(Debug, Default, Clone)]
pub struct Delivery {
    pub in_app: usize,
    pub push: usize,
    pub email: usize,
    /// Recipients who declined every channel this kind offers. Not an error:
    /// a preference honoured is the system working.
    pub declined: usize,
    /// Channels that were wanted and failed. Non-empty means someone was
    /// meant to hear and did not.
    pub failures: Vec<String>,
}

/// What the catalogue says about a kind.
#[derive(Debug, Clone, sqlx::FromRow)]
struct KindRow {
    cta_path: Option<String>,
    allows_in_app: bool,
    allows_push: bool,
    allows_email: bool,
    default_in_app: bool,
    default_push: bool,
    default_email: bool,
    transactional: bool,
}

/// A notification being built.
pub struct Builder<'a> {
    ctx: Ctx<'a>,
    recipient: Recipient,
    kind: &'a str,
    args: Vec<(String, String)>,
    payload: Option<Value>,
}

/// Start building a notification.
pub fn send<'a>(ctx: impl Into<Ctx<'a>>, recipient: Recipient, kind: &'a str) -> Builder<'a> {
    Builder {
        ctx: ctx.into(),
        recipient,
        kind,
        args: Vec::new(),
        payload: None,
    }
}

impl<'a> Builder<'a> {
    /// Substitute `{name}` in the translated text.
    pub fn arg(mut self, name: &str, value: impl Into<String>) -> Self {
        self.args.push((name.to_string(), value.into()));
        self
    }

    /// Structured data for the client — ids to navigate to, not text.
    pub fn payload(mut self, payload: Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Resolve, render and deliver.
    pub async fn execute(self) -> Result<Delivery, AppError> {
        let kind_row = load_kind(self.ctx.db, self.kind).await?;
        let recipients = resolve_recipients(self.ctx.db, &self.recipient).await?;

        let mut delivery = Delivery::default();
        for user_id in recipients {
            self.deliver_to(user_id, &kind_row, &mut delivery).await;
        }

        if kind_row.transactional
            && delivery.in_app == 0
            && delivery.push == 0
            && delivery.email == 0
        {
            // A transactional notification is an obligation. Reaching nobody
            // is a failure worth waking someone for, not a quiet no-op.
            tracing::error!(
                kind = self.kind,
                failures = ?delivery.failures,
                "transactional notification reached nobody"
            );
            metrics::counter!(
                "skilluv_notification_undelivered_total",
                "kind" => self.kind.to_string()
            )
            .increment(1);
        }

        Ok(delivery)
    }

    async fn deliver_to(&self, user_id: Uuid, kind: &KindRow, delivery: &mut Delivery) {
        let locale = user_locale(self.ctx.db, user_id).await;

        let args: Vec<(&str, &str)> = self
            .args
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let title = i18n::t_with(&locale, &format!("notification.{}.title", self.kind), &args);
        let body = i18n::t_with(&locale, &format!("notification.{}.body", self.kind), &args);

        let mut reached = false;

        // In-app first: it is the durable record, and the one the others are
        // a courtesy on top of.
        if kind.allows_in_app && wants(self.ctx.db, user_id, self.kind, Channel::InApp, kind).await
        {
            match self.write_in_app(user_id, &locale, &title, &body).await {
                Ok(_) => {
                    delivery.in_app += 1;
                    reached = true;
                }
                Err(e) => {
                    delivery.failures.push(format!("in_app: {e}"));
                    tracing::error!(kind = self.kind, user = %user_id, error = %e,
                        "in-app notification failed");
                }
            }
        }

        if kind.allows_push && wants(self.ctx.db, user_id, self.kind, Channel::Push, kind).await {
            let message = crate::services::mobile_push::MobilePushMessage {
                title: &title,
                body: &body,
                data: self.payload.clone(),
            };
            match crate::services::mobile_push::push_to_user_mobile(self.ctx.db, user_id, message)
                .await
            {
                Ok(_) => {
                    delivery.push += 1;
                    reached = true;
                }
                // Not an error by default: a device token goes stale every
                // time someone reinstalls, and the in-app record stands.
                Err(e) => tracing::debug!(kind = self.kind, user = %user_id, error = %e,
                    "mobile push failed"),
            }
        }

        if kind.allows_email && wants(self.ctx.db, user_id, self.kind, Channel::Email, kind).await {
            match self.send_email(user_id, &locale, &title, &body, kind).await {
                Ok(true) => {
                    delivery.email += 1;
                    reached = true;
                }
                Ok(false) => {}
                Err(e) => {
                    delivery.failures.push(format!("email: {e}"));
                    tracing::error!(kind = self.kind, user = %user_id, error = %e,
                        "notification email failed");
                }
            }
        }

        if !reached {
            delivery.declined += 1;
        }
    }

    /// Absolute URL for the email button, or `None`.
    ///
    /// Placeholders in the path are filled from the payload. One that cannot
    /// be filled suppresses the button entirely: a dead link is worse than
    /// no link, because the reader clicks it and concludes the product is
    /// broken.
    fn cta_url(&self, kind: &KindRow) -> Option<String> {
        let path = kind.cta_path.as_deref()?;
        let base = self.ctx.frontend_url?.trim_end_matches('/');

        let mut filled = path.to_string();
        while let Some(start) = filled.find('{') {
            let end = filled[start..].find('}')? + start;
            let key = &filled[start + 1..end];
            let value = self
                .payload
                .as_ref()
                .and_then(|p| p.get(key))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string().trim_matches('"').to_string(),
                })?;
            filled = format!("{}{}{}", &filled[..start], value, &filled[end + 1..]);
        }

        Some(format!("{base}{filled}"))
    }

    /// One-click unsubscribe link for a declinable email.
    ///
    /// Signed with the same secret `GET /api/email/unsubscribe/{token}`
    /// verifies, so the link works without the reader logging in — which is
    /// the whole point of one-click.
    fn unsubscribe_url(&self, user_id: Uuid) -> Option<String> {
        let base = self.ctx.frontend_url?.trim_end_matches('/');
        let jwt_secret = self.ctx.jwt_secret?;
        let secret = crate::routes::email_prefs::unsub_secret(jwt_secret);
        let token = crate::services::digest::build_unsubscribe_token(user_id, self.kind, &secret);
        Some(format!("{base}/api/email/unsubscribe/{token}"))
    }

    async fn write_in_app(
        &self,
        user_id: Uuid,
        locale: &str,
        title: &str,
        body: &str,
    ) -> Result<Uuid, AppError> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO notifications
                (user_id, notification_type, title, body, data, kind, locale, payload)
            VALUES ($1, $2, $3, $4, $5, $2, $6, $5)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(self.kind)
        .bind(title)
        .bind(body)
        .bind(&self.payload)
        .bind(locale)
        .fetch_one(self.ctx.db)
        .await?;

        // Unread counter and live push are a best effort on top of the row,
        // and absent entirely from a database-only context. The durable row
        // is what `GET /api/notifications` reads, so nothing is lost beyond
        // immediacy.
        if let Some(redis) = self.ctx.redis {
            let mut redis = redis.clone();
            let _: Result<i64, _> = redis::AsyncCommands::incr(
                &mut redis,
                format!("notifications:unread:{user_id}"),
                1,
            )
            .await;
        }

        if let Some(ws) = self.ctx.ws {
            ws.send_to_user(
                user_id,
                crate::websocket::WsMessage {
                    event: "notification".to_string(),
                    room: None,
                    payload: serde_json::json!({
                        "id": id,
                        "kind": self.kind,
                        "type": self.kind,
                        "title": title,
                        "body": body,
                        "data": self.payload,
                    }),
                },
            )
            .await;
        }

        Ok(id)
    }

    /// Returns `false` when there is nobody to write to — an account with no
    /// verified address, which is not a failure.
    async fn send_email(
        &self,
        user_id: Uuid,
        locale: &str,
        title: &str,
        body: &str,
        kind_row: &KindRow,
    ) -> Result<bool, AppError> {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT email, display_name FROM users WHERE id = $1 AND email_verified = TRUE",
        )
        .bind(user_id)
        .fetch_optional(self.ctx.db)
        .await?;

        let Some((address, display_name)) = row else {
            return Ok(false);
        };

        // The world they chose, so the message looks like it came from the
        // place they chose it in.
        let theme: Option<String> =
            sqlx::query_scalar("SELECT preferred_theme FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(self.ctx.db)
                .await
                .ok()
                .flatten()
                .flatten();

        // The one thing to do next. An email that says something happened
        // and offers no way to act on it makes the reader hunt for the page
        // themselves, which most of them will not do.
        let cta_url = self.cta_url(kind_row);
        let cta_label = cta_url
            .as_ref()
            .map(|_| i18n::t(locale, &format!("notification.{}.cta", self.kind)));

        // Declinable mail carries the one-click unsubscribe. Transactional
        // mail does not: offering to opt out of a payout receipt would be a
        // promise we cannot keep.
        let unsubscribe_url = if kind_row.transactional {
            None
        } else {
            self.unsubscribe_url(user_id)
        };

        let html =
            crate::services::email_template::render(crate::services::email_template::Email {
                locale,
                theme: theme.as_deref(),
                title,
                body,
                recipient_name: display_name.as_deref(),
                cta_label: cta_label.as_deref(),
                cta_url: cta_url.as_deref(),
                unsubscribe_url: unsubscribe_url.as_deref(),
            });

        // `send_with_log` rather than the raw sender: it honours
        // `email_disabled`, set when an address hard-bounces or the person
        // unsubscribed from everything, and records the attempt. Bypassing
        // it would keep mailing an address the provider already rejected,
        // which is how a sending domain gets its reputation destroyed.
        let Some(email) = self.ctx.email else {
            // Wanted, and impossible here. Loud rather than silent: a
            // transactional message that never left is a lost obligation.
            tracing::error!(
                kind = self.kind,
                user = %user_id,
                "email channel requested but this context carries no email                  service — the message was not sent"
            );
            return Ok(false);
        };

        email
            .send_with_log(
                self.ctx.db,
                crate::services::email::SendWithLogParams {
                    user_id,
                    to_email: &address,
                    to_name: display_name.as_deref().unwrap_or(""),
                    subject: title,
                    html: &html,
                    kind: self.kind,
                },
            )
            .await
    }
}

/// Resolve which people a recipient stands for.
async fn resolve_recipients(db: &PgPool, recipient: &Recipient) -> Result<Vec<Uuid>, AppError> {
    match recipient {
        Recipient::User(id) => Ok(vec![*id]),
        Recipient::Enterprise(id) => {
            let rows: Vec<(Uuid,)> =
                sqlx::query_as("SELECT user_id FROM enterprise_members WHERE enterprise_id = $1")
                    .bind(id)
                    .fetch_all(db)
                    .await?;
            Ok(rows.into_iter().map(|(id,)| id).collect())
        }
        Recipient::Capability(capability) => {
            let rows: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT user_id FROM user_capabilities WHERE capability = $1
                   AND (revoked_at IS NULL)",
            )
            .bind(capability)
            .fetch_all(db)
            .await?;
            Ok(rows.into_iter().map(|(id,)| id).collect())
        }
    }
}

/// The catalogue entry, or a refusal.
///
/// An unknown kind is rejected rather than sent on defaults: a typo in a kind
/// would otherwise deliver a notification titled `notification.payout.snet.
/// title` to a real person.
async fn load_kind(db: &PgPool, kind: &str) -> Result<KindRow, AppError> {
    sqlx::query_as(
        "SELECT cta_path, allows_in_app, allows_push, allows_email,
                default_in_app, default_push, default_email, transactional
           FROM notification_kinds WHERE kind = $1",
    )
    .bind(kind)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Internal(format!(
            "unknown notification kind '{kind}' — add it to notification_kinds \
             along with its translations"
        ))
    })
}

/// Does this person want this kind on this channel?
///
/// Transactional kinds are not negotiable. Otherwise: their stored choice,
/// else the default for the kind.
async fn wants(db: &PgPool, user_id: Uuid, kind: &str, channel: Channel, row: &KindRow) -> bool {
    if row.transactional {
        return true;
    }

    let stored: Option<bool> = sqlx::query_scalar(
        "SELECT enabled FROM notification_preferences
          WHERE user_id = $1 AND kind = $2 AND channel = $3",
    )
    .bind(user_id)
    .bind(kind)
    .bind(channel.as_str())
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    stored.unwrap_or(match channel {
        Channel::InApp => row.default_in_app,
        Channel::Push => row.default_push,
        Channel::Email => row.default_email,
    })
}

/// The language this person reads, falling back to the default.
pub(crate) async fn user_locale(db: &PgPool, user_id: Uuid) -> String {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT preferred_language FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .flatten();
    // No request to read: a notification often originates from a background
    // job, which is exactly why the preference lives on the account.
    i18n::resolve(stored.as_deref(), None)
}

/// The Redis key holding someone's unread count.
///
/// The writer in [`Builder::execute`] and the readers below go through the
/// same helper, so the two can no longer drift apart.
fn unread_key(user_id: Uuid) -> String {
    format!("notifications:unread:{user_id}")
}

/// Unread count, from Redis when warm and from the table otherwise.
///
/// A miss reseeds the key rather than leaving it cold: without that, the
/// counter never recovers from a Redis restart.
pub async fn unread_count(
    db: &PgPool,
    redis: &mut redis::aio::ConnectionManager,
    user_id: Uuid,
) -> Result<i64, AppError> {
    let key = unread_key(user_id);
    if let Some(count) = redis::AsyncCommands::get::<_, Option<i64>>(redis, &key).await? {
        return Ok(count);
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    let () = redis::AsyncCommands::set(redis, &key, count).await?;
    Ok(count)
}

/// Zero the counter, after everything has been marked read.
pub async fn reset_counter(
    redis: &mut redis::aio::ConnectionManager,
    user_id: Uuid,
) -> Result<(), AppError> {
    let () = redis::AsyncCommands::set(redis, unread_key(user_id), 0i64).await?;
    Ok(())
}

/// Take one off the counter, after a single notification was marked read.
///
/// Floors at zero: a cold key reads as 0, and decrementing it would leave a
/// negative badge that never heals.
pub async fn decrement_counter(
    redis: &mut redis::aio::ConnectionManager,
    user_id: Uuid,
) -> Result<(), AppError> {
    let key = unread_key(user_id);
    let current: i64 = redis::AsyncCommands::get(redis, &key).await.unwrap_or(0);
    if current > 0 {
        let _: i64 = redis::AsyncCommands::decr(redis, &key, 1).await?;
    }
    Ok(())
}
