-- The leadership domain: opened, given its trades, its surface and its bases.
--
-- ## What was already there
--
-- Migration 0400 declared `leadership` as a row with `is_active = FALSE`, and
-- 0204 gave it six craft-score tiers. This opens it, for the same reason
-- quality was opened three migrations ago: there is now a catalogue behind it.
--
-- ## The problem this domain has that no other one does
--
-- Almost everything a leader produces is internal. A roadmap names unreleased
-- products, an RFC names a system's weaknesses, a team health audit names a
-- team. The domains that came before could ask for a public link because their
-- artefacts are publishable by nature; this one cannot, and pretending
-- otherwise would produce a domain where only the unemployed can build a
-- record.
--
-- So this domain carries a redaction state, and the state decides what an
-- attestation may say. Three levels, and the middle one is the interesting
-- one:
--
--   * `public` — the document can be read. Attested like any other artefact.
--   * `anonymised` — the author has rewritten it so that the organisation,
--     the teams and the people in it cannot be identified, and a reviewer has
--     confirmed that. Attested with the document attached.
--   * `confidential` — the document cannot be shown at all. Attested with an
--     abstract claim: what kind of artefact, at what scale, in what industry,
--     and nothing else.
--
-- ## Why anonymisation is declared and confirmed rather than performed
--
-- The backlog asks for the platform to "mask company name, team names,
-- individual names". Nothing can do that reliably on prose. A regular
-- expression that removes a company's name leaves the product name, the
-- office city, the head count and the three customers mentioned in paragraph
-- four — and a system that claims to have anonymised a document is worse than
-- one that does not, because somebody trusts it.
--
-- The mechanism here is the one migration 0412 used for audio licences and
-- that works: the author declares, a reviewer confirms, and the attestation
-- waits for both. A human read it. That is the only claim we can honestly
-- make.

-- ═══════════════════════════════════════════════════════════════════
-- The domain opens
-- ═══════════════════════════════════════════════════════════════════

UPDATE skill_domains
   SET is_active = TRUE,
       name = 'Leadership',
       description =
           'Decide, arbitrate, and hold a direction with other people. The '
           'trades whose output is a document somebody else acts on.'
 WHERE slug = 'leadership';

-- ═══════════════════════════════════════════════════════════════════
-- Six trades
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated)
VALUES
    ('lead-product',
     'Product Manager',
     'Decides what gets built and what does not, and writes down the reason '
     'so the decision can be argued with later.',
     'leadership', ARRAY['design', 'code'],
     ARRAY['roadmap', 'prd', 'okr', 'discovery'], TRUE),

    ('lead-tech',
     'Tech Lead / Staff Engineer',
     'Writes the decision down before it is taken: alternatives, trade-offs, '
     'and what would have to be true for this to be the wrong call.',
     'leadership', ARRAY['code', 'ops'],
     ARRAY['rfc', 'adr', 'architecture'], TRUE),

    ('lead-project',
     'Delivery Lead / Producer',
     'Holds a plan that survives contact with reality: dependencies, risks, '
     'and the date somebody outside the team can rely on.',
     'leadership', ARRAY['game', 'code'],
     ARRAY['delivery', 'risk', 'coordination'], TRUE),

    ('lead-people',
     'People Manager',
     'Builds the frame other people grow in: expectations, hiring, one-to-ones, '
     'and the conversations nobody wants to have.',
     'leadership', ARRAY['soft_skills'],
     ARRAY['hiring', 'career-ladder', 'team-health'], TRUE),

    ('lead-community',
     'Community Lead / DevRel',
     'Builds a place people come back to, and can say why they came back '
     'rather than counting who showed up once.',
     'leadership', ARRAY['soft_skills', 'design'],
     ARRAY['community', 'devrel', 'ambassadors'], TRUE),

    ('lead-mentor',
     'Mentor / Curriculum Lead',
     'Designs how somebody gets from where they are to where the work is, '
     'and runs it with a cohort rather than describing it.',
     'leadership', ARRAY['soft_skills'],
     ARRAY['mentoring', 'curriculum', 'cohort'], TRUE)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Who may review leadership work
-- ═══════════════════════════════════════════════════════════════════
--
-- Five families for six trades, and the grouping is real here where in
-- quality it collapsed. Somebody who can read a roadmap can read a delivery
-- plan: both are a sequence of commitments with dependencies and a stated
-- cost of being wrong, and the competence is reading whether the plan survives
-- its own assumptions.
--
-- `people` and `teaching` are kept apart although both are about growing
-- people, because the artefacts are not the same object. A career ladder is a
-- contract between an organisation and its staff; a curriculum is a sequence
-- of things somebody has to be able to do. Reading the first well says nothing
-- about reading the second.

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    -- What gets built, in what order, and by when (2)
    ('lead-product',   'delivery'),
    ('lead-project',   'delivery'),
    -- How it gets built (1)
    ('lead-tech',      'technical'),
    -- Who builds it, and under what expectations (1)
    ('lead-people',    'people'),
    -- Who else shows up (1)
    ('lead-community', 'community'),
    -- How somebody learns to (1)
    ('lead-mentor',    'teaching')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;

-- ═══════════════════════════════════════════════════════════════════
-- The surface leadership work lives on
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('leadership_artifact', 'leadership', 'Leadership document',
     'A roadmap, an RFC, a retrospective, a playbook, a curriculum. Judged on '
     'one question: could somebody who was not in the room act on it.', 80)
ON CONFLICT (slug) DO NOTHING;

ALTER TABLE project_slices
    ADD COLUMN leadership_subtype VARCHAR(30),
    -- How much of it can be shown.
    --
    -- NULL on everything that is not a leadership artefact. On one, it is
    -- required — see the CHECK below — because "unset" and "public" reading
    -- the same is how an internal roadmap ends up on a public profile.
    ADD COLUMN redaction_state VARCHAR(15),
    -- The author saying they have rewritten it so nobody can be identified.
    -- A claim, and treated as one: the attestation waits for a reviewer to
    -- confirm it as well.
    ADD COLUMN redaction_declared_at TIMESTAMPTZ,
    ADD COLUMN redaction_confirmed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN redaction_confirmed_at TIMESTAMPTZ,
    -- What a confidential artefact may say about itself: the shape of the
    -- organisation, not its name. `{"industry": …, "org_size": …,
    -- "team_size": …, "duration_months": …}`.
    --
    -- An object rather than columns because the useful keys differ by trade —
    -- a community strategy is described by audience size and a career ladder
    -- by head count — and columns would have meant six nulls each time.
    ADD COLUMN leadership_context JSONB,
    -- Whether the organisation actually took the proposal up, and where that
    -- can be seen.
    --
    -- Two attestations rest on this one column. A written decision earns
    -- `leadership_decision_recorded` on being verified, whatever happened to
    -- it; one that was adopted earns `leadership_rfc_accepted` as well. The
    -- separation is the point: a domain that only attests accepted proposals
    -- teaches people to propose what will pass, and the hardest technical
    -- writing on this platform will be the rejected kind.
    ADD COLUMN leadership_adopted_at TIMESTAMPTZ,
    ADD COLUMN leadership_adoption_evidence_url VARCHAR(500);

-- `target_domain` is not added here. Migration 0450 put it on the table for
-- both domains at once, for the reason it gives: two columns holding the same
-- values, read by the same two queries, drift.

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_leadership_subtype_values
    CHECK (leadership_subtype IS NULL OR leadership_subtype IN (
        'roadmap',           -- what gets built, in what order, and why
        'prd',               -- one feature, argued from a problem
        'rfc',               -- a technical decision with its alternatives
        'adr',               -- a decision recorded after the fact
        'delivery_plan',     -- dependencies, risks, and a date
        'retrospective',     -- what was learned, and what changed because of it
        'playbook',          -- how a team does a recurring thing
        'career_ladder',     -- what is expected at each level
        'hiring_process',    -- how somebody is assessed, and by what rubric
        'team_health_audit', -- what the team says, and the plan that followed
        'community_strategy',-- who is being built for, and what brings them back
        'cohort_curriculum', -- how somebody gets from here to the work
        'okrs_doc'           -- what a quarter is being spent on
    ));

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_leadership_subtype_belongs CHECK (
        (slice_type = 'leadership_artifact') = (leadership_subtype IS NOT NULL)
    );

-- Every leadership artefact says how much of it can be shown, and nothing
-- else carries the question at all.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_redaction_belongs CHECK (
        (slice_type = 'leadership_artifact') = (redaction_state IS NOT NULL)
    ),
    ADD CONSTRAINT project_slices_redaction_values CHECK (
        redaction_state IS NULL
        OR redaction_state IN ('public', 'anonymised', 'confidential')
    );

-- A confirmation is a person and a moment, or neither.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_redaction_confirmation_is_complete CHECK (
        (redaction_confirmed_at IS NULL) = (redaction_confirmed_by IS NULL)
    ),
    -- Nobody confirms a rewriting that was never declared.
    ADD CONSTRAINT project_slices_confirmation_follows_declaration CHECK (
        redaction_confirmed_at IS NULL OR redaction_declared_at IS NOT NULL
    ),
    -- A confidential artefact has to say something about itself, or the
    -- attestation it earns would claim nothing at all.
    ADD CONSTRAINT project_slices_confidential_work_describes_itself CHECK (
        redaction_state IS DISTINCT FROM 'confidential'
        OR (leadership_context IS NOT NULL
            AND jsonb_typeof(leadership_context) = 'object'
            AND leadership_context <> '{}'::JSONB)
    );

-- Adoption is a claim about somebody else's organisation, so it names where
-- it can be seen — a merged proposal, a changelog, a published decision log.
-- A confidential artefact is the exception: there is nothing public to point
-- at, and the reviewer who confirmed the redaction is who confirms this too.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_adoption_shows_itself CHECK (
        leadership_adopted_at IS NULL
        OR leadership_adoption_evidence_url IS NOT NULL
        OR redaction_state = 'confidential'
    ),
    ADD CONSTRAINT project_slices_adoption_evidence_is_a_link CHECK (
        leadership_adoption_evidence_url IS NULL
        OR leadership_adoption_evidence_url ~ '^https://'
    );

COMMENT ON COLUMN project_slices.leadership_adopted_at IS
    'Whether the organisation took the proposal up. A written decision earns '
    'its attestation either way; an adopted one earns a second. Attesting '
    'only what passed teaches people to propose what will pass.';

COMMENT ON COLUMN project_slices.redaction_state IS
    'How much of a leadership artefact can be shown. Required on one and '
    'absent everywhere else: "unset" and "public" reading the same is how an '
    'internal roadmap reaches a public profile.';

COMMENT ON COLUMN project_slices.redaction_declared_at IS
    'The author stating they have rewritten the document so that nobody can '
    'be identified. A claim. The attestation also waits for '
    '`redaction_confirmed_at`, because a system that claims to have '
    'anonymised prose is worse than one that does not — somebody trusts it.';

CREATE INDEX idx_slices_leadership_subtype
    ON project_slices (leadership_subtype)
    WHERE leadership_subtype IS NOT NULL;

-- What the reviewer queue for redaction reads: declared, not yet confirmed.
CREATE INDEX idx_slices_redaction_pending
    ON project_slices (redaction_declared_at)
    WHERE redaction_declared_at IS NOT NULL AND redaction_confirmed_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What a leadership attestation can rest on
-- ═══════════════════════════════════════════════════════════════════
--
-- Eight bases where the backlog listed seven. The one added is
-- `leadership_decision_recorded`, and it exists because the backlog's
-- `leadership_rfc_accepted` counts an outcome the author does not control: an
-- RFC can be excellent and rejected, and a domain that only attests accepted
-- proposals teaches people to propose what will pass.
--
-- Both are kept. The accepted one is worth more — it is in the weights — and
-- the recorded one means the work existed and was read.
--
-- `requires_deliverable` is FALSE for four of them, which is more exceptions
-- than any other domain takes, and each one is a period rather than a file: a
-- cohort run, a retrospective whose actions landed, a community initiative
-- that changed a number, a featuring. A period is not a document, and
-- demanding one would have made half of this domain unattestable.

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order)
VALUES
    ('leadership_roadmap_validated', 'leadership', 'Roadmap validated',
     'A sequence of commitments with its dependencies, its risks and what it '
     'deliberately leaves out. Reviewed and accepted.',
     TRUE, 10),
    ('leadership_decision_recorded', 'leadership', 'Technical decision recorded',
     'A decision written down before it was taken: the alternatives, the '
     'trade-offs, and what would make it the wrong call.',
     TRUE, 20),
    ('leadership_rfc_accepted', 'leadership', 'Proposal accepted',
     'A written proposal an organisation adopted. Worth more than one that '
     'was merely well argued, and both are recorded — a domain that only '
     'attests accepted proposals teaches people to propose what will pass.',
     TRUE, 30),
    ('leadership_retrospective_facilitated', 'leadership', 'Retrospective facilitated',
     'A retrospective whose action items were owned, dated, and mostly done '
     'within the quarter. The half everybody skips.',
     FALSE, 40),
    ('leadership_cohort_completed', 'leadership', 'Cohort led to the end',
     'A cohort run from start to graduation, with most of the people who '
     'joined finishing it.',
     FALSE, 50),
    ('leadership_playbook_published', 'leadership', 'Playbook published',
     'How a team does a recurring thing, written so the team keeps doing it '
     'after its author has left.',
     TRUE, 60),
    ('leadership_community_initiative_impact', 'leadership', 'Community initiative with an effect',
     'An initiative that moved a number somebody can name, in a direction '
     'somebody wanted.',
     FALSE, 70),
    ('leadership_people_framework_validated', 'leadership', 'People framework validated',
     'A career ladder, a hiring process or a team health audit: a structure '
     'other people are assessed or grown inside, with expectations somebody '
     'can be measured against.',
     TRUE, 75),
    ('featured_leader', 'leadership', 'Featured by the leadership community',
     'Leadership work the community singled out as exemplary.',
     FALSE, 80)
ON CONFLICT (basis) DO NOTHING;
