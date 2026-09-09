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
    /// Whether a missing transport is allowed to be silent.
    ///
    /// TRUE only for `dev`, `test` and `local`. Anywhere else, a send with no
    /// transport configured is an error rather than a log line: see `send`.
    logs_instead_of_sending: bool,
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
    /// `environment` is `AppConfig::environment`, the same string
    /// `assert_production_secrets` reads, so that "is this a real
    /// deployment" is decided in one place and not spelled a second way here.
    pub fn new(
        api_key: Option<String>,
        from_email: &str,
        from_name: &str,
        environment: &str,
    ) -> Self {
        let smtp = build_smtp_from_env();
        let logs_instead_of_sending = matches!(environment, "dev" | "test" | "local");

        if smtp.is_some() {
            tracing::info!(
                "Email service initialized with SMTP transport ({})",
                env::var("SMTP_HOST").unwrap_or_default()
            );
        } else if api_key.is_some() {
            tracing::info!("Email service initialized with Brevo API");
        } else if logs_instead_of_sending {
            tracing::warn!(
                "Email service in dev mode (logging only, no SMTP_HOST or BREVO_API_KEY)"
            );
        } else {
            tracing::error!(
                environment = environment,
                "No SMTP_HOST and no BREVO_API_KEY on a deployment that serves real                  people. Every send will fail loudly rather than be dropped quietly.                  Nothing that depends on mail works until one of them is set:                  email verification, password reset, invitations, the newsletter                  confirmation link."
            );
        }
        Self {
            api_key,
            smtp,
            from_email: from_email.to_string(),
            from_name: from_name.to_string(),
            logs_instead_of_sending,
        }
    }

    /// Direct send without `email_log` bookkeeping (used when the recipient has no
    /// user row yet - e.g. magic-link signup).
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
            // No transport at all.
            //
            // This used to log `[DEV] Email would be sent` and return `Ok(())`
            // in every environment, which made an unconfigured provider in
            // production indistinguishable from a successful send: the caller
            // got its success, the code got its `Ok`, and nothing anywhere
            // said a mail had been dropped. Somebody subscribing to the
            // newsletter got a 202 and never heard from us, and the platform
            // had no way to know.
            //
            // The log line stays where it is true, which is a machine with no
            // mail provider on purpose. Anywhere else it is an error, so that
            // the caller can decide and Sentry sees it.
            None if self.logs_instead_of_sending => {
                tracing::info!(
                    to = to_email,
                    subject = subject,
                    "[DEV] Email would be sent"
                );
                Ok(())
            }
            None => {
                tracing::error!(
                    to = to_email,
                    subject = subject,
                    "Mail dropped: no SMTP_HOST and no BREVO_API_KEY on this deployment"
                );
                Err(AppError::Internal(
                    "No mail transport is configured on this deployment".to_string(),
                ))
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
        let body = brevo_payload(
            &self.from_name,
            &self.from_email,
            to_email,
            to_name,
            subject,
            html_content,
        );

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
                "Brevo API error: {status} - {error_body}"
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

        // Best-effort logging - never fail the send because logging failed.
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

    /// Wraps a template body in the shared Skilluv shell - brand wordmark,
    /// consistent typography, framed card on a soft neutral background, and
    /// a footer. `preheader` is the short teaser Gmail / Outlook show under
    /// the subject in the inbox (kept hidden in the body).
    ///
    /// Values are inline styles because CSS classes are stripped or ignored
    /// by most inbox rendering engines. Font stack starts with Space Grotesk
    /// (loaded on the web app) and falls back to a robust system stack -
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
                    Skilluv © {year} - Prouve ce que tu sais faire.<br />
                    Tu reçois cet email parce qu'une action a été effectuée sur ton compte. Si ce n'est pas toi, ignore et supprime.
                </p>
            </td>
        </tr>
    </table>
</body>
</html>"#
        )
    }

    /// Standard CTA button - inline-styled so it survives Gmail / Outlook.
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
            &format!("Sécurité - {event_title}"),
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
                L'invitation expire dans 7 jours. Elle est liée à cette adresse email uniquement - connecte-toi (ou crée un compte) avec la même pour l'accepter.
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
                Le code expire dans 10 minutes. Personne de chez Skilluv ne te le demandera jamais - ne le communique à personne.
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

/// The Brevo request body, built where it can be read and tested.
///
/// `name` is omitted when it is empty rather than sent as `""`. Brevo refuses
/// the empty string with `{"code":"missing_parameter","message":"name is
/// missing in to"}`, a 400 on every send, and the whole point of
/// `send_direct` is a recipient with no user row and therefore no name to
/// give: the newsletter confirmation passes `""` because there is nobody to
/// name yet.
///
/// So every newsletter confirmation mail failed at the provider, was logged
/// and reported, and the caller still got its 202 because the failure is
/// deliberately not told to the person subscribing. Four Sentry events and an
/// endpoint that looked like it worked.
///
/// Fixed here and not at the call site: no caller should have to know that
/// this provider dislikes an empty string, and the next one to pass `""`
/// would have reintroduced it.
fn brevo_payload(
    from_name: &str,
    from_email: &str,
    to_email: &str,
    to_name: &str,
    subject: &str,
    html_content: &str,
) -> serde_json::Value {
    let mut recipient = json!({ "email": to_email });
    if !to_name.trim().is_empty() {
        recipient["name"] = json!(to_name);
    }
    json!({
        "sender": { "name": from_name, "email": from_email },
        "to": [recipient],
        "subject": subject,
        "htmlContent": html_content,
    })
}

#[cfg(test)]
mod tests {
    use super::{EmailService, brevo_payload};

    /// A recipient with no name carries no `name` key at all.
    ///
    /// Sending `"name": ""` is a 400 from Brevo on every message:
    /// `{"code":"missing_parameter","message":"name is missing in to"}`. That
    /// is what every newsletter confirmation mail hit. `send_direct` exists
    /// precisely for a recipient with no user row and therefore no name, so
    /// the empty string is the normal case for it, not an edge one.
    #[test]
    fn a_recipient_with_no_name_is_sent_without_the_field() {
        let body = brevo_payload(
            "Skilluv",
            "no-reply@skill-uv.com",
            "somebody@example.com",
            "",
            "Skilluv: confirm your subscription",
            "<p>hi</p>",
        );
        let to = &body["to"][0];
        assert_eq!(to["email"], "somebody@example.com");
        assert!(
            to.get("name").is_none(),
            "an empty name has to be absent, not empty: {body}"
        );

        // Whitespace is the same case wearing a disguise.
        let padded = brevo_payload("S", "f@x.co", "t@x.co", "   ", "s", "<p>h</p>");
        assert!(padded["to"][0].get("name").is_none());
    }

    /// And a real name still travels.
    #[test]
    fn a_named_recipient_keeps_their_name() {
        let body = brevo_payload(
            "Skilluv",
            "no-reply@skill-uv.com",
            "ama@example.com",
            "Ama",
            "Welcome",
            "<p>hi</p>",
        );
        assert_eq!(body["to"][0]["name"], "Ama");
        assert_eq!(body["sender"]["name"], "Skilluv");
    }

    /// A deployment with no mail transport says so instead of saying nothing.
    ///
    /// The `None` arm returned `Ok(())` everywhere, logging "[DEV] Email
    /// would be sent". On a real deployment that made an unconfigured
    /// provider indistinguishable from a successful send: the caller got its
    /// success, the code got its `Ok`, and a person who subscribed to the
    /// newsletter got a 202 and never heard from us.
    ///
    /// The test cannot assert a mail was sent, since sending needs a
    /// provider. It asserts the one thing that was wrong: silence.
    #[tokio::test]
    async fn a_real_deployment_without_a_transport_fails_loudly() {
        // SMTP_HOST is read from the environment by `build_smtp_from_env`, so
        // this only holds where it is unset, which is where this test runs.
        if std::env::var("SMTP_HOST").is_ok_and(|v| !v.is_empty()) {
            return;
        }

        let service = EmailService::new(None, "no-reply@skill-uv.com", "Skilluv", "prod");
        let sent = service
            .send_direct("somebody@example.com", "", "Confirm", "<p>hi</p>")
            .await;

        assert!(
            sent.is_err(),
            "a send with no transport on a real deployment has to be an error, \
             not a log line nobody reads"
        );
    }

    /// And a laptop keeps its log line.
    ///
    /// The dev behaviour is the reason the arm was written and it is correct
    /// where it is true: a machine with no provider on purpose should not
    /// fail every signup. `dev`, `test` and `local` are the three names
    /// `AppStateConfig::tolerates_test_fixtures` already uses, so there is
    /// one spelling of "not a real deployment" and not two.
    #[tokio::test]
    async fn a_development_machine_still_only_logs() {
        if std::env::var("SMTP_HOST").is_ok_and(|v| !v.is_empty()) {
            return;
        }

        for environment in ["dev", "test", "local"] {
            let service = EmailService::new(None, "no-reply@skill-uv.com", "Skilluv", environment);
            let sent = service
                .send_direct("somebody@example.com", "", "Confirm", "<p>hi</p>")
                .await;
            assert!(
                sent.is_ok(),
                "{environment} must keep logging rather than failing every send"
            );
        }
    }
}
