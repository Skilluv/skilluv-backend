//! skilluv-discord-bot (SKI-116) — v2 gateway bot.
//!
//! Supersedes the v1 skilluv-discord-notifier by adding what a webhook
//! process cannot do:
//!   * welcome DM to new members
//!   * slash commands (`/skilluv`, `/skilluv verify`, `/skilluv profil`)
//!   * account linking (Discord user -> Skilluv account)
//!
//! Still handles the notification queue too, posting via the bot user
//! rather than channel webhooks. This means only ONE Coolify app is
//! needed for all Discord surfaces (queue + interactivity), and the
//! queue's producer contract (rows in discord_notifications_queue)
//! is unchanged from v1 — the backend keeps enqueuing the same events.
//!
//! Deploy as a long-running Coolify app pointing at this binary. The
//! gateway keeps a WebSocket connection open to Discord; if the process
//! dies Coolify restarts it and Discord reconnects.

use anyhow::{Context as _, Result};
use serde_json::Value;
use serenity::Client;
use serenity::all::{
    ActivityData, ChannelId, CommandDataOptionValue, CommandInteraction, CommandOptionType,
    Context, CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, EventHandler, GatewayIntents, GuildId, Http,
    Interaction, Member, Ready,
};
use serenity::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

const QUEUE_POLL_SECONDS: u64 = 15;
const MAX_FAILED_ATTEMPTS: i16 = 10;

struct Handler {
    db: PgPool,
    guild_id: GuildId,
    promotions_channel: ChannelId,
    annonces_channel: ChannelId,
    frontend_url: String,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        tracing::info!(
            bot = %ready.user.name,
            guild = %self.guild_id,
            "bot connected to Discord gateway"
        );

        // Set an activity so the bot's presence carries some intent.
        ctx.set_activity(Some(ActivityData::watching("skill-uv.com")));

        // Register slash commands scoped to our single guild — instant
        // availability vs. up-to-one-hour propagation for global commands.
        let cmds = vec![
            CreateCommand::new("skilluv")
                .description("Skilluv commands")
                .add_option(CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "me",
                    "Show your linked Skilluv profile",
                ))
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "verify",
                        "Verify a Skilluv attestation by its hash",
                    )
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "hash",
                            "The attestation hash (from the /verify URL)",
                        )
                        .required(true),
                    ),
                )
                .add_option(CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "help",
                    "How to use the bot",
                )),
        ];
        if let Err(e) = self.guild_id.set_commands(&ctx.http, cmds).await {
            tracing::error!(error = %e, "failed to register slash commands");
        } else {
            tracing::info!(guild = %self.guild_id, "slash commands registered");
        }

        // Kick off the queue poller in a background task. It shares the
        // Http via Arc so we can send messages independently of an
        // incoming event.
        let http = ctx.http.clone();
        let db = self.db.clone();
        let promotions = self.promotions_channel;
        let annonces = self.annonces_channel;
        let frontend = self.frontend_url.clone();
        tokio::spawn(async move {
            queue_poll_loop(http, db, promotions, annonces, frontend).await;
        });
    }

    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        if new_member.user.bot {
            return; // don't DM other bots
        }
        let msg = format!(
            "Welcome to Skilluv, **{}** !\n\n\
             This is the community around <{frontend}> — a compagnonnage \
             platform where open source contributions become verifiable \
             attestations.\n\n\
             Getting started:\n\
             - Post an intro in your favorite channel\n\
             - Try `/skilluv help` here in DM or on the server\n\
             - Sign up on <{frontend}> and reply here with your username \
             if you'd like your Discord tied to your profile\n\n\
             See you around !",
            new_member.user.name,
            frontend = self.frontend_url,
        );
        // Best-effort — a user with DMs disabled just doesn't get one.
        if let Err(e) = new_member
            .user
            .direct_message(&ctx.http, CreateMessage::new().content(msg))
            .await
        {
            tracing::info!(
                user = %new_member.user.name, error = %e,
                "welcome DM refused (user has DMs off)"
            );
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(cmd) = interaction else {
            return; // ignore autocomplete / component / modal for now
        };
        if let Err(e) = self.handle_command(&ctx, &cmd).await {
            tracing::warn!(cmd = %cmd.data.name, error = %e, "slash command failed");
            let _ = cmd
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!("Something went wrong: `{e}`"))
                            .ephemeral(true),
                    ),
                )
                .await;
        }
    }
}

impl Handler {
    async fn handle_command(&self, ctx: &Context, cmd: &CommandInteraction) -> Result<()> {
        if cmd.data.name != "skilluv" {
            return Ok(());
        }
        // The single top-level `skilluv` command has subcommands.
        let sub = cmd.data.options.first().context("no subcommand provided")?;
        let content = match sub.name.as_str() {
            "me" => self.handle_me(cmd).await?,
            "verify" => {
                let hash = extract_string(sub, "hash").context("missing hash arg")?;
                self.handle_verify(&hash).await?
            }
            "help" => help_message(&self.frontend_url),
            other => format!("Unknown subcommand `{other}` — try `/skilluv help`"),
        };
        cmd.create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await?;
        Ok(())
    }

    /// `/skilluv me` — look up the caller by discord_user_id, echo the
    /// public profile URL if linked, otherwise instruct how to link.
    async fn handle_me(&self, cmd: &CommandInteraction) -> Result<String> {
        let discord_id = cmd.user.id.to_string();
        let row: Option<(Uuid, String)> =
            sqlx::query_as("SELECT id, username FROM users WHERE discord_user_id = $1")
                .bind(&discord_id)
                .fetch_optional(&self.db)
                .await
                .context("db query failed")?;
        Ok(match row {
            Some((_, username)) => format!(
                "You are linked to **{username}** — profile: {frontend}/@{username}",
                frontend = self.frontend_url,
            ),
            None => format!(
                "Your Discord account is not linked to a Skilluv profile yet.\n\
                 Sign up or log in at {frontend}, then reply here with your \
                 username so a moderator can link it (a self-serve OAuth flow \
                 will land in a follow-up).",
                frontend = self.frontend_url,
            ),
        })
    }

    /// `/skilluv verify <hash>` — echo the attestation summary if the
    /// hash is known. Public info, no auth needed.
    async fn handle_verify(&self, hash: &str) -> Result<String> {
        let trimmed = hash.trim();
        if trimmed.is_empty() || trimmed.len() > 128 {
            return Ok("The hash must be 1..128 chars. Copy it from the /verify URL.".into());
        }
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT ps.title, u.username, ps.attestation_hash
              FROM project_slices ps
              JOIN users u ON u.id = ps.claimed_by_user_id
             WHERE ps.attestation_hash = $1
             LIMIT 1
            "#,
        )
        .bind(trimmed)
        .fetch_optional(&self.db)
        .await
        .context("db query failed")?;
        Ok(match row {
            Some((title, username, _)) => format!(
                "Attestation `{trimmed}` — **{username}** validated **{title}**\n\
                 Public verify page: {frontend}/verify/{trimmed}",
                frontend = self.frontend_url,
            ),
            None => format!("No Skilluv attestation matches `{trimmed}`."),
        })
    }
}

fn help_message(frontend: &str) -> String {
    format!(
        "**Skilluv bot** — commands available :\n\
         - `/skilluv me` — show your linked Skilluv profile\n\
         - `/skilluv verify <hash>` — check a Skilluv attestation\n\
         - `/skilluv help` — this message\n\n\
         Platform: {frontend}",
    )
}

fn extract_string(
    sub: &serenity::model::application::CommandDataOption,
    name: &str,
) -> Option<String> {
    let CommandDataOptionValue::SubCommand(opts) = &sub.value else {
        return None;
    };
    opts.iter()
        .find(|o| o.name == name)
        .and_then(|o| match &o.value {
            CommandDataOptionValue::String(s) => Some(s.clone()),
            _ => None,
        })
}

// ═══════════════════════════════════════════════════════════════════
// Notification queue poller — parity with v1 notifier, posts via bot.
// ═══════════════════════════════════════════════════════════════════

async fn queue_poll_loop(
    http: Arc<Http>,
    db: PgPool,
    promotions: ChannelId,
    annonces: ChannelId,
    frontend: String,
) {
    tracing::info!("queue poller started, tick every {QUEUE_POLL_SECONDS}s");
    loop {
        match tick(&http, &db, promotions, annonces, &frontend).await {
            Ok(n) if n > 0 => tracing::info!(sent = n, "queue tick posted"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "queue tick failed"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(QUEUE_POLL_SECONDS)).await;
    }
}

#[derive(sqlx::FromRow)]
struct QueueRow {
    id: Uuid,
    event_type: String,
    payload_json: sqlx::types::Json<Value>,
}

async fn tick(
    http: &Http,
    db: &PgPool,
    promotions: ChannelId,
    annonces: ChannelId,
    frontend: &str,
) -> Result<usize> {
    let rows: Vec<QueueRow> = sqlx::query_as(
        r#"
        SELECT id, event_type, payload_json
          FROM discord_notifications_queue
         WHERE sent_at IS NULL AND failed_count < $1
         ORDER BY created_at ASC
         LIMIT 20
        "#,
    )
    .bind(MAX_FAILED_ATTEMPTS)
    .fetch_all(db)
    .await?;

    let mut sent = 0usize;
    for row in rows {
        let channel = match row.event_type.as_str() {
            "rank_promotion" | "badge_earned" => promotions,
            "attestation_new" | "slice_validated" => annonces,
            _ => promotions, // fallback
        };
        let msg = render_message(&row.event_type, &row.payload_json.0, frontend);
        match channel.say(http, msg).await {
            Ok(_) => {
                sqlx::query("UPDATE discord_notifications_queue SET sent_at = NOW() WHERE id = $1")
                    .bind(row.id)
                    .execute(db)
                    .await?;
                sent += 1;
            }
            Err(e) => {
                sqlx::query(
                    "UPDATE discord_notifications_queue SET failed_count = failed_count + 1, last_error = $2 WHERE id = $1",
                )
                .bind(row.id)
                .bind(e.to_string())
                .execute(db)
                .await?;
                tracing::warn!(id = %row.id, error = %e, "post failed");
            }
        }
    }
    Ok(sent)
}

fn render_message(event_type: &str, payload: &Value, frontend: &str) -> String {
    match event_type {
        "rank_promotion" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let rank = payload["new_rank"].as_str().unwrap_or("");
            format!("**{username}** just reached rank **{rank}** on Skilluv.")
        }
        "badge_earned" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let badge = payload["badge_name"].as_str().unwrap_or("a new badge");
            format!("**{username}** earned the **{badge}** badge.")
        }
        "attestation_new" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let title = payload["challenge_title"].as_str().unwrap_or("a challenge");
            let hash = payload["attestation_hash"].as_str().unwrap_or("");
            format!("**{username}** just validated **{title}** — verify: {frontend}/verify/{hash}")
        }
        "slice_validated" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let repo = payload["repo"].as_str().unwrap_or("a repo");
            format!("**{username}** shipped a validated PR on **{repo}**.")
        }
        _ => format!("Skilluv event: {event_type}"),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let token = std::env::var("DISCORD_BOT_TOKEN").context("DISCORD_BOT_TOKEN required")?;
    let guild_id: u64 = std::env::var("DISCORD_GUILD_ID")
        .context("DISCORD_GUILD_ID required")?
        .parse()
        .context("DISCORD_GUILD_ID must be a numeric snowflake")?;
    let promotions_channel: u64 = std::env::var("DISCORD_PROMOTIONS_CHANNEL_ID")
        .context("DISCORD_PROMOTIONS_CHANNEL_ID required")?
        .parse()
        .context("DISCORD_PROMOTIONS_CHANNEL_ID must be a numeric snowflake")?;
    let annonces_channel: u64 = std::env::var("DISCORD_ANNONCES_CHANNEL_ID")
        .context("DISCORD_ANNONCES_CHANNEL_ID required")?
        .parse()
        .context("DISCORD_ANNONCES_CHANNEL_ID must be a numeric snowflake")?;
    let frontend_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "https://skill-uv.com".into());
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;

    let db = PgPool::connect(&database_url).await?;

    let handler = Handler {
        db,
        guild_id: GuildId::new(guild_id),
        promotions_channel: ChannelId::new(promotions_channel),
        annonces_channel: ChannelId::new(annonces_channel),
        frontend_url,
    };

    // GUILDS + GUILD_MEMBERS = welcome DM; MESSAGE_CONTENT is not needed
    // for slash commands (kept off for lower privilege footprint).
    let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MEMBERS;

    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await
        .context("failed to build Discord client")?;

    tracing::info!("starting Discord client");
    if let Err(e) = client.start().await {
        tracing::error!(error = %e, "client stopped");
        return Err(e.into());
    }
    Ok(())
}
