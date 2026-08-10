# Discord integration setup (SKI-34)

Skilluv posts community events (rank promotions, badges, attestations,
validated PRs) to Discord channels via a background notifier binary
`skilluv-discord-notifier`.

## What this covers (v1)

- **Notification queue → Discord webhooks** ✅ (livré PR #67)
- Backend event hooks push to `discord_notifications_queue`
- Notifier binary polls every 15s, posts to Discord via channel webhooks
- 4 event types: `rank_promotion`, `badge_earned`, `attestation_new`, `slice_validated`
- Failed posts retry up to 10× (per row), then abandon

## What this does NOT cover (v2 follow-up)

- Welcome DM auto to new Discord members → needs Discord bot user + gateway connection (serenity crate, ~800 lines Rust)
- `/skilluv` slash command → same requirement
- Link Discord account ↔ Skilluv account → needs OAuth Discord flow

These will land in a follow-up ticket if community feedback demands them.

## Setup (v1)

### 1. Create the Discord server + channels

Skip if already exists. Otherwise :
- Create a Skilluv Discord server
- Create channels : `#annonces` (attestations + slice validations), `#promotions` (rank + badges), `#ops-alerts` (SKI-32 CI failures)

### 2. Create webhooks per channel

For each of `#annonces` and `#promotions` :
1. Right-click channel → Edit Channel → Integrations
2. Webhooks → New Webhook → name "Skilluv Notifier"
3. Copy the URL — pattern `https://discord.com/api/webhooks/XXXX/YYYY`

### 3. Env vars for the notifier binary

```bash
export DATABASE_URL="postgres://..."
export DISCORD_PROMOTIONS_WEBHOOK_URL="https://discord.com/api/webhooks/XXX/YYY"
export DISCORD_ANNONCES_WEBHOOK_URL="https://discord.com/api/webhooks/AAA/BBB"
```

### 4. Deploy the notifier

**Option A — Coolify (recommended)** :
Create a new Docker application in Coolify with the same Dockerfile as
the backend but overriding `CMD` to `["skilluv-discord-notifier"]`. Or
build a dedicated Dockerfile stage that only includes this binary.

**Option B — Systemd on the host** :
```
# /etc/systemd/system/skilluv-discord-notifier.service
[Unit]
Description=Skilluv Discord Notifier
After=network.target

[Service]
Type=simple
EnvironmentFile=/opt/skilluv/discord-notifier.env
ExecStart=/opt/skilluv/skilluv-discord-notifier
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### 5. Enqueue a test notification

From a psql session or admin API :

```sql
INSERT INTO discord_notifications_queue (event_type, payload_json)
VALUES (
    'rank_promotion',
    '{"username": "jeremie", "new_rank": "artisan"}'::jsonb
);
```

Within 15s, `#promotions` should receive :
> 🎉 **jeremie** just reached rank **artisan** on Skilluv !

## Backend integration — how to enqueue

From any service that detects a notable event :

```rust
sqlx::query(
    r#"
    INSERT INTO discord_notifications_queue (event_type, payload_json)
    VALUES ($1, $2)
    "#,
)
.bind("attestation_new")
.bind(sqlx::types::Json(serde_json::json!({
    "username": user.username,
    "challenge_title": slice.title,
    "attestation_hash": attestation.hash,
})))
.execute(&state.db)
.await?;
```

Recommended integration points (post-PR follow-up) :
- `services/slice_validation.rs::approve` → `attestation_new`
- `services/proof_hooks.rs` promotion detection → `rank_promotion`, `badge_earned`
- `services/ci_sync.rs::handle_pull_request_event` merged branch → `slice_validated`

## Troubleshooting

**Notifier polls but never posts** → check env vars, run `cargo run --bin skilluv-discord-notifier` locally with `RUST_LOG=info`. If it says "no Discord webhook URLs configured", set the env vars.

**Row stuck with high failed_count** → look at `last_error` column. Common causes:
- Discord rate-limited (429) — the retry backoff will handle it, wait
- Webhook URL revoked (401) — regenerate + update env, then reset the row: `UPDATE discord_notifications_queue SET failed_count=0, last_error=NULL WHERE id='...';`
- Malformed payload — check the message rendering in `render_message()`

**Delayed notifications** → poll interval is 15s. Increase or decrease via editing `POLL_INTERVAL_SECONDS` in `src/bin/discord_notifier.rs`.
