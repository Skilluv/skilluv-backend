-- Revision rounds, for every domain that delivers to somebody.
--
-- ## Why this is not audio-specific
--
-- The audio backlog asked for revision rounds and pointed at a design ticket
-- for the mechanism, on the grounds that design had asked first. Both are
-- describing the same thing: creative work delivered to a person who then
-- says what to change, a bounded number of times, in writing.
--
-- Writing it once, keyed by domain, is the difference between one mechanism
-- and four that drift. Design and game get it by inserting rows; nothing here
-- mentions audio except the vocabulary and the limit.
--
-- ## Why the number of rounds is a limit and not a courtesy
--
-- Unbounded revision is how a fixed-price creative job becomes unpaid work,
-- and the person who loses is always the one who delivered. The limit exists
-- to be quoted: five rounds, stated before the work starts, and a sixth is a
-- new engagement rather than a favour.
--
-- Stored per domain because the right number is not the same everywhere — a
-- logo converges faster than a soundtrack — and stored in a row because it is
-- a commercial policy, not a fact about software.
--
-- ## Why a round is closed by the person who asked for it
--
-- `resolved_at` is set when the requester says the change landed, not when
-- the maker says they made it. A round the maker can close alone is a counter
-- the maker can run down, and the whole point of counting is that both sides
-- agree on the count.

CREATE TABLE revision_round_kinds (
    slug VARCHAR(40) PRIMARY KEY,
    skill_domain VARCHAR(30) NOT NULL REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    sort_order SMALLINT NOT NULL DEFAULT 100
);

COMMENT ON TABLE revision_round_kinds IS
    'What a revision round can be about, per domain. Naming the kind is what '
    'makes a history readable: five rounds of "mix" and five rounds of '
    '"the brief changed" are different stories about the same count.';

INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
    ('audio_mood_revision', 'audio', 'Ambiance',
     'L''intention musicale ou sonore n''est pas celle attendue. La révision la plus coûteuse : elle remet en cause l''écriture.', 10),
    ('audio_arrangement_revision', 'audio', 'Arrangement',
     'La matière est la bonne, la disposition non : instrumentation, densité, structure.', 20),
    ('audio_mix_revision', 'audio', 'Mixage',
     'Équilibres, panoramique, espace. L''écriture ne bouge pas.', 30),
    ('audio_master_revision', 'audio', 'Mastering',
     'Niveau final, loudness, plafond de crête pour une destination donnée.', 40),
    ('audio_texture_revision', 'audio', 'Texture sonore',
     'Le grain d''un bruitage : matière, longueur, attaque.', 50),
    ('audio_alternate_take', 'audio', 'Prise alternative',
     'Une autre interprétation de la même ligne. Propre au travail de voix.', 60),
    ('audio_delivery_revision', 'audio', 'Livraison',
     'Formats, nommage, stems, documentation. Le contenu est accepté.', 70),
    ('audio_brief_change', 'audio', 'Changement de brief',
     'La demande a changé après le début du travail. Comptée pour ce qu''elle est.', 80);

CREATE TABLE slice_revision_rounds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    round_no SMALLINT NOT NULL CHECK (round_no > 0),
    kind VARCHAR(40) NOT NULL REFERENCES revision_round_kinds(slug) ON UPDATE CASCADE,

    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    -- What to change, in words. A round with no statement is a rejection with
    -- no appeal, and the person who has to act on it cannot.
    notes_md TEXT NOT NULL CHECK (btrim(notes_md) <> ''),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Closed by the requester, when they agree the change landed.
    resolved_at TIMESTAMPTZ,
    resolved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    resolution_note TEXT,

    UNIQUE (slice_id, round_no),
    CONSTRAINT revision_round_resolution_is_complete CHECK (
        (resolved_at IS NULL) = (resolved_by IS NULL)
    )
);

CREATE INDEX idx_revision_rounds_open
    ON slice_revision_rounds (slice_id, round_no)
    WHERE resolved_at IS NULL;

COMMENT ON COLUMN slice_revision_rounds.resolved_at IS
    'Set by the person who asked for the change, not by the one who made it. '
    'A counter the maker can run down alone is not a count both sides agree '
    'on, which is the only kind worth keeping.';

-- ═══════════════════════════════════════════════════════════════════
-- The limit
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE revision_round_limits (
    skill_domain VARCHAR(30) PRIMARY KEY
        REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    max_rounds SMALLINT NOT NULL CHECK (max_rounds BETWEEN 1 AND 20),
    rationale TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO revision_round_limits (skill_domain, max_rounds, rationale) VALUES
    ('audio', 5,
     'Cinq. Une révision d''ambiance, une d''arrangement, une de mixage, une de '
     'mastering et une de livraison couvrent le trajet complet d''une pièce. '
     'Au-delà, ce n''est plus la même commande.');

CREATE TRIGGER trg_revision_round_limits_updated_at
    BEFORE UPDATE ON revision_round_limits
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- The limit is enforced here rather than in a service, because the count it
-- protects is the one both sides quote. A service check races with itself the
-- moment two people press the button at the same time.
CREATE FUNCTION trg_slice_revision_rounds_within_limit() RETURNS TRIGGER AS $$
DECLARE
    domain_of_slice VARCHAR;
    allowed SMALLINT;
    used SMALLINT;
BEGIN
    SELECT primary_domain INTO domain_of_slice
      FROM project_slices WHERE id = NEW.slice_id;

    SELECT max_rounds INTO allowed
      FROM revision_round_limits WHERE skill_domain = domain_of_slice;

    -- A domain that has not set a limit does not have one. Refusing instead
    -- would make this migration break every domain it does not mention.
    IF allowed IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT count(*) INTO used
      FROM slice_revision_rounds WHERE slice_id = NEW.slice_id AND id <> NEW.id;

    IF used >= allowed THEN
        RAISE EXCEPTION
            'slice % has used its % revision rounds', NEW.slice_id, allowed
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_slice_revision_rounds_within_limit
    BEFORE INSERT ON slice_revision_rounds
    FOR EACH ROW EXECUTE FUNCTION trg_slice_revision_rounds_within_limit();
