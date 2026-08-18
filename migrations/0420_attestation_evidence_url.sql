-- Where the evidence is, for the attestations that rest on somebody else's page.
--
-- ## The gap
--
-- Migration 0178 gave attestations a `basis` and a rule: an artefact basis
-- must link the deliverable that carries it, so a stranger can open the thing
-- and check. That works when the evidence is on this platform.
--
-- Some of it never is. `code_rfc_accepted` points at a proposal in somebody
-- else's repository. `code_standard_contribution` points at a specification.
-- `audio_project_credited` points at a game's end titles, a film's credit
-- roll, a podcast description — the normal outcome of audio work, and the only
-- trace it leaves. In each case the deliverable link says what was made and
-- nothing says where the recognition appears, which is the half a reader
-- actually wants to check.
--
-- ## Why not `metadata`
--
-- Because a reader follows it. A URL in a JSONB blob is a URL no template
-- renders and no validator checks, and the whole value of this column is that
-- the public attestation page can offer the link.

ALTER TABLE attestations
    ADD COLUMN evidence_url TEXT;

COMMENT ON COLUMN attestations.evidence_url IS
    'Where the recognition appears, when it appears somewhere this platform '
    'does not own: an accepted proposal, a specification, a credit roll. '
    'Complements the deliverable link rather than replacing it — one says what '
    'was made, the other says who acknowledged it.';

ALTER TABLE attestations
    ADD CONSTRAINT attestations_evidence_url_is_a_link CHECK (
        evidence_url IS NULL OR evidence_url ~ '^https?://'
    );

-- ═══════════════════════════════════════════════════════════════════
-- One basis requires it, and only one
-- ═══════════════════════════════════════════════════════════════════
--
-- `audio_project_credited` is the only basis whose entire claim is "somebody
-- else's published work names this person" and that has nothing else to point
-- at. The deliverable link says what was made; without the credit URL there is
-- no way to check the part that matters, and the attestation is a sentence.
--
-- `code_rfc_accepted` and `code_standard_contribution` look like the same
-- shape and are not. Migration 0178 decided they need no artefact link because
-- the proposal is often the artefact, they have been issued that way since,
-- and requiring the URL now would refuse claims the platform already accepts —
-- retroactively, for records already made. The column is offered to them and
-- not demanded: `attestations_evidence_url_is_a_link` still applies when one
-- is given.

CREATE FUNCTION trg_attestations_acknowledgement_says_where() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.basis = 'audio_project_credited'
       AND btrim(COALESCE(NEW.evidence_url, '')) = '' THEN
        RAISE EXCEPTION
            'an attestation on basis % must say where the credit appears', NEW.basis
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_attestations_acknowledgement_says_where
    BEFORE INSERT OR UPDATE OF basis, evidence_url ON attestations
    FOR EACH ROW EXECUTE FUNCTION trg_attestations_acknowledgement_says_where();
