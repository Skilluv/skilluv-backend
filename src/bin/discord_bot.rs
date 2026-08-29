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
use skilluv_backend::services::discord_roles;
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
        // Built from the list every validator reads, not copied beside it.
        // The copy that used to sit here named seven domains long after four
        // more had opened, so the bot was quietly telling people that
        // quality, leadership, communication and education did not exist.
        let domain_hint = domain_hint();

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
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "contests",
                        "Open contests you can still enter",
                    )
                    .add_sub_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "domain",
                        domain_hint.as_str(),
                    )),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "featured",
                        "Who is featured this week",
                    )
                    .add_sub_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "domain",
                        domain_hint.as_str(),
                    )),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "portfolio",
                        "Somebody's public Skilluv profile",
                    )
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "username",
                            "Their Skilluv username",
                        )
                        .required(true),
                    ),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "craft",
                        "Your craft score in one domain, and what it is made of",
                    )
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "domain",
                            domain_hint.as_str(),
                        )
                        .required(true),
                    ),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "queue",
                        "How much work is waiting on a reviewer in one domain",
                    )
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "domain",
                            domain_hint.as_str(),
                        )
                        .required(true),
                    ),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::SubCommand,
                        "cohorts",
                        "Cohorts recruiting now",
                    )
                    .add_sub_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "domain",
                        domain_hint.as_str(),
                    )),
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

        // Second poller: the Discord half of the role loop.
        //
        // Separate from the notification tick because the two fail
        // independently and at different rates — a rate-limited role write
        // must not stop an announcement from going out, and vice versa.
        let http2 = ctx.http.clone();
        let db2 = self.db.clone();
        let guild = self.guild_id;
        let frontend2 = self.frontend_url.clone();
        tokio::spawn(async move {
            role_sync_loop(http2, db2, guild, frontend2).await;
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
            "contests" => {
                self.handle_contests(extract_string(sub, "domain").as_deref())
                    .await?
            }
            "featured" => {
                self.handle_featured(extract_string(sub, "domain").as_deref())
                    .await?
            }
            "portfolio" => {
                let username = extract_string(sub, "username").context("missing username arg")?;
                self.handle_portfolio(&username).await?
            }
            "craft" => {
                let domain = extract_string(sub, "domain").context("missing domain arg")?;
                self.handle_craft(cmd, &domain).await?
            }
            "queue" => {
                let domain = extract_string(sub, "domain").context("missing domain arg")?;
                self.handle_queue(&domain).await?
            }
            "cohorts" => {
                self.handle_cohorts(extract_string(sub, "domain").as_deref())
                    .await?
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

    /// `/skilluv craft <domain>` — the caller's craft score in one domain.
    ///
    /// One command with a domain argument rather than one per domain. The
    /// Discord structure documents for leadership and quality each described
    /// a `my-stats` of their own, and neither existed; writing them as two
    /// would have made the next domain a third.
    async fn handle_craft(&self, cmd: &CommandInteraction, domain: &str) -> Result<String> {
        if !skilluv_backend::validators::SKILL_DOMAINS.contains(&domain) {
            return Ok(format!(
                "`{domain}` is not a domain. One of: {}",
                skilluv_backend::validators::SKILL_DOMAINS.join(", ")
            ));
        }

        let discord_id = cmd.user.id.to_string();
        let user: Option<(Uuid, String)> =
            sqlx::query_as("SELECT id, username FROM users WHERE discord_user_id = $1")
                .bind(&discord_id)
                .fetch_optional(&self.db)
                .await
                .context("db query failed")?;
        let Some((user_id, username)) = user else {
            return Ok(format!(
                "This Discord account is not linked to a Skilluv profile yet — {}/settings",
                self.frontend_url
            ));
        };

        let score: Option<(i32, Option<String>)> = sqlx::query_as(
            "SELECT score, tier_slug FROM craft_scores WHERE user_id = $1 AND skill_domain = $2",
        )
        .bind(user_id)
        .bind(domain)
        .fetch_optional(&self.db)
        .await
        .context("db query failed")?;

        let Some((score, tier)) = score else {
            return Ok(format!(
                "**{username}** has no craft score in `{domain}` yet. It is computed from \
                 validated work, so the first one arrives with the first validation."
            ));
        };

        // What the score is made of, so a number nobody can question is not
        // what gets posted in a channel.
        let attested: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attestations a
               JOIN attestation_bases b ON b.basis = a.basis
              WHERE a.user_id = $1 AND b.skill_domain = $2",
        )
        .bind(user_id)
        .bind(domain)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        let tier = tier.unwrap_or_else(|| "—".into());
        Ok(format!(
            "**{username}** — `{domain}`\n\
             Craft score: **{score}** ({tier})\n\
             Attestations in this domain: {attested}\n\
             {}/u/{username}",
            self.frontend_url
        ))
    }

    /// `/skilluv queue <domain>` — what is waiting on a reviewer.
    ///
    /// Public on purpose. A review queue nobody can see is a queue that grows
    /// quietly, and the number being visible is what makes somebody
    /// volunteer.
    async fn handle_queue(&self, domain: &str) -> Result<String> {
        if !skilluv_backend::validators::SKILL_DOMAINS.contains(&domain) {
            return Ok(format!(
                "`{domain}` is not a domain. One of: {}",
                skilluv_backend::validators::SKILL_DOMAINS.join(", ")
            ));
        }

        // Two numbers, and the second is the one that matters: work nobody has
        // picked up is work nobody has promised to look at.
        let (picked, unpicked): (i64, i64) = sqlx::query_as(
            r#"
            SELECT count(*) FILTER (WHERE s.picked_by_validator_id IS NOT NULL),
                   count(*) FILTER (WHERE s.picked_by_validator_id IS NULL)
              FROM project_slices s
              JOIN slice_types t ON t.slug = s.slice_type
             WHERE s.status = 'pending_validation'
               AND t.skill_domain = $1
            "#,
        )
        .bind(domain)
        .fetch_one(&self.db)
        .await
        .context("db query failed")?;

        if picked + unpicked == 0 {
            return Ok(format!("Nothing waiting in `{domain}`."));
        }

        // How long the oldest unpicked one has been there. A queue of three
        // that turns over in a day is healthy; a queue of three where one has
        // sat for a fortnight is not, and the count alone hides that.
        let oldest: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            r#"
            SELECT min(s.submitted_at)
              FROM project_slices s
              JOIN slice_types t ON t.slug = s.slice_type
             WHERE s.status = 'pending_validation'
               AND s.picked_by_validator_id IS NULL
               AND t.skill_domain = $1
            "#,
        )
        .bind(domain)
        .fetch_one(&self.db)
        .await
        .unwrap_or(None);

        let waiting = oldest
            .map(|d| {
                let days = (chrono::Utc::now() - d).num_days();
                format!("\nOldest unclaimed: {days} day(s).")
            })
            .unwrap_or_default();

        Ok(format!(
            "`{domain}` validation queue:\n\
             - **{unpicked}** waiting for somebody to pick up\n\
             - {picked} picked up and in review{waiting}\n\
             {}/validation",
            self.frontend_url
        ))
    }

    /// `/skilluv cohorts [domain]` — cohorts somebody can still join.
    ///
    /// One command for every domain that runs cohorts, which is every domain
    /// since migration 0532 gave them one model. Private cohorts never appear
    /// here: they are reached by invitation, and listing them in a public
    /// channel would defeat what makes them private.
    async fn handle_cohorts(&self, domain: Option<&str>) -> Result<String> {
        let rows: Vec<(
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            i64,
            Option<i32>,
        )> = sqlx::query_as(
            r#"
            SELECT c.name, c.slug, c.starts_at,
                   (SELECT count(*) FROM cohort_members m
                     WHERE m.cohort_id = c.id AND m.left_at IS NULL),
                   c.max_members
              FROM cohorts c
              LEFT JOIN orientations o ON o.id = c.orientation_id
             WHERE c.is_public
               AND c.archived_at IS NULL
               AND c.concluded_at IS NULL
               AND (c.starts_at IS NULL OR c.starts_at > NOW())
               AND ($1::TEXT IS NULL
                    OR c.target_domain = $1
                    OR o.primary_domain = $1)
             ORDER BY c.starts_at ASC NULLS LAST
             LIMIT 5
            "#,
        )
        .bind(domain)
        .fetch_all(&self.db)
        .await
        .context("db query failed")?;

        if rows.is_empty() {
            return Ok("No cohort is recruiting right now.".into());
        }

        let lines: Vec<String> = rows
            .iter()
            .map(|(name, slug, starts, members, max)| {
                let when = starts
                    .map(|d| format!(" — starts {}", d.format("%d/%m")))
                    .unwrap_or_default();
                let places = match max {
                    Some(m) => format!(" ({members}/{m})"),
                    None => format!(" ({members} joined)"),
                };
                format!(
                    "- **{name}**{when}{places} — {}/cohorts/{slug}",
                    self.frontend_url
                )
            })
            .collect();
        Ok(format!("Cohorts recruiting:\n{}", lines.join("\n")))
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
            Some((id, username)) => {
                // The trades and the scores, because "you are linked" is not
                // worth a round trip on its own.
                let profile = self.profile_lines(id).await;
                format!(
                    "You are linked to **{username}** — {frontend}/@{username}{profile}",
                    frontend = self.frontend_url,
                )
            }
            None => format!(
                // This used to send people to a moderator, because linking was
                // manual and `users.discord_user_id` had no writer. It has one
                // now, so the instruction changed with it — a message telling
                // somebody to queue for a human when a button exists is worse
                // than no message.
                "Ton compte Discord n'est pas encore lié à un profil Skilluv.\n\n\
                 Connecte-toi sur {frontend}, puis va dans **Paramètres → \
                 Comptes liés → Discord**. Tes rôles arrivent dans la minute, \
                 et ils suivront ton profil ensuite.\n\n\
                 Astuce : déclare d'abord tes métiers. C'est ce qui ouvre les \
                 salons de ton domaine — sans eux, il n'y a pas grand-chose à \
                 te donner.",
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
        // Two tables, because attestations live in two places. The slice
        // hash is the older one; `attestations` is where every design, code
        // and AI attestation has gone since, and it carries the ten-character
        // verification code people actually paste. Reading only the first
        // meant every one of those answered "no such attestation".
        let row: Option<(String, String, bool, String)> = sqlx::query_as(
            r#"
            SELECT ps.title, u.username, FALSE AS revoked,
                   ps.attestation_hash AS code
              FROM project_slices ps
              JOIN users u ON u.id = ps.claimed_by_user_id
             WHERE ps.attestation_hash = $1
            UNION ALL
            SELECT a.title, u2.username, a.revoked_at IS NOT NULL,
                   a.verification_code
              FROM attestations a
              JOIN users u2 ON u2.id = a.user_id
             WHERE a.verification_code = $1
            LIMIT 1
            "#,
        )
        .bind(trimmed)
        .fetch_optional(&self.db)
        .await
        .context("db query failed")?;
        Ok(match row {
            // A revoked attestation is answered, not hidden. Somebody
            // checking one has been shown a copy and needs to be told it no
            // longer holds.
            Some((title, username, true, code)) => format!(
                "Attestation `{code}` — **{username}**, **{title}** — \
                 **cette attestation a été révoquée**.\n\
                 {frontend}/attestations/verify/{code}",
                frontend = self.frontend_url,
            ),
            Some((title, username, false, code)) => format!(
                "Attestation `{code}` — **{username}** a validé **{title}**\n\
                 {frontend}/attestations/verify/{code}",
                frontend = self.frontend_url,
            ),
            None => format!("No Skilluv attestation matches `{trimmed}`."),
        })
    }

    /// The trades and scores under a name, as chat-sized lines.
    ///
    /// Empty when there is nothing to say. A profile that prints "0 points,
    /// no trade" for a new member reads as a verdict rather than as a start.
    async fn profile_lines(&self, user_id: Uuid) -> String {
        let trades: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT o.name
              FROM user_orientations uo
              JOIN orientations o ON o.id = uo.orientation_id
             WHERE uo.user_id = $1 AND uo.ended_at IS NULL
             ORDER BY uo.is_primary DESC, o.name
             LIMIT 3
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        let scores: Vec<(String, i32)> = sqlx::query_as(
            "SELECT skill_domain, score FROM craft_scores
              WHERE user_id = $1 AND score > 0
              ORDER BY score DESC LIMIT 3",
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        let mut out = String::new();
        if !trades.is_empty() {
            out.push_str(&format!("\nMétiers : {}", trades.join(", ")));
        }
        if !scores.is_empty() {
            let rendered: Vec<String> = scores
                .iter()
                .map(|(domain, score)| format!("{domain} {score}"))
                .collect();
            out.push_str(&format!("\nCraft score : {}", rendered.join(" · ")));
        }
        out
    }

    /// `/skilluv contests [domain]` — what somebody can still enter.
    ///
    /// Cross-domain contests are always included, whichever domain was asked
    /// for: those are the events that want the widest field, and filtering
    /// them out would hide exactly the ones worth announcing.
    async fn handle_contests(&self, domain: Option<&str>) -> Result<String> {
        let rows: Vec<(String, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            r#"
            SELECT name, slug, ends_at
              FROM tournaments
             WHERE status IN ('upcoming', 'registration', 'active')
               AND ($1::TEXT IS NULL OR skill_domain = $1 OR skill_domain IS NULL)
             ORDER BY ends_at ASC NULLS LAST
             LIMIT 5
            "#,
        )
        .bind(domain)
        .fetch_all(&self.db)
        .await
        .context("db query failed")?;

        if rows.is_empty() {
            return Ok("Aucun concours ouvert en ce moment.".into());
        }
        let lines: Vec<String> = rows
            .iter()
            .map(|(name, slug, ends)| {
                let until = ends
                    .map(|d| format!(" — jusqu'au {}", d.format("%d/%m")))
                    .unwrap_or_default();
                format!(
                    "- **{name}**{until} — {}/contests/{slug}",
                    self.frontend_url
                )
            })
            .collect();
        Ok(format!("Concours ouverts :\n{}", lines.join("\n")))
    }

    /// `/skilluv featured [domain]` — the week's editorial pick.
    async fn handle_featured(&self, domain: Option<&str>) -> Result<String> {
        let row: Option<(String, String, String, chrono::NaiveDate)> = sqlx::query_as(
            r#"
            SELECT u.username, u.display_name, ft.reason_md, ft.week_of
              FROM featured_talents ft
              JOIN users u ON u.id = ft.user_id
             WHERE ($1::TEXT IS NULL OR ft.skill_domain = $1)
             ORDER BY ft.week_of DESC
             LIMIT 1
            "#,
        )
        .bind(domain)
        .fetch_optional(&self.db)
        .await
        .context("db query failed")?;

        Ok(match row {
            Some((username, display_name, reason, week)) => format!(
                "**{display_name}** ({}/@{username}) — semaine du {week}\n{reason}",
                self.frontend_url
            ),
            None => "Personne n'a encore été mis en avant ici.".into(),
        })
    }

    /// `/skilluv portfolio <username>` — somebody's public profile.
    ///
    /// Public rows only. A hidden or banned profile answers as unknown rather
    /// than as hidden: confirming that an account exists is itself a leak on
    /// a surface anybody can query.
    async fn handle_portfolio(&self, username: &str) -> Result<String> {
        let trimmed = username.trim().trim_start_matches('@');
        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, username, display_name FROM users
              WHERE username = $1 AND profile_active = TRUE
                AND is_banned = FALSE AND profile_hidden = FALSE",
        )
        .bind(trimmed)
        .fetch_optional(&self.db)
        .await
        .context("db query failed")?;

        Ok(match row {
            Some((id, username, display_name)) => {
                let profile = self.profile_lines(id).await;
                format!(
                    "**{display_name}** — {frontend}/@{username}{profile}",
                    frontend = self.frontend_url,
                )
            }
            None => format!("Aucun profil public au nom de `{trimmed}`."),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// The Discord half of the role loop
// ═══════════════════════════════════════════════════════════════════

const ROLE_SYNC_POLL_SECONDS: u64 = 20;

/// Drain `discord_role_sync_queue` and make the server match the platform.
///
/// Separate from the notification poller because the two fail independently: a
/// rate-limited role write must not stop an announcement going out.
async fn role_sync_loop(http: Arc<Http>, db: PgPool, guild: GuildId, frontend: String) {
    tracing::info!("role sync poller started, tick every {ROLE_SYNC_POLL_SECONDS}s");
    loop {
        match role_sync_tick(&http, &db, guild, &frontend).await {
            Ok(n) if n > 0 => tracing::info!(synced = n, "role sync tick applied"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "role sync tick failed"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(ROLE_SYNC_POLL_SECONDS)).await;
    }
}

#[derive(sqlx::FromRow)]
struct SyncRow {
    id: Uuid,
    user_id: Uuid,
    reason: String,
    discord_user_id: Option<String>,
    username: Option<String>,
}

async fn role_sync_tick(http: &Http, db: &PgPool, guild: GuildId, frontend: &str) -> Result<usize> {
    // Oldest first, capped. A nightly sweep can queue every member at once and
    // Discord rate-limits role writes per guild; batching keeps a promotion
    // from thirty seconds ago out from behind a thousand routine rows.
    let rows: Vec<SyncRow> = sqlx::query_as(
        "SELECT q.id, q.user_id, q.reason, u.discord_user_id, u.username
           FROM discord_role_sync_queue q
           JOIN users u ON u.id = q.user_id
          WHERE q.applied_at IS NULL AND q.failed_count < $1
          ORDER BY q.requested_at
          LIMIT 20",
    )
    .bind(MAX_FAILED_ATTEMPTS)
    .fetch_all(db)
    .await
    .context("claim role sync rows")?;

    if rows.is_empty() {
        return Ok(0);
    }

    // Read once per tick, not per row: the declaration is embedded in the
    // binary and the guild's roles change about never.
    let rules = discord_roles::rules().map_err(|e| anyhow::anyhow!("{e}"))?;
    let by_name: std::collections::HashMap<String, serenity::all::RoleId> = guild
        .roles(http)
        .await
        .context("list guild roles")?
        .iter()
        .map(|(id, r)| (r.name.clone(), *id))
        .collect();

    let mut done = 0usize;
    for row in &rows {
        match apply_one(http, db, guild, row, &rules, &by_name, frontend).await {
            Ok(true) => done += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(user = %row.user_id, reason = %row.reason, error = %e,
                    "role sync failed for one member");
                let _ = sqlx::query(
                    "UPDATE discord_role_sync_queue
                        SET failed_count = failed_count + 1, last_error = $2
                      WHERE id = $1",
                )
                .bind(row.id)
                // Truncated: a transport error can carry a whole response body
                // and this column is read by a human in a terminal.
                .bind(e.to_string().chars().take(500).collect::<String>())
                .execute(db)
                .await;
            }
        }
    }
    Ok(done)
}

/// Reconcile one member. Returns whether anything actually changed.
async fn apply_one(
    http: &Http,
    db: &PgPool,
    guild: GuildId,
    row: &SyncRow,
    rules: &[discord_roles::RoleRule],
    by_name: &std::collections::HashMap<String, serenity::all::RoleId>,
    frontend: &str,
) -> Result<bool> {
    let Some(snowflake) = row.discord_user_id.as_deref().filter(|s| !s.is_empty()) else {
        // Unlinked between the request and now: no Discord identity to
        // reconcile. Finished, rather than retried ten times.
        mark_synced(db, row.id, &[], &[]).await;
        return Ok(false);
    };
    let discord_id: serenity::all::UserId = snowflake
        .parse::<u64>()
        .with_context(|| format!("discord_user_id {snowflake:?} is not a snowflake"))?
        .into();

    let standing = discord_roles::standing(db, row.user_id)
        .await
        .map_err(|e| anyhow::anyhow!("standing: {e}"))?;
    let desired = discord_roles::desired(rules, &standing);
    let managed = discord_roles::managed(rules);

    let Ok(member) = guild.member(http, discord_id).await else {
        // Linked the account but never joined the server, or has left it.
        // Neither is an error worth ten retries — there is nobody there to
        // give a role to.
        mark_synced(db, row.id, &[], &[]).await;
        return Ok(false);
    };

    // Held roles by name. Roles the guild has that this repository never
    // declared resolve to names too, which is exactly what `diff` needs in
    // order to leave them alone.
    let held: Vec<String> = member
        .roles
        .iter()
        .filter_map(|rid| {
            by_name
                .iter()
                .find(|(_, id)| *id == rid)
                .map(|(name, _)| name.clone())
        })
        .collect();

    let d = discord_roles::diff(&desired, &held, &managed);
    if d.is_empty() {
        mark_synced(db, row.id, &[], &[]).await;
        return Ok(false);
    }

    for name in &d.add {
        match by_name.get(name) {
            Some(rid) => member
                .add_role(http, *rid)
                .await
                .with_context(|| format!("add role {name}"))?,
            // Declared in server.toml but absent from the guild. Not fatal for
            // the other roles, and naming it is how somebody learns to run
            // `discord-setup.py --create`.
            None => tracing::warn!(role = %name, "declared role missing from the guild"),
        }
    }
    for name in &d.remove {
        if let Some(rid) = by_name.get(name) {
            member
                .remove_role(http, *rid)
                .await
                .with_context(|| format!("remove role {name}"))?;
        }
    }

    // The welcome, sent once: on the sync that follows a link, and only when
    // roles were actually granted. Somebody who links and receives nothing has
    // no trades and no rank yet, and congratulating them on an empty set would
    // be worse than silence.
    if row.reason == "linked" && !d.add.is_empty() {
        let who = row.username.as_deref().unwrap_or("ton compte");
        let granted = d
            .add
            .iter()
            .map(|r| format!("**{r}**"))
            .collect::<Vec<_>>()
            .join(", ");
        let msg = format!(
            "Ton compte Discord est lié à **{who}**.\n\n\
             Tu viens de recevoir : {granted}\n\n\
             Ces rôles suivent ton profil : ils changent quand tes métiers, ton \
             rang ou tes habilitations changent, et tu n'as rien à demander.\n\n\
             Ton profil : {frontend}/@{who}"
        );
        // Best effort, and one call rather than two. A closed DM must not fail
        // a sync that already succeeded on the server.
        match discord_id.create_dm_channel(http).await {
            Ok(channel) => {
                let _ = channel
                    .id
                    .send_message(http, CreateMessage::new().content(msg))
                    .await;
            }
            Err(e) => tracing::debug!(error = %e, "no DM channel; skipping welcome"),
        }
    }

    mark_synced(db, row.id, &d.add, &d.remove).await;
    Ok(true)
}

/// Record what was done, so "why did I lose @Relecteur" has an answer.
async fn mark_synced(db: &PgPool, id: Uuid, added: &[String], removed: &[String]) {
    let _ = sqlx::query(
        "UPDATE discord_role_sync_queue
            SET applied_at = NOW(), roles_added = $2, roles_removed = $3
          WHERE id = $1",
    )
    .bind(id)
    .bind(added)
    .bind(removed)
    .execute(db)
    .await;
}

/// Discord's hard limit on a command or option description.
const DESCRIPTION_LIMIT: usize = 100;

/// The `domain` option's description: the trades, from the list every
/// validator reads, capped to what Discord accepts.
///
/// ## Why capping and not a shorter list
///
/// Discord rejects any description over 100 characters, and `set_commands`
/// replaces the whole command tree in one call — so one long string fails the
/// entire registration. The bot then keeps whatever commands a previous build
/// left behind, logs an error nobody is watching, and serves a stale command
/// list. That is exactly what happened: the twelfth domain took the joined
/// list to 104 characters, five options were refused, and the guild kept the
/// three subcommands of an older build while six newer ones were invisible.
///
/// Shortening the list by four characters would have fixed it until the
/// thirteenth domain. The cap does not care how many there are.
fn domain_hint() -> String {
    let full = skilluv_backend::validators::SKILL_DOMAINS.join(", ");
    if full.chars().count() <= DESCRIPTION_LIMIT {
        return full;
    }
    // Cut on a separator so the hint never ends mid-word, and leave room for
    // the ellipsis. A reader who needs the whole list gets it from the
    // validation error on a wrong value, which names every domain.
    let budget = DESCRIPTION_LIMIT - 1;
    let mut kept = String::new();
    for domain in skilluv_backend::validators::SKILL_DOMAINS {
        let sep = if kept.is_empty() { "" } else { ", " };
        if kept.chars().count() + sep.len() + domain.chars().count() > budget {
            break;
        }
        kept.push_str(sep);
        kept.push_str(domain);
    }
    kept.push('…');
    kept
}

fn help_message(frontend: &str) -> String {
    format!(
        "**Skilluv bot** — commands available :\n\
         - `/skilluv me` — your linked profile, trades and craft score\n\
         - `/skilluv verify <hash>` — check a Skilluv attestation\n\
         - `/skilluv contests [domain]` — open contests\n\
         - `/skilluv featured [domain]` — this week\'s featured member\n\
         - `/skilluv portfolio <username>` — somebody\'s public profile\n\
         - `/skilluv craft <domain>` — your craft score there\n\
         - `/skilluv queue <domain>` — what is waiting on a reviewer\n\
         - `/skilluv cohorts [domain]` — cohorts recruiting now\n\
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
    target_channel_id: Option<String>,
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
        SELECT id, event_type, payload_json, target_channel_id
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
        // The row carries the room it was routed to at enqueue time. This
        // column has existed since migration 0135 and was ignored, so every
        // announcement went to one of two hardcoded channels regardless.
        let channel = match row
            .target_channel_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok())
        {
            Some(id) => ChannelId::new(id),
            // No room configured for that purpose. Posting in the default
            // beats dropping the announcement.
            None => match row.event_type.as_str() {
                "rank_promotion" | "badge_earned" => promotions,
                _ => annonces,
            },
        };
        let msg = skilluv_backend::services::discord_announce::render(
            &row.event_type,
            &row.payload_json.0,
            frontend,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this guards is silent in production and invisible in tests
    /// that do not run it: Discord refuses the whole `set_commands` call when
    /// one description is too long, so the guild keeps the previous build's
    /// commands and the only trace is an error line in the bot's log.
    ///
    /// It shipped. Twelve domains joined to 104 characters, five options were
    /// refused, and the server served three subcommands while six others had
    /// been written and deployed.
    #[test]
    fn the_domain_hint_fits_what_discord_accepts() {
        let hint = domain_hint();
        assert!(
            hint.chars().count() <= DESCRIPTION_LIMIT,
            "{} characters: Discord refuses the whole registration",
            hint.chars().count()
        );
        assert!(!hint.is_empty());

        // Still useful: whatever fits must name real domains, not an ellipsis
        // on its own.
        assert!(
            hint.starts_with(skilluv_backend::validators::SKILL_DOMAINS[0]),
            "{hint}"
        );
    }

    /// The cap has to hold for a catalogue that keeps growing — that is the
    /// whole reason it exists rather than a hand-shortened list.
    #[test]
    fn the_hint_would_still_fit_with_far_more_domains() {
        // Twice the current catalogue, with long names.
        let many: Vec<String> = (0..30).map(|i| format!("some_long_domain_{i}")).collect();
        let full = many.join(", ");
        assert!(full.chars().count() > DESCRIPTION_LIMIT, "premise");

        // Same algorithm the function applies, over the larger list.
        let budget = DESCRIPTION_LIMIT - 1;
        let mut kept = String::new();
        for d in &many {
            let sep = if kept.is_empty() { "" } else { ", " };
            if kept.chars().count() + sep.len() + d.chars().count() > budget {
                break;
            }
            kept.push_str(sep);
            kept.push_str(d);
        }
        kept.push('…');
        assert!(kept.chars().count() <= DESCRIPTION_LIMIT, "{kept}");
    }

    // There is deliberately no test over the fixed descriptions. One was
    // written, listing them by hand, and every string in it was subtly wrong
    // — "Open contests" for "Open contests you can still enter". It passed,
    // and it asserted nothing about the code. A copy of a literal is not a
    // check on that literal; it is a second literal that drifts.
    //
    // The five options Discord refused were all the generated one, which is
    // what the tests above hold. A fixed description going over 100
    // characters would be visible in the diff that wrote it.
}
