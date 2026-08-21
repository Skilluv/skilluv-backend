-- A revoked deliverable stops counting, whichever door it was revoked
-- through.
--
-- ## The disagreement
--
-- Two paths revoke a deliverable and they did different things. Moderation
-- sets `verification_status = 'revoked'`. Fraud review sets `revoked_at` and
-- leaves the status at 'verified'.
--
-- Fifteen queries count verified deliverables by status alone — the rank
-- computation, the attestation generators, the timeline, the tracks. So a
-- deliverable revoked for fraud kept counting: the person caught cheating
-- kept the rank the cheating earned, and the attestations built on it stayed
-- issuable.
--
-- ## Why a trigger rather than fifteen edits
--
-- Adding `AND revoked_at IS NULL` to every reader fixes today and not
-- tomorrow: the sixteenth query gets written without it, and nothing fails
-- loudly when it does. Making the two columns agree at the source means
-- `verification_status = 'verified'` is true whenever it is written, which
-- is what every one of those queries already assumes.

CREATE OR REPLACE FUNCTION deliverable_revocation_clears_verification()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.revoked_at IS NOT NULL AND NEW.verification_status <> 'revoked' THEN
        NEW.verification_status := 'revoked';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION deliverable_revocation_clears_verification() IS
    'Keeps verification_status truthful when a deliverable is revoked by a '
    'path that only stamps revoked_at. Readers count on the status alone.';

DROP TRIGGER IF EXISTS trg_deliverable_revocation ON deliverables;

CREATE TRIGGER trg_deliverable_revocation
    BEFORE INSERT OR UPDATE OF revoked_at, verification_status ON deliverables
    FOR EACH ROW
    EXECUTE FUNCTION deliverable_revocation_clears_verification();

-- Rows already in this state. Anything revoked but still reading as verified
-- has been counting since it was revoked.
UPDATE deliverables
   SET verification_status = 'revoked'
 WHERE revoked_at IS NOT NULL
   AND verification_status <> 'revoked';
