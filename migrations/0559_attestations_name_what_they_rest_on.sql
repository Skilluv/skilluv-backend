-- An attestation that rests on a challenge, and the two uniqueness rules that
-- were missing.
--
-- ## The gap
--
-- `uniq_attestations_artefact_per_deliverable` (0198) makes an artefact
-- attestation unique per person, basis and deliverable. Every domain until now
-- has been covered by it, because every domain's attestations rest on a
-- deliverable.
--
-- Security has three that do not, and each of them is a real thing to attest:
--
--   * a captured flag and a passed lab — machine graded, no human read an
--     artefact, and 0546 explains why they deliberately produce no deliverable;
--   * an independent co-discovery — the second person to find a vulnerability
--     has a finding of their own and no fix to their name.
--
-- With nothing constraining them, a hook that ran twice would issue two
-- identical attestations, and the profile would show a person having solved the
-- same challenge twice. That is the failure this migration closes — before
-- anything can produce it, rather than after somebody notices.
--
-- ## Why `challenge_template_id` did not exist before
--
-- Five hundred challenge templates, and no attestation could point at one.
-- Nothing needed it: the training catalogue produced submissions, and
-- attestations came from slices. The security practice catalogue is the first
-- place where completing a *challenge* is the attestable act, so the column
-- arrives with the first thing that needs it.

ALTER TABLE attestations
    ADD COLUMN challenge_template_id UUID
        REFERENCES challenge_templates(id) ON DELETE SET NULL;

COMMENT ON COLUMN attestations.challenge_template_id IS
    'The catalogue challenge this attestation rests on, where completing the '
    'challenge is itself the attestable act — a captured flag, a passed lab. '
    'Null for everything that rests on a deliverable instead.';

-- ═══════════════════════════════════════════════════════════════════
-- The two rules
-- ═══════════════════════════════════════════════════════════════════
--
-- Partial, revocation-aware, and shaped exactly like the 0198 one: a revoked
-- attestation must not block a later legitimate re-issue, which is what a
-- revocation followed by a corrected finding looks like.

CREATE UNIQUE INDEX uniq_attestations_per_challenge
    ON attestations (user_id, basis, challenge_template_id)
    WHERE challenge_template_id IS NOT NULL AND revoked_at IS NULL;

CREATE UNIQUE INDEX uniq_attestations_per_security_finding
    ON attestations (user_id, basis, security_finding_id)
    WHERE security_finding_id IS NOT NULL AND revoked_at IS NULL;

-- An attestation names at most one kind of thing it rests on. Both at once
-- would mean two answers to "what is this about", and the verification page
-- would have to pick one.
ALTER TABLE attestations
    ADD CONSTRAINT an_attestation_rests_on_one_kind_of_thing CHECK (
        challenge_template_id IS NULL OR security_finding_id IS NULL
    );
