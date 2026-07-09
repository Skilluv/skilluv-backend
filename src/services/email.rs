use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde_json::json;
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

use crate::errors::AppError;

const BREVO_API_URL: &str = "https://api.brevo.com/v3/smtp/email";

pub struct EmailService {
    api_key: Option<String>,
    smtp: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: String,
    from_name: String,
}

impl EmailService {
    pub fn new(api_key: Option<String>, from_email: &str, from_name: &str) -> Self {
        let smtp = build_smtp_from_env();

        if smtp.is_some() {
            tracing::info!(
                "Email service initialized with SMTP transport ({})",
                env::var("SMTP_HOST").unwrap_or_default()
            );
        } else if api_key.is_some() {
            tracing::info!("Email service initialized with Brevo API");
        } else {
            tracing::warn!(
                "Email service in dev mode (logging only, no SMTP_HOST or BREVO_API_KEY)"
            );
        }
        Self {
            api_key,
            smtp,
            from_email: from_email.to_string(),
            from_name: from_name.to_string(),
        }
    }

    /// Direct send without `email_log` bookkeeping (used when the recipient has no
    /// user row yet — e.g. magic-link signup).
    pub async fn send_direct(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_content: &str,
    ) -> Result<(), AppError> {
        self.send(to_email, to_name, subject, html_content).await
    }

    async fn send(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_content: &str,
    ) -> Result<(), AppError> {
        if let Some(smtp) = &self.smtp {
            return self
                .send_smtp(smtp, to_email, to_name, subject, html_content)
                .await;
        }
        match &self.api_key {
            Some(key) => {
                self.send_brevo(key, to_email, to_name, subject, html_content)
                    .await
            }
            None => {
                tracing::info!(
                    to = to_email,
                    subject = subject,
                    "[DEV] Email would be sent"
                );
                Ok(())
            }
        }
    }

    async fn send_smtp(
        &self,
        smtp: &AsyncSmtpTransport<Tokio1Executor>,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_content: &str,
    ) -> Result<(), AppError> {
        let from: Mailbox = format!("{} <{}>", self.from_name, self.from_email)
            .parse()
            .map_err(|e| AppError::Internal(format!("Invalid EMAIL_FROM: {e}")))?;
        let to: Mailbox = format!("{to_name} <{to_email}>")
            .parse()
            .map_err(|e| AppError::Validation(format!("Invalid recipient: {e}")))?;

        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(strip_html(html_content)),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_content.to_string()),
                    ),
            )
            .map_err(|e| AppError::Internal(format!("Email build failed: {e}")))?;

        smtp.send(message)
            .await
            .map_err(|e| AppError::Internal(format!("SMTP send failed: {e}")))?;
        Ok(())
    }

    async fn send_brevo(
        &self,
        api_key: &str,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_content: &str,
    ) -> Result<(), AppError> {
        let body = json!({
            "sender": {
                "name": self.from_name,
                "email": self.from_email,
            },
            "to": [{
                "email": to_email,
                "name": to_name,
            }],
            "subject": subject,
            "htmlContent": html_content,
        });

        let client = reqwest::Client::new();
        let response = client
            .post(BREVO_API_URL)
            .header("api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to send email via Brevo: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            tracing::error!(
                status = %status,
                body = %error_body,
                to = to_email,
                "Brevo API error"
            );
            return Err(AppError::Internal(format!(
                "Brevo API error: {status} — {error_body}"
            )));
        }

        tracing::info!(to = to_email, subject = subject, "Email sent via Brevo");
        Ok(())
    }

    /// Send with bounce-aware gating + logging in `email_log`.
    ///
    /// Returns `Ok(false)` if the email was suppressed because the user is `email_disabled`
    /// (hard bounce previously, or unsubscribed all). Returns `Ok(true)` if delivered to the
    /// provider successfully.
    pub async fn send_with_log(
        &self,
        db: &PgPool,
        user_id: Uuid,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html: &str,
        kind: &str,
    ) -> Result<bool, AppError> {
        // Bail if the user has hard-bounced or globally disabled emails
        let disabled: Option<(bool,)> =
            sqlx::query_as("SELECT email_disabled FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        if matches!(disabled, Some((true,))) {
            tracing::debug!(user_id = %user_id, kind, "email skipped: user.email_disabled");
            return Ok(false);
        }

        self.send(to_email, to_name, subject, html).await?;

        // Best-effort logging — never fail the send because logging failed.
        if let Err(err) = sqlx::query(
            "INSERT INTO email_log (user_id, kind, subject) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(kind)
        .bind(subject)
        .execute(db)
        .await
        {
            tracing::warn!(error = %err, "failed to log email_log row");
        }
        Ok(true)
    }

    // ─── Email templates ────────────────────────────────────────

    pub async fn send_email_verification(
        &self,
        email: &str,
        display_name: &str,
        token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        let link = format!("{base_url}/api/auth/verify-email?token={token}");
        let html = format!(
            r#"
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2 style="color: #1a1a2e;">Bienvenue sur Skilluv, {display_name} !</h2>
                <p>Confirme ton adresse email pour activer ton compte :</p>
                <p style="text-align: center; margin: 30px 0;">
                    <a href="{link}" style="background-color: #6c5ce7; color: white; padding: 14px 28px; text-decoration: none; border-radius: 8px; font-weight: bold;">
                        Verifier mon email
                    </a>
                </p>
                <p style="color: #666; font-size: 12px;">Ce lien expire dans 24 heures. Si tu n'as pas cree de compte, ignore cet email.</p>
            </div>
            "#
        );

        self.send(email, display_name, "Skilluv — Verifie ton email", &html)
            .await
    }

    pub async fn send_password_reset(
        &self,
        email: &str,
        display_name: &str,
        token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        let link = format!("{base_url}/reset-password?token={token}");
        let html = format!(
            r#"
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2 style="color: #1a1a2e;">Reinitialisation de mot de passe</h2>
                <p>Salut {display_name}, tu as demande a reinitialiser ton mot de passe :</p>
                <p style="text-align: center; margin: 30px 0;">
                    <a href="{link}" style="background-color: #6c5ce7; color: white; padding: 14px 28px; text-decoration: none; border-radius: 8px; font-weight: bold;">
                        Reinitialiser mon mot de passe
                    </a>
                </p>
                <p style="color: #666; font-size: 12px;">Ce lien expire dans 1 heure. Si tu n'as pas fait cette demande, ignore cet email.</p>
            </div>
            "#
        );

        self.send(
            email,
            display_name,
            "Skilluv — Reinitialisation de mot de passe",
            &html,
        )
        .await
    }

    /// Generic security-notification email (password changed, 2FA toggled, etc.).
    pub async fn send_security_alert(
        &self,
        email: &str,
        display_name: &str,
        event_title: &str,
        event_detail: &str,
    ) -> Result<(), AppError> {
        let html = format!(
            r#"
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2 style="color: #1a1a2e;">🔒 Alerte de sécurité</h2>
                <p>Salut {display_name},</p>
                <p><strong>{event_title}</strong></p>
                <p style="color: #444;">{event_detail}</p>
                <p style="color: #666; font-size: 12px; margin-top: 24px;">
                    Si ce n'est pas toi, change immédiatement ton mot de passe et contacte notre support.
                </p>
            </div>
            "#
        );
        self.send(
            email,
            display_name,
            &format!("Skilluv — {event_title}"),
            &html,
        )
        .await
    }

    pub async fn send_recruiter_invite(
        &self,
        email: &str,
        company_name: &str,
        token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        let link = format!("{base_url}/enterprise/invite?token={token}");
        let html = format!(
            r#"
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2 style="color: #1a1a2e;">Invitation recruteur</h2>
                <p>L'entreprise <strong>{company_name}</strong> t'invite a rejoindre son equipe de recrutement sur Skilluv.</p>
                <p style="text-align: center; margin: 30px 0;">
                    <a href="{link}" style="background-color: #6c5ce7; color: white; padding: 14px 28px; text-decoration: none; border-radius: 8px; font-weight: bold;">
                        Accepter l'invitation
                    </a>
                </p>
                <p style="color: #666; font-size: 12px;">Cette invitation expire dans 7 jours.</p>
            </div>
            "#
        );

        self.send(
            email,
            company_name,
            &format!("Skilluv — Invitation recruteur de {company_name}"),
            &html,
        )
        .await
    }

    pub async fn send_email_2fa_code(
        &self,
        email: &str,
        display_name: &str,
        code: &str,
    ) -> Result<(), AppError> {
        let html = format!(
            r#"
            <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <h2 style="color: #1a1a2e;">Code de verification</h2>
                <p>Salut {display_name}, voici ton code de verification :</p>
                <p style="text-align: center; margin: 30px 0;">
                    <span style="background-color: #f0f0f0; padding: 16px 32px; font-size: 32px; font-weight: bold; letter-spacing: 8px; border-radius: 8px; font-family: monospace;">
                        {code}
                    </span>
                </p>
                <p style="color: #666; font-size: 12px;">Ce code expire dans 10 minutes. Ne le partage avec personne.</p>
            </div>
            "#
        );

        self.send(email, display_name, "Skilluv — Code de verification", &html)
            .await
    }
}

/// Build an async SMTP transport from env if `SMTP_HOST` is set.
/// Vars: `SMTP_HOST` (required), `SMTP_PORT` (default 1025), `SMTP_USER`, `SMTP_PASSWORD`,
/// `SMTP_TLS` (`starttls` | `implicit` | `none`, default `none` for local Mailpit).
fn build_smtp_from_env() -> Option<AsyncSmtpTransport<Tokio1Executor>> {
    let host = env::var("SMTP_HOST").ok()?;
    let port: u16 = env::var("SMTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1025);
    let tls_mode = env::var("SMTP_TLS")
        .unwrap_or_else(|_| "none".to_string())
        .to_lowercase();

    let mut builder = match tls_mode.as_str() {
        "implicit" => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .ok()?
            .port(port),
        "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .ok()?
            .port(port),
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host).port(port),
    };

    if let (Ok(user), Ok(pass)) = (env::var("SMTP_USER"), env::var("SMTP_PASSWORD")) {
        builder = builder.credentials(Credentials::new(user, pass));
    }

    Some(builder.build())
}

/// Minimal HTML → text fallback for the multipart/alternative plain part.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
