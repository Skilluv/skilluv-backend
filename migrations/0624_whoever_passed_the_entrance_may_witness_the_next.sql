-- The reviewer ladder gets a first rung, so a cold start stops needing an
-- admin for every entrance forever.
--
-- ## The circle
--
-- A rite verdict needs `admin`, `mentor` or `domain_curator` (services/
-- reviews.rs). Of those, only `mentor` is granted automatically, and it wants
-- five attestations received or three mentorship sessions given
-- (capabilities_engine.rs). Attestations come from work somebody validated.
-- Validating needs one of those three capabilities.
--
-- So: no reviewer, no completed rite, no attestation, no mentor, no reviewer.
-- Nothing in the platform breaks that circle. Today it is broken by hand, by
-- an account somebody granted `admin` to, and it stays broken by hand for
-- every newcomer of every domain, forever.
--
-- This is not a design problem, which is where it was first filed. The code
-- rite lands in the same queue as the other eleven: routes/onboarding.rs
-- inserts the pull request as a deliverable with `verifiable_by =
-- 'human_review'`, `verification_status = 'pending'`, and queues a review
-- task. Opening a pull request is a gesture, not a verdict. Twelve domains,
-- one circle.
--
-- ## The rung
--
-- Whoever passed the entrance of a trade may witness the next entrance of
-- that same trade. That is what a compagnonnage does, and it is the smallest
-- thing that makes the loop self-sustaining: person one still needs a
-- steward, person two can be read by person one.
--
-- `rite_reviewer:{domain}` is deliberately the narrowest capability in the
-- catalogue. It admits one kind of verdict, on one kind of deliverable, in
-- one domain: a published `is_domain_rite` template of the same domain the
-- holder passed. It grants nothing on slices, missions, bounties, contests or
-- attestations, and the gate in services/reviews.rs checks the deliverable is
-- a rite before honouring it rather than trusting the name.
--
-- The rules that already protect a verdict are untouched and still apply
-- first: nobody reviews their own deliverable, and a verdict is still a row
-- in `reviews` with a body attached to a person.
--
-- ## Why not lower the mentor threshold instead
--
-- `mentor` is cross-domain and carries mentorship sessions, which are paid.
-- Somebody who has written one HELLO is not that, and reaching the rung by
-- lowering a bar that guards paid work would trade one real guarantee for a
-- convenience. This adds a capability rather than weakening one.
--
-- ## Reversibility
--
-- If it turns out newcomers reading newcomers costs more than it gives, the
-- rows come out of `capability_catalog` and the grant stops. The FK from
-- `user_capabilities` means the held rows have to go with them, which is a
-- deliberate friction: revoking a right people were given is a decision, not
-- a cleanup.

INSERT INTO capability_catalog (capability, family, scope, description, is_derived)
SELECT
    'rite_reviewer:' || d.domain,
    'rite_reviewer',
    d.domain,
    'May read the Bonjour Skilluv rite of ' || d.domain || ', and nothing else. '
        || 'Granted automatically when the holder''s own rite in that domain is completed.',
    FALSE
FROM (
    VALUES
        ('code'), ('design'), ('game'), ('security'), ('ops'), ('ai'),
        ('soft_skills'), ('audio'), ('quality'), ('leadership'),
        ('communication'), ('education')
) AS d(domain)
ON CONFLICT (capability) DO NOTHING;

-- The twelve are written out rather than read from a table because
-- `validators::SKILL_DOMAINS` is the authority on which domains a person may
-- declare, and it is a Rust constant. A test asserts the two agree, which is
-- the same shape of guard the rest of the schema uses for this constant.

DO $guard$
DECLARE
    listed BIGINT;
BEGIN
    SELECT count(*) INTO listed
      FROM capability_catalog
     WHERE family = 'rite_reviewer';
    IF listed <> 12 THEN
        RAISE EXCEPTION
            'expected twelve rite reviewer capabilities, found %', listed;
    END IF;
END $guard$;
