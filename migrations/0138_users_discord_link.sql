-- SKI-116 (M-10) — Link Skilluv accounts to Discord identities.
--
-- The v2 Discord bot (skilluv-discord-bot binary) sends welcome DMs
-- and answers slash commands. Both benefit from a link between a
-- Discord user and a Skilluv account: /skilluv me shows the caller's
-- own profile, /skilluv profil @user shows a mention target's profile,
-- welcome DMs offer an account-linking flow.
--
-- The link is optional: a user without a Skilluv account still gets
-- welcomed and can use commands that don't need identity.
--
-- discord_user_id is Discord's snowflake ID (u64, stored as text to
-- avoid the sqlx bigint-vs-u64 friction and stay consistent with the
-- Discord API which returns strings).

ALTER TABLE users ADD COLUMN discord_user_id TEXT;

-- One Discord identity <-> at most one Skilluv account. Partial unique
-- so most users (no link) don't collide on NULL.
CREATE UNIQUE INDEX users_discord_user_id_unique
    ON users (discord_user_id)
    WHERE discord_user_id IS NOT NULL;
