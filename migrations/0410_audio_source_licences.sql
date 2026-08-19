-- Where every sound came from, and under what licence.
--
-- ## Why this is not paperwork
--
-- In every other domain, a provenance problem makes the work weaker. Here it
-- makes it unusable: one untraced loop in a track means a client cannot ship
-- it, a platform can take it down, and the composer finds out eighteen months
-- later. The review grid of 0405 puts provenance in the common criteria for
-- that reason, and a criterion a reviewer has to establish by asking is a
-- criterion nobody establishes.
--
-- So the declaration is a set of rows the author writes, and the attestation
-- generator reads. `audio_composition_published` and `audio_soundpack_delivered`
-- do not issue without it.
--
-- ## The difference between "no sources" and "not asked"
--
-- An empty list is ambiguous — everything original, or nobody filled the form
-- — and those two must not read the same to a generator that is about to
-- assert something publicly. `audio_sources_declared_at` on the slice is the
-- author saying the list is complete, whatever length it has. A track with no
-- samples has zero rows and a declaration; a track nobody documented has zero
-- rows and none.
--
-- ## Attribution is required where the licence requires it
--
-- A Creative Commons BY licence is free and conditional: using the sound
-- without the credit line is an infringement, not an oversight. The
-- constraint refuses the row rather than accepting a declaration that is
-- itself a breach.

CREATE TABLE audio_source_licences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,

    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        -- Made by the author. Nothing to attribute, everything to keep.
        'original',
        -- Public domain or CC0. Free of conditions.
        'public_domain',
        -- Creative Commons with conditions — BY, BY-SA, BY-NC. Attribution
        -- required, and some of them forbid the commercial use a mission is.
        'creative_commons',
        -- Bought or subscribed, royalty-free for the use. Splice, packs,
        -- sample libraries.
        'royalty_free',
        -- A negotiated licence for a named use.
        'licensed_commercial',
        -- Somebody else's performance or writing, licensed from them.
        'third_party_work'
    )),

    -- What it is and where it came from, in words a reader can check.
    source_name VARCHAR(200) NOT NULL CHECK (btrim(source_name) <> ''),
    source_url TEXT,
    -- The licence's own identifier when it has one: 'CC-BY-4.0', 'CC0-1.0'.
    licence_identifier VARCHAR(60),
    -- The credit line, verbatim, as it must appear.
    attribution_text TEXT,

    purchased_from VARCHAR(120),
    purchase_price_eur NUMERIC(10,2)
        CHECK (purchase_price_eur IS NULL OR purchase_price_eur >= 0),
    purchase_date DATE,

    -- Whether the licence allows the commercial use a paid mission implies.
    -- NULL means the author has not established it, which is different from
    -- "no": a reviewer can ask, and a generator can refuse to guess.
    permits_commercial_use BOOLEAN,

    declared_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A conditional licence without its credit line is a breach written down.
    CONSTRAINT audio_licence_cc_states_its_attribution CHECK (
        kind <> 'creative_commons' OR btrim(COALESCE(attribution_text, '')) <> ''
    ),
    -- Anything acquired says where from. "Bought somewhere" is not provenance.
    CONSTRAINT audio_licence_acquired_says_where CHECK (
        kind NOT IN ('royalty_free', 'licensed_commercial', 'third_party_work')
        OR btrim(COALESCE(purchased_from, '')) <> ''
        OR btrim(COALESCE(source_url, '')) <> ''
    )
);

COMMENT ON TABLE audio_source_licences IS
    'Every source used in an audio artefact and the licence it came under. '
    'Read by the attestation generators: a composition or a pack is not '
    'attested until the list is complete, because an untraced sample makes '
    'the whole delivery unusable rather than merely weaker.';

CREATE INDEX idx_audio_source_licences_by_slice
    ON audio_source_licences (slice_id, kind);

CREATE TRIGGER trg_audio_source_licences_updated_at
    BEFORE UPDATE ON audio_source_licences
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The declaration itself
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE project_slices
    ADD COLUMN audio_sources_declared_at TIMESTAMPTZ,
    ADD COLUMN audio_sources_declared_by UUID REFERENCES users(id) ON DELETE SET NULL;

COMMENT ON COLUMN project_slices.audio_sources_declared_at IS
    'When the author stated that the source list is complete. Distinct from '
    'the list being empty: a wholly original track has no rows and a '
    'declaration, and a track nobody documented has neither. The generators '
    'read this, not the row count.';

-- A declaration is a statement somebody makes; it cannot exist without them.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_audio_declaration_has_an_author CHECK (
        (audio_sources_declared_at IS NULL) = (audio_sources_declared_by IS NULL)
    );

-- A licence row belongs to an audio slice, for the same reason a file does.
CREATE FUNCTION trg_audio_licences_belong_to_an_audio_slice() RETURNS TRIGGER AS $$
DECLARE
    kind VARCHAR;
BEGIN
    SELECT slice_type INTO kind FROM project_slices WHERE id = NEW.slice_id;
    IF kind <> 'audio_artifact' THEN
        RAISE EXCEPTION 'slice % is a %, not an audio_artifact', NEW.slice_id, kind
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audio_licences_belong_to_an_audio_slice
    BEFORE INSERT OR UPDATE OF slice_id ON audio_source_licences
    FOR EACH ROW EXECUTE FUNCTION trg_audio_licences_belong_to_an_audio_slice();
