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

/// Paramètres pour [`EmailService::send_with_log`].
#[derive(Debug, Clone, Copy)]
pub struct SendWithLogParams<'a> {
    pub user_id: Uuid,
    pub to_email: &'a str,
    pub to_name: &'a str,
    pub subject: &'a str,
    pub html: &'a str,
    pub kind: &'a str,
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
        params: SendWithLogParams<'_>,
    ) -> Result<bool, AppError> {
        let SendWithLogParams {
            user_id,
            to_email,
            to_name,
            subject,
            html,
            kind,
        } = params;
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
        if let Err(err) =
            sqlx::query("INSERT INTO email_log (user_id, kind, subject) VALUES ($1, $2, $3)")
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

    // ─── Email shell ─────────────────────────────────────────────

    /// Wraps a template body in the shared Skilluv shell — brand wordmark,
    /// consistent typography, framed card on a soft neutral background, and
    /// a footer. `preheader` is the short teaser Gmail / Outlook show under
    /// the subject in the inbox (kept hidden in the body).
    ///
    /// Values are inline styles because CSS classes are stripped or ignored
    /// by most inbox rendering engines. Font stack starts with Space Grotesk
    /// (loaded on the web app) and falls back to a robust system stack —
    /// most clients will render with the fallback since custom fonts don't
    /// load reliably in email.
    fn shell(&self, preheader: &str, body: &str) -> String {
        // Brand tokens mirror the frontend (`app.css` :root).
        const FONT_STACK: &str = "'Space Grotesk', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Helvetica Neue', Arial, sans-serif";
        const ACCENT: &str = "#ea580c"; // forge accent
        const TEXT: &str = "#1c1917";
        const TEXT_MUTED: &str = "#78716c";
        const SURFACE: &str = "#ffffff";
        const SURFACE_BG: &str = "#fafaf9";
        const BORDER: &str = "#e7e5e4";
        let year = chrono::Utc::now().format("%Y");
        format!(
            r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width,initial-scale=1" />
</head>
<body style="margin:0;padding:32px 16px;background:{SURFACE_BG};font-family:{FONT_STACK};color:{TEXT};line-height:1.55;-webkit-font-smoothing:antialiased;">
    <div style="display:none;max-height:0;overflow:hidden;opacity:0;color:transparent;">{preheader}</div>
    <table role="presentation" cellspacing="0" cellpadding="0" border="0" width="100%" style="max-width:560px;margin:0 auto;background:{SURFACE};border:1px solid {BORDER};border-radius:16px;overflow:hidden;">
        <tr>
            <td style="padding:32px 32px 0;">
                <div style="font-size:22px;font-weight:900;letter-spacing:-0.02em;line-height:1;">
                    <span style="color:{ACCENT};">Skill</span><span style="color:{TEXT};">uv</span>
                </div>
            </td>
        </tr>
        <tr>
            <td style="padding:24px 32px 32px;font-size:15px;">
                {body}
            </td>
        </tr>
        <tr>
            <td style="padding:16px 32px 20px;border-top:1px solid {BORDER};background:{SURFACE_BG};">
                <p style="margin:0;color:{TEXT_MUTED};font-size:11px;line-height:1.4;">
                    Skilluv © {year} — Prouve ce que tu sais faire.<br />
                    Tu reçois cet email parce qu'une action a été effectuée sur ton compte. Si ce n'est pas toi, ignore et supprime.
                </p>
            </td>
        </tr>
    </table>
</body>
</html>"#
        )
    }

    /// Standard CTA button — inline-styled so it survives Gmail / Outlook.
    fn cta_button(label: &str, href: &str) -> String {
        const ACCENT: &str = "#ea580c";
        format!(
            r#"<a href="{href}" style="display:inline-block;background:{ACCENT};color:#ffffff;text-decoration:none;padding:12px 26px;border-radius:9999px;font-weight:600;font-size:14px;letter-spacing:0.02em;">{label}</a>"#
        )
    }

    // ─── Email templates ────────────────────────────────────────

    pub async fn send_email_verification(
        &self,
        email: &str,
        display_name: &str,
        token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        // Points at the frontend page (not the JSON API endpoint) so the user
        // lands on a friendly UI that calls the backend on their behalf and
        // then routes them to /auth/login. `base_url` MUST be the user-facing
        // origin (frontend dev server in dev, shared domain in prod).
        let link = format!("{base_url}/auth/verify-email?token={token}");
        let button = Self::cta_button("Confirmer mon adresse", &link);
        let body = format!(
            r#"
            <h1 style="margin:0 0 12px;font-size:24px;font-weight:800;letter-spacing:-0.01em;line-height:1.2;">
                Bienvenue, {display_name}.
            </h1>
            <p style="margin:0 0 20px;color:#44403c;">
                Il te reste une étape pour activer ton compte : confirme que cette adresse email est bien la tienne.
            </p>
            <p style="margin:24px 0;">{button}</p>
            <p style="margin:24px 0 0;color:#78716c;font-size:13px;line-height:1.5;">
                Le lien expire dans 24 heures. Tu peux aussi le copier-coller dans ton navigateur :<br />
                <span style="word-break:break-all;color:#57534e;font-size:12px;">{link}</span>
            </p>
            "#
        );
        let html = self.shell(
            "Confirme ton adresse email pour activer ton compte Skilluv.",
            &body,
        );
        self.send(email, display_name, "Confirme ton adresse email", &html)
            .await
    }

    pub async fn send_password_reset(
        &self,
        email: &str,
        display_name: &str,
        token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        // Same rule as verify-email: link to the frontend page, not the API.
        let link = format!("{base_url}/auth/reset-password?token={token}");
        let button = Self::cta_button("Choisir un nouveau mot de passe", &link);
        let body = format!(
            r#"
            <h1 style="margin:0 0 12px;font-size:24px;font-weight:800;letter-spacing:-0.01em;line-height:1.2;">
                Réinitialisation de ton mot de passe
            </h1>
            <p style="margin:0 0 20px;color:#44403c;">
                Salut {display_name}, on a bien reçu ta demande. Choisis un nouveau mot de passe en cliquant sur le bouton ci-dessous.
            </p>
            <p style="margin:24px 0;">{button}</p>
            <p style="margin:24px 0 0;color:#78716c;font-size:13px;line-height:1.5;">
                Ce lien est valable 1 heure et à usage unique. Si tu n'as pas fait cette demande, ignore cet email : ton mot de passe actuel reste inchangé.
            </p>
            "#
        );
        let html = self.shell(
            "Un lien pour choisir un nouveau mot de passe Skilluv.",
            &body,
        );
        self.send(
            email,
            display_name,
            "Réinitialisation de ton mot de passe",
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
        let body = format!(
            r#"
            <h1 style="margin:0 0 12px;font-size:22px;font-weight:800;letter-spacing:-0.01em;line-height:1.2;">
                Activité de sécurité sur ton compte
            </h1>
            <p style="margin:0 0 8px;color:#44403c;">Salut {display_name},</p>
            <p style="margin:0 0 8px;font-weight:600;color:#1c1917;">{event_title}</p>
            <p style="margin:0 0 20px;color:#44403c;">{event_detail}</p>
            <div style="border-left:3px solid #ea580c;padding:10px 14px;background:#fff7ed;color:#9a3412;font-size:13px;border-radius:0 8px 8px 0;">
                Si ce n'est pas toi, change ton mot de passe immédiatement et révoque toutes tes sessions depuis <strong>Paramètres → Sécurité</strong>.
            </div>
            "#
        );
        let html = self.shell(event_title, &body);
        self.send(
            email,
            display_name,
            &format!("Sécurité — {event_title}"),
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
        // /auth/invite/{token} is the frontend landing page that offers OAuth
        // signup for the invited email + a link to the standard accept flow
        // for existing users. The frontend enforces the email-match check
        // client-side, the backend enforces it again server-side on accept.
        let link = format!("{base_url}/auth/invite/{token}");
        let button = Self::cta_button("Rejoindre l'équipe", &link);
        let body = format!(
            r#"
            <h1 style="margin:0 0 12px;font-size:24px;font-weight:800;letter-spacing:-0.01em;line-height:1.2;">
                {company_name} t'invite à recruter avec eux
            </h1>
            <p style="margin:0 0 12px;color:#44403c;">
                Une place de recruteur t'est réservée dans l'espace <strong>{company_name}</strong> sur Skilluv. En acceptant, tu pourras sourcer des talents vérifiés par leurs performances, ouvrir des conversations et gérer les crédits partagés avec ton équipe.
            </p>
            <p style="margin:24px 0;">{button}</p>
            <p style="margin:24px 0 0;color:#78716c;font-size:13px;line-height:1.5;">
                L'invitation expire dans 7 jours. Elle est liée à cette adresse email uniquement — connecte-toi (ou crée un compte) avec la même pour l'accepter.
            </p>
            "#
        );
        let html = self.shell(
            &format!("{company_name} t'invite à rejoindre son équipe de recrutement sur Skilluv."),
            &body,
        );
        self.send(
            email,
            company_name,
            &format!("Invitation : rejoins l'équipe de {company_name}"),
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
        let body = format!(
            r#"
            <h1 style="margin:0 0 12px;font-size:22px;font-weight:800;letter-spacing:-0.01em;line-height:1.2;">
                Ton code de vérification
            </h1>
            <p style="margin:0 0 20px;color:#44403c;">
                Salut {display_name}, saisis ce code dans la fenêtre de connexion pour finaliser ton accès.
            </p>
            <div style="text-align:center;margin:28px 0;">
                <div style="display:inline-block;padding:16px 28px;background:#fafaf9;border:1px solid #e7e5e4;border-radius:12px;font-family:'JetBrains Mono',SFMono-Regular,Consolas,monospace;font-size:30px;font-weight:700;letter-spacing:0.5em;color:#1c1917;">
                    {code}
                </div>
            </div>
            <p style="margin:24px 0 0;color:#78716c;font-size:13px;line-height:1.5;">
                Le code expire dans 10 minutes. Personne de chez Skilluv ne te le demandera jamais — ne le communique à personne.
            </p>
            "#
        );
        let html = self.shell(&format!("Code de vérification Skilluv : {code}"), &body);
        self.send(email, display_name, "Ton code de vérification", &html)
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
