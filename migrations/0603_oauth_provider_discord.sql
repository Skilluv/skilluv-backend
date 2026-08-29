-- Discord becomes a fourth OAuth provider.
--
-- ## What this fixes
--
-- Migration 0036 wrote `CHECK (provider IN ('github', 'google', 'linkedin'))`
-- and the platform has had a fourth provider's worth of code for a day: the
-- adapter, the routes, the snowflake landing on `users`. Every one of those
-- paths ended at this constraint, which is a fine place to end — the write was
-- refused rather than half-applied — but it had to be widened before any of it
-- could work.
--
-- ## Why the CHECK stays a CHECK
--
-- Migration 0404 turned the capability CHECK into a table, for a reason worth
-- repeating: five migrations had restated that list and a sixth was coming with
-- every new domain, so it became a row somebody could add without touching
-- schema. That reasoning does not carry here.
--
-- An OAuth provider is not data. Each one is a Rust module with a token
-- exchange, a profile shape and a set of scopes — `services::oauth::discord`
-- is 130 lines. Adding a row to a table would let somebody name a provider
-- that nothing can authenticate against, and the failure would land at
-- runtime, on a redirect, in front of a person trying to sign in. The CHECK
-- and `VALID_PROVIDERS` in `services::oauth` are two statements of one list
-- that only ever changes when code changes, and the pair is checked by
-- `oauth_providers_agree_with_the_database` in `tests/test_discord_link.rs`.
--
-- Four values in nine hundred migrations. This is the third time it has been
-- edited, not the sixth.

ALTER TABLE user_oauth_providers
    DROP CONSTRAINT user_oauth_providers_provider_check;

ALTER TABLE user_oauth_providers
    ADD CONSTRAINT user_oauth_providers_provider_check
        CHECK (provider IN ('discord', 'github', 'google', 'linkedin'));

COMMENT ON COLUMN user_oauth_providers.provider IS
    'Which OAuth provider this link is with. Mirrors '
    '`services::oauth::VALID_PROVIDERS`; each value is a Rust adapter with its '
    'own token exchange and profile shape, so this list changes only when code '
    'does. Discord is link-only — it is how an existing account claims its '
    'Discord identity, never a way to sign up.';
