//! Newsletter subscription, double opt-in (SKI-369).
//!
//! The footer form used to wait 400ms so the button looked busy, clear the
//! field so it looked accepted, and discard the address. Every signal the
//! interface gave said the person had subscribed. They had not, and had no way
//! to find out.
//!
//! ## Three endpoints, all public
//!
//! - `POST /newsletter/subscriptions` takes an address and mails a
//!   confirmation link. It never says whether the address was already known.
//! - `GET  /newsletter/confirm/{token}` is the link. Clicking it is the
//!   consent that matters.
//! - `GET  /newsletter/unsubscribe/{token}` needs no account, because the
//!   person who wants out is usually the person who never had one.
//!
//! ## Why the answer is always the same
//!
//! A distinct response for a known address turns a public endpoint into an
//! oracle for "is this person on the list", which is an enumeration surface on
//! exactly the data a newsletter holds. New, pending, already confirmed and
//! previously unsubscribed all get the same body and the same status. The
//! confirmation mail is what changes state, and it goes to the address rather
//! than to the caller.
//!
//! ## Composing or sending the newsletter is not here
//!
//! This collects an address honestly and lets go of it on request. What gets
//! sent, and when, is somebody else's decision and somebody else's code. What
//! that code must respect is written at `mailable_addresses`.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{RateLimiter, extract_ip};

/// The response envelope. Defined here like the other fifty-seven route
/// modules define it: there is no shared one to import.
fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

pub fn newsletter_routes() -> Router<AppState> {
    Router::new()
        .route("/newsletter/subscriptions", post(subscribe))
        .route("/newsletter/confirm/{token}", get(confirm))
        .route("/newsletter/unsubscribe/{token}", get(unsubscribe))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SubscribeBody {
    #[schema(max_length = 320)]
    pub email: String,
    /// `fr`, `en` or `ar`. Anything else is refused rather than silently
    /// defaulted, so a client with a stale locale list finds out.
    #[serde(default = "default_locale")]
    #[schema(max_length = 5)]
    pub locale: String,
    /// Where the address came from, so a list can be explained later.
    #[serde(default = "default_source")]
    #[schema(max_length = 40)]
    pub source: String,
    /// The exact wording shown beside the field.
    ///
    /// Stored rather than a boolean: if the wording changes, a consent
    /// gathered under the old one has to stay readable as that consent.
    #[serde(default)]
    #[schema(max_length = 2000)]
    pub consent_text: Option<String>,
}

fn default_locale() -> String {
    "fr".into()
}
fn default_source() -> String {
    "footer".into()
}

/// What every call to `subscribe` answers, whatever it found.
const SAME_ANSWER: &str = "If that address can receive mail, a confirmation link is on its way.";

fn opaque_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Cheap shape check. Deliverability is decided by the confirmation mail
/// arriving, not by a regular expression.
fn looks_like_an_address(email: &str) -> bool {
    let at = email.find('@');
    match at {
        Some(i) => {
            let (local, domain) = email.split_at(i);
            !local.is_empty()
                && domain.len() > 3
                && domain.contains('.')
                && !domain.ends_with('.')
                && !email.contains(' ')
                && email.len() <= 320
        }
        None => false,
    }
}

/// POST /api/newsletter/subscriptions
#[utoipa::path(
    post, path = "/api/newsletter/subscriptions", tag = "newsletter",
    request_body = SubscribeBody,
    responses(
        (status = 202, description = "Accepted. The same answer whether or not the address was known."),
        (status = 400, description = "The address is not shaped like one, or the locale is not served", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Too many attempts from this IP or for this address", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    let email = body.email.trim().to_lowercase();
    if !looks_like_an_address(&email) {
        return Err(AppError::Validation("that is not an email address".into()));
    }
    if !matches!(body.locale.as_str(), "fr" | "en" | "ar") {
        return Err(AppError::Validation(
            "locale must be one of fr, en, ar".into(),
        ));
    }

    let mut redis = state.redis.clone();
    let ip = extract_ip(&headers);

    // Per IP, and then per address.
    //
    // The second limit is the one that matters and it is easy to forget: with
    // only an IP limit, this endpoint is a way to mail somebody repeatedly by
    // typing their address from a handful of machines. The address limit is
    // deliberately tighter, because a person subscribing twice in an hour is
    // already unusual.
    RateLimiter::check(&mut redis, "newsletter_ip", &ip, 10, 3600).await?;
    RateLimiter::check(&mut redis, "newsletter_address", &email, 3, 3600).await?;

    let token = opaque_token();
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(500).collect::<String>());

    // One statement for all four cases.
    //
    // A new address inserts. A pending one gets a fresh token, because the
    // first mail may never have arrived. An unsubscribed one is asked to
    // confirm again rather than silently resurrected: leaving is a decision,
    // and undoing it needs the same click that made it in the first place.
    //
    // A confirmed one is deliberately left alone by the DO UPDATE guard below,
    // so a re-subscribe cannot reset somebody's existing state, and the caller
    // still gets the same answer.
    let row: Option<(String, String)> = sqlx::query_as(
        r#"
        INSERT INTO newsletter_subscriptions
            (email, locale, source, consent_text, consent_ip, consent_user_agent,
             confirm_token, confirm_token_expires_at, unsubscribe_token)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW() + INTERVAL '7 days', $8)
        ON CONFLICT (email) DO UPDATE
           SET confirm_token = EXCLUDED.confirm_token,
               confirm_token_expires_at = EXCLUDED.confirm_token_expires_at,
               locale = EXCLUDED.locale,
               consent_text = EXCLUDED.consent_text,
               consent_ip = EXCLUDED.consent_ip,
               consent_user_agent = EXCLUDED.consent_user_agent
         WHERE newsletter_subscriptions.status <> 'confirmed'
        RETURNING confirm_token, locale
        "#,
    )
    .bind(&email)
    .bind(&body.locale)
    .bind(&body.source)
    .bind(body.consent_text.as_deref())
    .bind(if ip.is_empty() { None } else { Some(&ip) })
    .bind(user_agent.as_deref())
    .bind(&token)
    .bind(opaque_token())
    .fetch_optional(&state.db)
    .await?;

    // `None` means the address was already confirmed and the WHERE clause
    // refused the update. Nothing is sent, and the caller cannot tell.
    if let Some((confirm_token, locale)) = row {
        let base = state.config.frontend_url.trim_end_matches('/');
        let link = format!("{base}/newsletter/confirm/{confirm_token}");
        let (subject, html) = confirmation_mail(&locale, &link);

        // Best effort, and the caller is not told either way. A mail provider
        // being down is not something the person typing their address can act
        // on, and reporting it would also report that the address was new.
        if let Err(err) = state.email.send_direct(&email, "", &subject, &html).await {
            tracing::error!(error = %err, "newsletter confirmation mail failed to send");
            sentry::capture_error(&err);
        }
    }

    Ok(Json(build_response(json!({ "message": SAME_ANSWER }))))
}

/// The confirmation mail, in the locale the form was in.
fn confirmation_mail(locale: &str, link: &str) -> (String, String) {
    match locale {
        "en" => (
            "Skilluv: confirm your subscription".to_string(),
            format!(
                "<p>Somebody, we hope you, asked for the Skilluv newsletter at this address.</p>\
                 <p><a href=\"{link}\">Confirm the subscription</a></p>\
                 <p>If it was not you, ignore this message. Nothing was subscribed and \
                 nothing else will be sent.</p>"
            ),
        ),
        "ar" => (
            "Skilluv: أكد اشتراكك".to_string(),
            format!(
                "<p>طلب أحدهم, نأمل أنك أنت, نشرة Skilluv على هذا العنوان.</p>\
                 <p><a href=\"{link}\">تأكيد الاشتراك</a></p>\
                 <p>إن لم تكن أنت, تجاهل هذه الرسالة. لم يتم تسجيل أي اشتراك.</p>"
            ),
        ),
        _ => (
            "Skilluv : confirme ton inscription".to_string(),
            format!(
                "<p>Quelqu'un, on espère toi, a demandé la newsletter Skilluv à cette adresse.</p>\
                 <p><a href=\"{link}\">Confirmer l'inscription</a></p>\
                 <p>Si ce n'est pas toi, ignore ce message. Rien n'est inscrit et rien \
                 d'autre ne sera envoyé.</p>"
            ),
        ),
    }
}

/// GET /api/newsletter/confirm/{token}
#[utoipa::path(
    get, path = "/api/newsletter/confirm/{token}", tag = "newsletter",
    params(("token" = String, Path, description = "Opaque token from the confirmation mail")),
    responses(
        (status = 200, description = "Confirmed, or already confirmed"),
        (status = 404, description = "No such token, or it expired", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn confirm(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, AppError> {
    // The token is spent on use. A confirmation link that keeps working is a
    // link that keeps sitting in a mailbox somebody else may one day read.
    let updated: Option<(String,)> = sqlx::query_as(
        r#"
        UPDATE newsletter_subscriptions
           SET status = 'confirmed',
               confirmed_at = NOW(),
               unsubscribed_at = NULL,
               confirm_token = NULL,
               confirm_token_expires_at = NULL
         WHERE confirm_token = $1
           AND confirm_token_expires_at > NOW()
        RETURNING unsubscribe_token
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?;

    match updated {
        Some((unsubscribe_token,)) => Ok(Json(build_response(json!({
            "confirmed": true,
            // Handed over now so the page can offer a way out immediately,
            // rather than making somebody wait for the first mail to find one.
            "unsubscribe_token": unsubscribe_token,
        })))),
        None => Err(AppError::NotFound(
            "this confirmation link is not valid any more; subscribe again to get a new one".into(),
        )),
    }
}

/// GET /api/newsletter/unsubscribe/{token}
#[utoipa::path(
    get, path = "/api/newsletter/unsubscribe/{token}", tag = "newsletter",
    params(("token" = String, Path, description = "Opaque token from any newsletter mail")),
    responses(
        (status = 200, description = "Unsubscribed, or already unsubscribed"),
        (status = 404, description = "No such token", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, AppError> {
    // Idempotent, and the token survives. Clicking unsubscribe twice is not an
    // error, and a token that stopped working would leave somebody who kept an
    // old mail with no way out.
    let updated: Option<(String,)> = sqlx::query_as(
        r#"
        UPDATE newsletter_subscriptions
           SET status = 'unsubscribed',
               unsubscribed_at = COALESCE(unsubscribed_at, NOW()),
               confirmed_at = NULL,
               confirm_token = NULL,
               confirm_token_expires_at = NULL
         WHERE unsubscribe_token = $1
        RETURNING email
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?;

    match updated {
        Some(_) => Ok(Json(build_response(json!({ "unsubscribed": true })))),
        None => Err(AppError::NotFound("unknown unsubscribe link".into())),
    }
}

/// Every address the newsletter may be sent to.
///
/// This is the rule the sending code must use, and the reason it lives here
/// rather than in whatever eventually composes a newsletter: **any opt-out
/// wins**.
///
/// An address is mailable when its own row is confirmed AND, where an account
/// exists with the same address, that account has not turned the
/// `newsletter.issue` kind off and has not had mail disabled altogether.
/// Unsubscribing anywhere is then effective everywhere.
///
/// The alternative, one of the two being authoritative, means somebody who
/// clicks unsubscribe in a mail keeps receiving it because they ticked a box
/// in their settings two years ago. That is the failure this join prevents,
/// and it is a legal one as much as a courteous one.
///
/// `notification_preferences` holds a row only where somebody has expressed a
/// choice, so its absence is not a refusal: an anonymous subscriber, or an
/// account holder who never opened the settings screen, has no row and stays
/// mailable on the strength of the confirmation they clicked. What overrides
/// that is an explicit `enabled = FALSE`.
///
/// Resolved at send time by lowercased email rather than cached, so an
/// account created after the subscription is picked up without a backfill.
pub async fn mailable_addresses(db: &sqlx::PgPool) -> Result<Vec<(String, String)>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT n.email, n.locale
          FROM newsletter_subscriptions n
          LEFT JOIN users u ON lower(u.email) = n.email
         WHERE n.status = 'confirmed'
           AND COALESCE(u.email_disabled, FALSE) = FALSE
           AND NOT EXISTS (
               SELECT 1 FROM notification_preferences p
                WHERE p.user_id = u.id
                  AND p.kind = 'newsletter.issue'
                  AND p.channel = 'email'
                  AND p.enabled = FALSE
           )
         ORDER BY n.email
        "#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::looks_like_an_address;

    #[test]
    fn an_address_is_shaped_like_one() {
        for ok in [
            "a@b.co",
            "jeremie@skill-uv.com",
            "first.last+tag@sub.domain.org",
        ] {
            assert!(looks_like_an_address(ok), "{ok} should pass");
        }
        for bad in [
            "",
            "nope",
            "@nodomain.com",
            "no-at-sign.com",
            "trailing@dot.",
            "two words@example.com",
            "no-tld@localhost",
        ] {
            assert!(!looks_like_an_address(bad), "{bad} should not pass");
        }
    }
}
