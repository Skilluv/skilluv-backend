-- What a game attestation rests on.
--
-- The security rollout ended with 0547 and 0559 giving attestations a column
-- for the object each basis stands on, and widening the deliverable's "at least
-- one parent" so a finding counts toward a rank. The game domain needs the
-- same, for its two objects that are not slices — a jam and a mod — plus the
-- external URL a shipped title points at.
--
-- A game_artifact attestation needs no new column: it rests on its slice, and
-- the slice is already the deliverable's parent. Only the jam, the mod and the
-- shipped-title link are new here.

ALTER TABLE attestations
    ADD COLUMN game_jam_id UUID REFERENCES game_jams(id) ON DELETE CASCADE,
    ADD COLUMN game_mod_id UUID REFERENCES game_mods(id) ON DELETE CASCADE,
    -- The itch / GameJolt / store page a shipped-title attestation vouches for.
    ADD COLUMN external_publish_url VARCHAR(500);

COMMENT ON COLUMN attestations.game_jam_id IS
    'The jam a game_jam_winner or game_jam_participant attestation records.';
COMMENT ON COLUMN attestations.game_mod_id IS
    'The registered mod a game_mod_published attestation records.';

-- Idempotency: re-issuing the same attestation returns the existing row rather
-- than raising, the way 0559 did it for the security and challenge links.
CREATE UNIQUE INDEX uniq_attestation_game_jam
    ON attestations (user_id, basis, game_jam_id)
    WHERE game_jam_id IS NOT NULL AND revoked_at IS NULL;
CREATE UNIQUE INDEX uniq_attestation_game_mod
    ON attestations (user_id, basis, game_mod_id)
    WHERE game_mod_id IS NOT NULL AND revoked_at IS NULL;
CREATE INDEX idx_attestations_game_jam
    ON attestations (game_jam_id) WHERE game_jam_id IS NOT NULL;
CREATE INDEX idx_attestations_game_mod
    ON attestations (game_mod_id) WHERE game_mod_id IS NOT NULL;

-- A confirmed mod is shipped work hosted elsewhere. It gets a deliverable so it
-- counts toward the cross-domain rank exactly as a finding or a merged pull
-- request does — the point of one rank. The mod is the deliverable's parent
-- when the mod stands alone; a mod registered against a slice uses the slice.
ALTER TABLE deliverables
    ADD COLUMN game_mod_id UUID REFERENCES game_mods(id) ON DELETE CASCADE;

ALTER TABLE deliverables
    DROP CONSTRAINT deliverables_at_least_one_parent,
    ADD CONSTRAINT deliverables_at_least_one_parent CHECK (
        slice_id IS NOT NULL
        OR challenge_id IS NOT NULL
        OR tournament_submission_id IS NOT NULL
        OR mission_delivery_id IS NOT NULL
        OR security_finding_id IS NOT NULL
        OR game_mod_id IS NOT NULL
    );

CREATE INDEX idx_deliverables_game_mod
    ON deliverables (game_mod_id) WHERE game_mod_id IS NOT NULL;
