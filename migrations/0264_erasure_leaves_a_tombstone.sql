-- Erasing a person without erasing what other people proved.
--
-- ## What deletion did
--
-- `DELETE FROM users` — and the cascades took everything with it: the
-- contest entries, the podium places, the validated deliverables, the
-- attestations that other people's records refer to.
--
-- That is wrong in both directions at once. It destroys more than the person
-- asked for — a contest where the second place vanished leaves first and third
-- unexplained, and the winner's own attestation cites a ranking that no longer
-- adds up. And it is not what erasure requires: what has to go is the personal
-- data, not every trace that a participant existed.
--
-- ## The tombstone
--
-- The `users` row survives with nothing personal in it: no name, no e-mail, no
-- avatar, no biography, and a password hash nothing can match. Everything
-- pointing at it still points somewhere, and everything it says about the
-- person is gone.
--
-- Purely personal rows are still deleted outright — notifications, e-mail
-- preferences, cloud tokens, wizard answers, declared portfolios. Nobody
-- else's record depends on those.
--
-- ## Why the username is not simply blanked
--
-- It is `NOT NULL` and unique, and it is the key half the platform joins on.
-- It becomes `supprime-{eight hex}` — recognisable as a tombstone at a glance,
-- unique by construction, and impossible to confuse with a name somebody
-- chose. The same for the e-mail, on a domain reserved by RFC 2606 so a stray
-- mailer cannot deliver anywhere.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

COMMENT ON COLUMN users.deleted_at IS
    'When this account was erased at its owner''s request. The row survives as '
    'a tombstone so that other people''s records — contest rankings, reviews '
    'they received — keep pointing somewhere; nothing personal survives with '
    'it. Never null for an account that can still be logged into.';

-- Every read path that shows a person has to skip these, and the partial
-- index is what makes that cheap enough that nobody is tempted to omit it.
CREATE INDEX IF NOT EXISTS idx_users_alive
    ON users (id)
    WHERE deleted_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- A tombstone cannot be logged into
-- ═══════════════════════════════════════════════════════════════════

-- Belt and braces on top of the anonymised hash. The application refuses an
-- erased account at login; this makes an application that forgot to refuse it
-- unable to succeed anyway, because there is no password that hashes to a
-- string this short.
ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_erased_account_is_inactive;

ALTER TABLE users
    ADD CONSTRAINT users_erased_account_is_inactive CHECK (
        deleted_at IS NULL
        OR (profile_active = FALSE AND profile_hidden = TRUE)
    );
