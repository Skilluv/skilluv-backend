//! Drip email sequences (Phase 3.15).
//!
//! Onboarding + retention sequences. Triggered hourly by a background task ; each
//! send is recorded in `email_log` so we never send the same `kind` to the same user
//! twice (idempotency).

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::EmailService;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct DripRunReport {
    pub sequences_evaluated: usize,
    pub emails_sent: usize,
    pub emails_skipped_already_sent: usize,
    pub emails_skipped_no_match: usize,
    pub failures: usize,
}

pub fn start_drip_task(db: PgPool, email: Arc<EmailService>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;
            if let Err(err) = run_all(&db, &email).await {
                tracing::warn!(error = %err, "drip sequences run failed");
            }
        }
    });
}

pub async fn run_all(db: &PgPool, email: &EmailService) -> Result<DripRunReport, AppError> {
    let mut report = DripRunReport {
        sequences_evaluated: 0,
        emails_sent: 0,
        emails_skipped_already_sent: 0,
        emails_skipped_no_match: 0,
        failures: 0,
    };

    for seq in talent_sequences() {
        report.sequences_evaluated += 1;
        if let Err(err) = run_sequence(db, email, &seq, &mut report).await {
            tracing::warn!(seq = seq.kind, error = %err, "drip sequence failed");
            report.failures += 1;
        }
    }
    for seq in enterprise_sequences() {
        report.sequences_evaluated += 1;
        if let Err(err) = run_enterprise_sequence(db, email, &seq, &mut report).await {
            tracing::warn!(seq = seq.kind, error = %err, "drip sequence failed");
            report.failures += 1;
        }
    }
    Ok(report)
}

struct TalentSeq {
    /// `email_log.kind` value, also acts as dedup key.
    kind: &'static str,
    delay_min_days: i64,
    delay_max_days: i64,
    require_inactive: bool,
    subject: &'static str,
    /// Returns the HTML body for the user. Receives display_name + base_url.
    render: fn(&str, &str) -> String,
}

fn talent_sequences() -> Vec<TalentSeq> {
    vec![
        TalentSeq {
            kind: "drip_talent_d1_activate",
            delay_min_days: 1,
            delay_max_days: 2,
            require_inactive: true,
            subject: "Skilluv — tu n'as pas encore essayé un challenge",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>Salut {name},</h2>
<p>Tu as créé ton compte hier, mais tu n'as pas encore lancé ton premier challenge. Voici 3 pour démarrer :</p>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/challenges?domain=code" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Choisir mon challenge</a>
</p>
<p style="color:#666;font-size:12px;">5 minutes suffisent. Tu gagnes des fragments dès le premier réussi.</p>
</div>"#
                )
            },
        },
        TalentSeq {
            kind: "drip_talent_d3_join_guild",
            delay_min_days: 3,
            delay_max_days: 4,
            require_inactive: false,
            subject: "Skilluv — rejoins une guilde",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>Salut {name},</h2>
<p>Tu progresses bien. Pour aller plus loin : rejoins une guilde. Tu y gagnes plus de fragments, des coéquipiers, et tu peux participer aux Guild Wars.</p>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/guilds" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Explorer les guildes</a>
</p>
</div>"#
                )
            },
        },
        TalentSeq {
            kind: "drip_talent_d14_silent",
            delay_min_days: 14,
            delay_max_days: 15,
            require_inactive: true,
            subject: "Skilluv — on t'attend",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>{name}, ça fait 2 semaines.</h2>
<p>Plusieurs nouveaux challenges et features sociales depuis ta dernière visite. Tu reprends quand tu veux :</p>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/dashboard" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Mon tableau de bord</a>
</p>
</div>"#
                )
            },
        },
        TalentSeq {
            kind: "drip_talent_d30_last_chance",
            delay_min_days: 30,
            delay_max_days: 31,
            require_inactive: true,
            subject: "Skilluv — avant qu'on te perde",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>{name}, dernière chance.</h2>
<p>Tu ne t'es pas reconnecté·e depuis 30 jours. On va arrêter de t'envoyer des emails sauf le digest hebdo, jusqu'à ton retour.</p>
<p>Si tu veux nous dire pourquoi tu n'es pas revenu·e, réponds à ce mail — on lit tout.</p>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/dashboard" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Revenir maintenant</a>
</p>
</div>"#
                )
            },
        },
    ]
}

async fn run_sequence(
    db: &PgPool,
    email: &EmailService,
    seq: &TalentSeq,
    report: &mut DripRunReport,
) -> Result<(), AppError> {
    let since_min = Utc::now() - ChronoDuration::days(seq.delay_max_days);
    let since_max = Utc::now() - ChronoDuration::days(seq.delay_min_days);
    let candidates: Vec<(Uuid, String, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"
        SELECT u.id, u.email, u.display_name,
               (SELECT MAX(evaluated_at) FROM challenge_submissions cs WHERE cs.user_id = u.id) AS last_activity
        FROM users u
        LEFT JOIN user_email_preferences p ON p.user_id = u.id
        WHERE u.email_disabled = FALSE
          AND u.is_banned = FALSE
          AND COALESCE(p.marketing, FALSE) = TRUE
          AND u.created_at BETWEEN $1 AND $2
          AND NOT EXISTS (
              SELECT 1 FROM email_log el WHERE el.user_id = u.id AND el.kind = $3
          )
        LIMIT 500
        "#,
    )
    .bind(since_min)
    .bind(since_max)
    .bind(seq.kind)
    .fetch_all(db)
    .await?;

    for (user_id, user_email, display_name, last_activity) in candidates {
        if seq.require_inactive {
            let recently_active = last_activity
                .map(|d| (Utc::now() - d) < ChronoDuration::days(seq.delay_min_days))
                .unwrap_or(false);
            if recently_active {
                report.emails_skipped_no_match += 1;
                continue;
            }
        }
        let html = (seq.render)(&display_name, "https://skilluv.com");
        match email
            .send_with_log(
                db,
                user_id,
                &user_email,
                &display_name,
                seq.subject,
                &html,
                seq.kind,
            )
            .await
        {
            Ok(true) => report.emails_sent += 1,
            Ok(false) => report.emails_skipped_already_sent += 1,
            Err(_) => report.failures += 1,
        }
    }
    Ok(())
}

struct EntSeq {
    kind: &'static str,
    delay_min_days: i64,
    delay_max_days: i64,
    require_no_credit_use: bool,
    subject: &'static str,
    render: fn(&str, &str) -> String,
}

fn enterprise_sequences() -> Vec<EntSeq> {
    vec![
        EntSeq {
            kind: "drip_ent_d1_welcome",
            delay_min_days: 1,
            delay_max_days: 2,
            require_no_credit_use: true,
            subject: "Skilluv — Comment trouver le talent parfait",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>Salut {name},</h2>
<p>Tu as 1 crédit gratuit qui t'attend. Voici comment l'utiliser efficacement :</p>
<ol>
<li>Filtre les talents par <strong>domaine, ville, niveau</strong></li>
<li>Regarde la <strong>preuve de skill</strong> (challenges complétés, GitHub, projets)</li>
<li>Envoie ta demande avec un message personnalisé</li>
</ol>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/enterprise/talents" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Rechercher des talents</a>
</p>
</div>"#
                )
            },
        },
        EntSeq {
            kind: "drip_ent_d3_demo",
            delay_min_days: 3,
            delay_max_days: 4,
            require_no_credit_use: true,
            subject: "Skilluv — On peut t'aider à matcher ?",
            render: |name, _base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>Bonjour {name},</h2>
<p>Tu n'as pas encore contacté de talent. Si tu veux qu'on te propose 3 profils sélectionnés à la main pour ton besoin, réponds à ce mail avec :</p>
<ul><li>Stack tech / domaine recherché</li><li>Niveau (junior/mid/senior)</li><li>Type de contrat (CDI/freelance/etc.)</li></ul>
<p>On revient sous 24h avec une short-list.</p>
</div>"#
                )
            },
        },
        EntSeq {
            kind: "drip_ent_d7_value_education",
            delay_min_days: 7,
            delay_max_days: 8,
            require_no_credit_use: false,
            subject: "Skilluv — Comment maximiser ton ROI",
            render: |name, base| {
                format!(
                    r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>{name},</h2>
<p>Trois leviers pour multiplier tes embauches sur Skilluv :</p>
<ol>
<li><strong>Packs de crédits</strong> : 5 = -13%, 20 = -23%, 100 = -36%</li>
<li><strong>Sponsored challenges</strong> : crée un challenge brandé, accès direct aux soumissions</li>
<li><strong>Pipeline kanban</strong> : track tes candidats sans Excel</li>
</ol>
<p style="text-align:center;margin:30px 0;">
  <a href="{base}/enterprise/pricing" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Voir les packs</a>
</p>
</div>"#
                )
            },
        },
    ]
}

async fn run_enterprise_sequence(
    db: &PgPool,
    email: &EmailService,
    seq: &EntSeq,
    report: &mut DripRunReport,
) -> Result<(), AppError> {
    let since_min = Utc::now() - ChronoDuration::days(seq.delay_max_days);
    let since_max = Utc::now() - ChronoDuration::days(seq.delay_min_days);
    // Enterprise primary user = founder = first enterprise_members row created
    let candidates: Vec<(Uuid, String, String, i32)> = sqlx::query_as(
        r#"
        SELECT u.id, u.email, u.display_name,
               COALESCE(ec.total_used, 0)::INT AS credits_used_count
        FROM enterprises e
        JOIN enterprise_members em ON em.enterprise_id = e.id AND em.status = 'active'
        JOIN users u ON u.id = em.user_id
        LEFT JOIN enterprise_credits ec ON ec.enterprise_id = e.id
        WHERE u.email_disabled = FALSE
          AND u.is_banned = FALSE
          AND e.created_at BETWEEN $1 AND $2
          AND NOT EXISTS (
              SELECT 1 FROM email_log el WHERE el.user_id = u.id AND el.kind = $3
          )
        LIMIT 300
        "#,
    )
    .bind(since_min)
    .bind(since_max)
    .bind(seq.kind)
    .fetch_all(db)
    .await?;

    for (user_id, user_email, display_name, credits_used) in candidates {
        if seq.require_no_credit_use && credits_used > 0 {
            report.emails_skipped_no_match += 1;
            continue;
        }
        let html = (seq.render)(&display_name, "https://skilluv.com");
        match email
            .send_with_log(
                db,
                user_id,
                &user_email,
                &display_name,
                seq.subject,
                &html,
                seq.kind,
            )
            .await
        {
            Ok(true) => report.emails_sent += 1,
            Ok(false) => report.emails_skipped_already_sent += 1,
            Err(_) => report.failures += 1,
        }
    }
    Ok(())
}
