-- Registered mods, hosted elsewhere and proven here.
--
-- Skilluv never hosts a mod package — that is a legal line, not a storage one:
-- a mod re-hosts a game's assets, and re-hosting those is the vendor's to
-- forbid. So a mod is a registration, not an upload: a live URL on the platform
-- the game uses, a download count, and a reviewer confirming three things — the
-- URL is live, the mod is the author's, and the vendor's terms were kept.
--
-- A confirmed mod issues the game_mod_published attestation and, because that
-- basis requires a deliverable, creates one — so a modder's shipped work counts
-- toward the cross-domain rank the same as anyone else's, even though the
-- artefact lives on Nexus and not on us.

CREATE TABLE game_mods (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The optional slice: a mod can be registered against a game_artifact slice
    -- (subtype mod_package) or stand alone. When it is a slice, the deliverable
    -- the attestation rests on is the slice's.
    slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL,
    title VARCHAR(200) NOT NULL,
    target_game VARCHAR(120) NOT NULL,
    target_platform VARCHAR(20) NOT NULL
        CHECK (target_platform IN (
            'nexusmods', 'steam_workshop', 'curseforge', 'moddb', 'thunderstore', 'other'
        )),
    -- Where it lives. The proof is this URL being real and the mod being theirs.
    external_hosting_url VARCHAR(500) NOT NULL,
    -- Fetched from the platform where an API allows, declared otherwise. The
    -- craft score reads it for the "viral past a thousand" term.
    external_downloads_count INTEGER NOT NULL DEFAULT 0
        CHECK (external_downloads_count >= 0),
    description_md TEXT NOT NULL,
    -- The review state. A mod is registered, then a community reviewer confirms
    -- or refuses it with a reason.
    status VARCHAR(12) NOT NULL DEFAULT 'registered'
        CHECK (status IN ('registered', 'confirmed', 'refused')),
    reviewed_by UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    -- Required whenever the mod leaves 'registered'. A confirmation says who; a
    -- refusal says why. Either way the decision carries a reason.
    review_reason TEXT,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT a_reviewed_mod_names_its_reviewer CHECK (
        status = 'registered' OR (reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL)
    ),
    CONSTRAINT a_refused_mod_says_why CHECK (
        status <> 'refused' OR (review_reason IS NOT NULL AND btrim(review_reason) <> '')
    )
);

CREATE INDEX idx_game_mods_author ON game_mods (author_user_id, status);
CREATE INDEX idx_game_mods_status ON game_mods (status) WHERE status = 'registered';

COMMENT ON TABLE game_mods IS
    'A mod hosted on a third-party platform, registered here with a live URL and '
    'confirmed by a community reviewer. Skilluv holds the metadata and the '
    'proof, never the package. A confirmed mod issues game_mod_published and '
    'the deliverable that basis rests on.';
