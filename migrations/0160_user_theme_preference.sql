-- The world a person chose to read Skilluv in.
--
-- The frontend ships five: the smith's workshop, the lantern-lit night, the
-- tournament, the copyist's desk, the cherry blossom season. Someone picks
-- one and the whole application follows it.
--
-- Their email did not. It arrived in the default palette whatever they had
-- chosen, which makes the message read as though a different company sent
-- it — and quietly undoes the one moment where a product gets to feel
-- personal.
--
-- Stored on the account rather than resolved per request, for the same
-- reason as the language: an email is composed by a background job, with no
-- request to read a preference from.
--
-- Nullable: absence means the default, and writing 'forge' into every
-- existing row would make a deliberate choice indistinguishable from never
-- having made one.

ALTER TABLE users
    ADD COLUMN preferred_theme VARCHAR(20);

COMMENT ON COLUMN users.preferred_theme IS
    'One of forge, vesperal, arena, scriptorium, sakura. Drives the email '
    'palette (services::email_theme). NULL means the default. Deliberately '
    'not constrained by a CHECK: the frontend may ship a sixth world before '
    'the backend learns about it, and an unknown value falls back rather '
    'than rejecting the profile update.';

CREATE INDEX idx_users_preferred_theme
    ON users (preferred_theme)
    WHERE preferred_theme IS NOT NULL;
