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

/// The mail service and the URLs, installed once at startup.
///
/// The proof engine, the mention recorder and the reconciliation sweep run
/// with a `PgPool` and nothing else, by design — threading `AppState` down
/// into them would invert the dependency between the service and HTTP
/// layers. But they emit kinds whose email is on by default, and a context
/// with no mail service turned that into an error log and a message nobody
/// received. `rank.promoted`, `deliverable.first_verified` and
/// `admin.payout_needs_replay` were all silently email-less that way, and
/// the last one is the queue of payouts a human has to unblock.
///
/// So the pieces that are genuinely process-wide live here. They are:
/// `EmailService` is an API key and a from-address, and the two URLs are
/// deployment constants. None of them is request state, and pretending
/// otherwise is what made three notifications disappear.
struct Ambient {
    email: std::sync::Arc<crate::services::EmailService>,
    frontend_url: String,
    jwt_secret: String,
}

static AMBIENT: std::sync::OnceLock<Ambient> = std::sync::OnceLock::new();

/// Called once from `main`, before any background task can emit.
///
/// Idempotent and ignores a second call: a test harness booting two apps in
/// one process must not panic, and the values are identical anyway.
pub fn install_ambient(
    email: std::sync::Arc<crate::services::EmailService>,
    frontend_url: String,
    jwt_secret: String,
) {
    let _ = AMBIENT.set(Ambient {
        email,
        frontend_url,
        jwt_secret,
    });
}

impl<'a> Ctx<'a> {
    /// Database only, for callers that hold nothing else.
    ///
    /// Not email-less: the mail service falls back to what `main` installed,
    /// so a promotion reached from a background webhook sends the same
    /// message as one reached from a request. Live channels stay absent —
    /// a Redis connection and a WebSocket registry are per-process state
    /// this cannot borrow, and their loss costs immediacy rather than the
    /// message.
    pub fn db_only(db: &'a sqlx::PgPool) -> Self {
        let ambient = AMBIENT.get();
        Self {
            db,
            redis: None,
            ws: None,
            email: ambient.map(|a| a.email.as_ref()),
            frontend_url: ambient.map(|a| a.frontend_url.as_str()),
            jwt_secret: ambient.map(|a| a.jwt_secret.as_str()),
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
    /// Everyone holding any one of several capabilities.
    ///
    /// Owned strings because the capability is often built at runtime:
    /// `design_reviewer:{group}` names one of thirteen families, and the
    /// family is read from the slice being reviewed. Duplicates are removed,
    /// so somebody holding two of them is notified once.
    AnyCapability(Vec<String>),
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
    /// Folded into a notification the recipient already had. Not a delivery
    /// and not a refusal: the message arrived, on a line that was already
    /// there, and deliberately without a second buzz.
    pub grouped: usize,
    /// Failed on its channel and was put in the outbox to try again.
    pub queued: usize,
    /// A transactional push that could not be delivered, so an email was
    /// queued instead. Counted apart from `queued` because it means a
    /// device is unreachable, which is worth seeing on its own.
    pub fell_back: usize,
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
    /// How long an unread notification of this kind absorbs another about
    /// the same context. `None` never groups.
    group_window_seconds: Option<i32>,
    allows_in_app: bool,
    allows_push: bool,
    allows_email: bool,
    default_in_app: bool,
    default_push: bool,
    default_email: bool,
    transactional: bool,
}

/// What writing the durable row did.
struct InApp {
    #[allow(dead_code)] // Kept for the WebSocket payload and future callers.
    id: Uuid,
    /// True when it folded into an existing notification rather than
    /// adding one.
    grouped: bool,
}

/// A notification being built.
pub struct Builder<'a> {
    ctx: Ctx<'a>,
    recipient: Recipient,
    kind: &'a str,
    args: Vec<(String, String)>,
    payload: Option<Value>,
    stats: Vec<(String, String)>,
}

/// Start building a notification.
pub fn send<'a>(ctx: impl Into<Ctx<'a>>, recipient: Recipient, kind: &'a str) -> Builder<'a> {
    Builder {
        ctx: ctx.into(),
        recipient,
        kind,
        args: Vec::new(),
        payload: None,
        stats: Vec::new(),
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

    /// A figure to show in the email, under the body.
    ///
    /// For the handful of notifications that are a shape rather than a
    /// sentence — the weekly digest, and nothing else today. The label is
    /// a translation key, resolved in the recipient's language.
    pub fn stat(mut self, label_key: &str, value: impl Into<String>) -> Self {
        self.stats.push((label_key.to_string(), value.into()));
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

        // Queued counts as reached: the obligation is now the outbox's, and
        // it says so loudly of its own accord if it runs out of attempts.
        // Alerting here as well would page twice for one problem.
        if kind_row.transactional
            && delivery.in_app == 0
            && delivery.push == 0
            && delivery.email == 0
            && delivery.grouped == 0
            && delivery.queued == 0
            && delivery.fell_back == 0
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
        // Set when this event folded into a notification the person already
        // has. Everything below reads it: the whole value of grouping is
        // that the second mention in a thread does not buzz again.
        let mut absorbed = false;

        // In-app first: it is the durable record, and the one the others are
        // a courtesy on top of.
        if kind.allows_in_app && wants(self.ctx.db, user_id, self.kind, Channel::InApp, kind).await
        {
            match self
                .write_in_app(user_id, &locale, &title, &body, kind)
                .await
            {
                Ok(written) => {
                    absorbed = written.grouped;
                    if !absorbed {
                        delivery.in_app += 1;
                    } else {
                        delivery.grouped += 1;
                    }
                    reached = true;
                }
                Err(e) => {
                    delivery.failures.push(format!("in_app: {e}"));
                    tracing::error!(kind = self.kind, user = %user_id, error = %e,
                        "in-app notification failed");
                }
            }
        }

        // Quiet hours suppress the buzz, never the record. A transactional
        // kind ignores them: someone whose payout failed at 3am would
        // rather be woken.
        let quiet = !kind.transactional && in_quiet_hours(self.ctx.db, user_id).await;

        // A group that already buzzed does not buzz again. This is the
        // whole point: ten replies to one thread are one interruption, and
        // an application that vibrates ten times is one people mute.
        if kind.allows_push
            && !quiet
            && !absorbed
            && wants(self.ctx.db, user_id, self.kind, Channel::Push, kind).await
        {
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
                Err(e) => {
                    // A push is never retried: the usual cause is a token
                    // that went stale when someone reinstalled, and asking
                    // the same question again gets the same answer forever.
                    tracing::debug!(kind = self.kind, user = %user_id, error = %e,
                        "mobile push failed");

                    // For a transactional kind the push was the fast path
                    // and nothing took its place. Another road, then —
                    // ignoring the email preference, because the message is
                    // an obligation rather than a nudge.
                    if kind.transactional {
                        self.queue(
                            user_id,
                            &locale,
                            &title,
                            &body,
                            kind,
                            Channel::Email,
                            true,
                            &e.to_string(),
                        )
                        .await;
                        delivery.fell_back += 1;
                        reached = true;
                    }
                }
            }
        }

        // Same for email, and more so: the first one already said what
        // happened and where, and a second about the same thread would read
        // as the platform being broken.
        if kind.allows_email
            && !absorbed
            && wants(self.ctx.db, user_id, self.kind, Channel::Email, kind).await
        {
            match self.send_email(user_id, &locale, &title, &body, kind).await {
                Ok(true) => {
                    delivery.email += 1;
                    reached = true;
                }
                Ok(false) => {}
                Err(e) => {
                    // Queued rather than lost. A 503 from the provider used
                    // to be logged and the message was gone — not late,
                    // gone, with nowhere to put it.
                    delivery.failures.push(format!("email: {e}"));
                    tracing::warn!(kind = self.kind, user = %user_id, error = %e,
                        "notification email failed — queued for retry");
                    self.queue(
                        user_id,
                        &locale,
                        &title,
                        &body,
                        kind,
                        Channel::Email,
                        false,
                        &e.to_string(),
                    )
                    .await;
                    delivery.queued += 1;
                }
            }
        }

        if !reached {
            delivery.declined += 1;
        }
    }

    /// Hand a failed channel to the outbox.
    ///
    /// The words are already translated and interpolated, so a retry sends
    /// what this attempt meant to send. The frame around them is rendered
    /// again at delivery, which is why a template fix reaches a queued
    /// message and a theme change is honoured.
    #[allow(clippy::too_many_arguments)]
    async fn queue(
        &self,
        user_id: Uuid,
        locale: &str,
        title: &str,
        body: &str,
        kind_row: &KindRow,
        channel: Channel,
        is_fallback: bool,
        reason: &str,
    ) {
        let cta_url = self.cta_url(kind_row);
        let unsubscribe_url = if kind_row.transactional {
            None
        } else {
            self.unsubscribe_url(user_id)
        };

        crate::services::outbox::enqueue(
            self.ctx.db,
            crate::services::outbox::Queued {
                user_id,
                notification_id: None,
                kind: self.kind,
                channel,
                locale,
                title,
                body,
                payload: self.payload.as_ref(),
                cta_url: cta_url.as_deref(),
                unsubscribe_url: unsubscribe_url.as_deref(),
                is_fallback,
                reason,
            },
        )
        .await;
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

    /// The context this notification is about, if it has one.
    ///
    /// `kind:target_type:target_id`, built from the payload the caller
    /// already passes. A kind whose payload names no subject has no
    /// context, so it never groups — which is the right answer for
    /// anything carrying money or a decision.
    fn group_key(&self) -> Option<String> {
        let payload = self.payload.as_ref()?;
        // In the order senders use them. The first that is present wins,
        // so a comment on a post groups by the post rather than by itself.
        for field in [
            "post_id",
            "guild_id",
            "conversation_id",
            "target_id",
            "project_id",
            "slice_id",
        ] {
            if let Some(value) = payload.get(field).and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Null => None,
                other => Some(other.to_string().trim_matches('"').to_string()),
            }) {
                return Some(format!("{}:{field}:{value}", self.kind));
            }
        }
        None
    }

    /// Who this notification is about, for "Awa and 3 others".
    ///
    /// Taken from the `author` argument the sender already provides, which
    /// is the same string the ungrouped copy interpolates.
    fn actor(&self) -> Option<&str> {
        self.args
            .iter()
            .find(|(name, _)| name == "author" || name == "inviter")
            .map(|(_, value)| value.as_str())
    }

    async fn write_in_app(
        &self,
        user_id: Uuid,
        locale: &str,
        title: &str,
        body: &str,
        kind_row: &KindRow,
    ) -> Result<InApp, AppError> {
        // An unread notification about the same context, still inside its
        // window, absorbs this one instead of adding a line.
        if let (Some(window), Some(group_key)) = (kind_row.group_window_seconds, self.group_key())
            && let Some(existing) = self
                .absorb_into(user_id, &group_key, window, locale)
                .await?
        {
            return Ok(InApp {
                id: existing,
                grouped: true,
            });
        }

        // The button, resolved once and stored with the row.
        //
        // The catalogue holds the path and `cta_url` fills its placeholders
        // from the payload, returning nothing rather than a broken link.
        // Doing it here means the in-app client and the email agree on where
        // a notification leads — the client cannot resolve it on its own,
        // since the catalogue is not something it can read.
        let mut stored = self.payload.clone();
        if let Some(href) = self.cta_url(kind_row) {
            let cta = serde_json::json!({
                "href": href,
                "label": i18n::t(locale, &format!("notification.{}.cta", self.kind)),
            });
            let empty = stored.is_none();
            match stored.as_mut().and_then(|value| value.as_object_mut()) {
                Some(object) => {
                    object.insert("next_step_cta".to_string(), cta);
                }
                // A payload that is not an object is left alone rather than
                // replaced: whatever it holds was put there deliberately,
                // and a button is not worth losing it over.
                None if empty => {
                    stored = Some(serde_json::json!({ "next_step_cta": cta }));
                }
                None => {}
            }
        }

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO notifications
                (user_id, notification_type, title, body, data, kind, locale, payload,
                 group_key, group_actors, updated_at)
            VALUES ($1, $2, $3, $4, $5, $2, $6, $9, $7, $8, NOW())
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(self.kind)
        .bind(title)
        .bind(body)
        .bind(&stored)
        .bind(locale)
        .bind(self.group_key())
        .bind(serde_json::json!(
            self.actor().map(|a| vec![a]).unwrap_or_default()
        ))
        // `payload` keeps the raw arguments: it is what a re-render reads,
        // and a resolved button is not an argument.
        .bind(&self.payload)
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

        Ok(InApp { id, grouped: false })
    }

    /// Fold this event into an open notification about the same context.
    ///
    /// Returns the absorbing row's id, or `None` when there is nothing open
    /// to absorb it — a different context, an expired window, or one the
    /// person has already read. Read is a boundary on purpose: merging into
    /// a line someone has seen would make it change under them, and they
    /// would never learn the second thing happened.
    async fn absorb_into(
        &self,
        user_id: Uuid,
        group_key: &str,
        window_seconds: i32,
        locale: &str,
    ) -> Result<Option<Uuid>, AppError> {
        #[derive(sqlx::FromRow)]
        struct Open {
            id: Uuid,
            group_count: i32,
            group_actors: Value,
        }

        let open: Option<Open> = sqlx::query_as(
            "SELECT id, group_count, group_actors
               FROM notifications
              WHERE user_id = $1
                AND group_key = $2
                AND read = FALSE
                AND created_at > NOW() - ($3 || ' seconds')::INTERVAL
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(user_id)
        .bind(group_key)
        .bind(window_seconds.to_string())
        .fetch_optional(self.ctx.db)
        .await?;

        let Some(open) = open else {
            return Ok(None);
        };

        // Newest first, distinct, capped. A thread with four hundred
        // participants must not carry four hundred names in a row someone
        // reads on a phone, and only the first two are ever shown.
        const ACTORS_KEPT: usize = 4;
        let mut actors: Vec<String> = open
            .group_actors
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(actor) = self.actor() {
            actors.retain(|existing| existing != actor);
            actors.insert(0, actor.to_string());
            actors.truncate(ACTORS_KEPT);
        }

        let count = open.group_count + 1;
        let (title, body) = self.grouped_text(locale, &actors, count);

        sqlx::query(
            "UPDATE notifications
                SET group_count = $2,
                    group_actors = $3,
                    title = $4,
                    body = $5,
                    payload = $6,
                    data = $6,
                    updated_at = NOW()
              WHERE id = $1",
        )
        .bind(open.id)
        .bind(count)
        .bind(serde_json::json!(actors))
        .bind(&title)
        .bind(&body)
        .bind(&self.payload)
        .execute(self.ctx.db)
        .await?;

        // The live channel still fires: the bell count does not change, but
        // an open list must not show a stale line.
        if let Some(ws) = self.ctx.ws {
            ws.send_to_user(
                user_id,
                crate::websocket::WsMessage {
                    event: "notification.updated".to_string(),
                    room: None,
                    payload: serde_json::json!({
                        "id": open.id,
                        "kind": self.kind,
                        "title": title,
                        "body": body,
                        "group_count": count,
                    }),
                },
            )
            .await;
        }

        metrics::counter!(
            "skilluv_notifications_grouped_total",
            "kind" => self.kind.to_string()
        )
        .increment(1);

        Ok(Some(open.id))
    }

    /// The copy for a notification standing for several events.
    ///
    /// Falls back to the ungrouped text when a kind has no grouped copy, so
    /// a window added to the catalogue without translations degrades to the
    /// old wording rather than to a translation key.
    fn grouped_text(&self, locale: &str, actors: &[String], count: i32) -> (String, String) {
        let others = (count as usize).saturating_sub(1);
        let first = actors.first().map(String::as_str).unwrap_or("");
        let owned: Vec<(&str, String)> = vec![
            ("author", first.to_string()),
            ("actor", first.to_string()),
            ("count", count.to_string()),
            ("others", others.to_string()),
        ];
        let mut args: Vec<(&str, &str)> = self
            .args
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        // The grouped values win over the single-event ones they replace.
        for (name, value) in &owned {
            args.retain(|(existing, _)| existing != name);
            args.push((name, value.as_str()));
        }

        let title_key = format!("notification.{}.grouped.title", self.kind);
        let body_key = format!("notification.{}.grouped.body", self.kind);
        let title = i18n::t_with(locale, &title_key, &args);
        let body = i18n::t_with(locale, &body_key, &args);

        if title == title_key || body == body_key {
            tracing::debug!(
                kind = self.kind,
                "kind groups but has no grouped copy — falling back to the single-event wording"
            );
            return (
                i18n::t_with(locale, &format!("notification.{}.title", self.kind), &args),
                i18n::t_with(locale, &format!("notification.{}.body", self.kind), &args),
            );
        }
        (title, body)
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
                // Labels are translation keys until here, so one digest
                // reaches a French reader and an Arabic one in their own
                // words without the caller knowing either.
                stats: &self
                    .stats
                    .iter()
                    .map(|(key, value)| (i18n::t(locale, key), value.clone()))
                    .collect::<Vec<_>>(),
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
        // `expires_at` is checked as well as `revoked_at`: a capability that
        // has run out is not held, and the guards that let somebody *act* on
        // these queues have always read both. Notifying on the looser of the
        // two rules meant sending people work they would then be refused.
        Recipient::Capability(capability) => {
            let rows: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT user_id FROM user_capabilities
                  WHERE capability = $1
                    AND revoked_at IS NULL
                    AND (expires_at IS NULL OR expires_at > NOW())",
            )
            .bind(capability)
            .fetch_all(db)
            .await?;
            Ok(rows.into_iter().map(|(id,)| id).collect())
        }
        Recipient::AnyCapability(capabilities) => {
            let rows: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT DISTINCT user_id FROM user_capabilities
                  WHERE capability = ANY($1)
                    AND revoked_at IS NULL
                    AND (expires_at IS NULL OR expires_at > NOW())",
            )
            .bind(capabilities)
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
        "SELECT cta_path, group_window_seconds, allows_in_app, allows_push, allows_email,
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

/// Does this person want this kind, without a loaded catalogue row.
///
/// For callers outside a delivery — the settings screen projecting several
/// kinds onto one switch, a bulk job deciding whether to build an email at
/// all. An unknown kind reads as "no": inventing consent for something the
/// catalogue does not describe is the wrong direction to fail in.
pub async fn wants_kind(db: &PgPool, user_id: Uuid, kind: &str, channel: Channel) -> bool {
    match load_kind(db, kind).await {
        Ok(row) => wants(db, user_id, kind, channel, &row).await,
        Err(_) => false,
    }
}

/// Is it the middle of this person's night?
///
/// Only push asks. A buzz at three in the morning is how an application
/// gets its notifications revoked at the operating-system level, which is a
/// decision nobody goes back on — and the in-app record and the email are
/// waiting whenever they wake up.
///
/// Unknown timezone means not enforced. Assuming UTC would silence a talent
/// in Cotonou at the wrong hours half the year, which is worse than not
/// having the feature.
async fn in_quiet_hours(db: &PgPool, user_id: Uuid) -> bool {
    #[derive(sqlx::FromRow)]
    struct Window {
        quiet_hours_start: Option<i16>,
        quiet_hours_end: Option<i16>,
        timezone: Option<String>,
    }

    let Ok(Some(window)) = sqlx::query_as::<_, Window>(
        "SELECT quiet_hours_start, quiet_hours_end, timezone FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    else {
        return false;
    };

    let (Some(start), Some(end), Some(tz)) = (
        window.quiet_hours_start,
        window.quiet_hours_end,
        window.timezone,
    ) else {
        return false;
    };

    let Ok(zone) = tz.parse::<chrono_tz::Tz>() else {
        tracing::warn!(user = %user_id, timezone = %tz, "unparseable timezone — quiet hours not enforced");
        return false;
    };

    let hour = {
        use chrono::Timelike;
        chrono::Utc::now().with_timezone(&zone).hour() as i16
    };
    // A window that wraps midnight — 22 to 7 — is the normal case, and the
    // one a naive range check gets wrong.
    if start <= end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
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
