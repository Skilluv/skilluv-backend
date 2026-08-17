-- An attestation whose evidence is revoked stops standing.
--
-- ## What was missing
--
-- `AttestationsService::revoke_attestations_depending_on_deliverable` was
-- written for this and is called from nowhere. So the propagation existed as
-- an intention: a deliverable revoked for plagiarism kept its attestation
-- issued, the attestation kept feeding badges and profile counts, and the
-- record still said a stranger could go and check something that had been
-- withdrawn.
--
-- Migration 0179 met the same shape of problem and answered it the same way.
-- Adding the filter to every reader fixes today and not tomorrow — the next
-- query gets written without it and nothing fails loudly. Doing it at the
-- source means an attestation that is not revoked is one whose evidence is
-- still standing, which is what every reader already assumes.
--
-- ## When it fires
--
-- When the last live deliverable behind an attestation stops being live.
-- Not on the first: a compagnonnage attestation links several deliverables
-- and losing one of five leaves the claim standing on four. Losing all five
-- leaves it standing on nothing.
--
-- ## What it does not do
--
-- Restore anything. A deliverable re-verified later does not resurrect the
-- attestation, because somebody decided to revoke it and the engine has no
-- business overturning that. The generator will issue a new one on the next
-- recompute, and the history keeps both.

CREATE OR REPLACE FUNCTION revoke_attestations_left_without_evidence()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE attestations a
       SET revoked_at = NOW(),
           revoke_reason = COALESCE(
               a.revoke_reason,
               'le livrable sur lequel elle reposait a été révoqué')
     WHERE a.revoked_at IS NULL
       AND NEW.id = ANY(a.linked_deliverable_ids)
       AND NOT EXISTS (
             SELECT 1 FROM deliverables d
              WHERE d.id = ANY(a.linked_deliverable_ids)
                AND d.verification_status = 'verified'
                AND d.revoked_at IS NULL
       );
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION revoke_attestations_left_without_evidence() IS
    'Revokes an attestation once nothing it links is still verified. Both '
    'revocation paths reach it: fraud stamps revoked_at, moderation sets the '
    'status, and either can be the last one standing.';

DROP TRIGGER IF EXISTS trg_attestation_loses_its_evidence ON deliverables;

CREATE TRIGGER trg_attestation_loses_its_evidence
    AFTER UPDATE OF revoked_at, verification_status ON deliverables
    FOR EACH ROW
    EXECUTE FUNCTION revoke_attestations_left_without_evidence();

-- Attestations already standing on nothing. Every one of them has been
-- claiming, since the day its evidence went, that a stranger could go and
-- check something that is no longer there.
UPDATE attestations a
   SET revoked_at = NOW(),
       revoke_reason = 'le livrable sur lequel elle reposait a été révoqué'
 WHERE a.revoked_at IS NULL
   AND cardinality(a.linked_deliverable_ids) > 0
   AND NOT EXISTS (
         SELECT 1 FROM deliverables d
          WHERE d.id = ANY(a.linked_deliverable_ids)
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
   );
