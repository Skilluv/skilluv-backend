-- Handing a mission in, being told to try again, and saying afterwards how it
-- went.
--
-- ## What was missing
--
-- Migration 0192 built a mission that goes `in_progress → delivered → closed`.
-- That is a code mission: one pull request, merged or not.
--
-- A design mission is not shaped like that. A brand identity is handed in,
-- the client says the mark does not work in one colour, it is handed in
-- again. Two or three rounds is the normal case, not a failure — the same
-- thing the challenge loop already models with
-- `slice_validation_decisions`.
--
-- Without somewhere to record those rounds, the only way to express "not yet"
-- was to cancel the mission or to leave it `in_progress` while the two of
-- them argued by e-mail. Both lose the trail that an arbitration would need.
--
-- ## Why the mission status still never goes backwards
--
-- 0192 said "a mission goes forward, or it is cancelled, and it never goes
-- back", and that stays true. A delivery is submitted while the mission is
-- `in_progress`; a request for changes leaves it `in_progress`; the mission
-- reaches `delivered` only when a delivery is **accepted**.
--
-- The rounds live on the delivery, not on the mission. Nothing regresses.
--
-- ## Why an unagreed round is recorded rather than refused
--
-- A mission announces how many rounds it includes. Refusing the client's
-- fourth request would be the platform enforcing a contract it is not party
-- to; letting it pass unmarked would leave a designer with three unpaid
-- rounds and no record.
--
-- So it is recorded. `beyond_agreed_rounds` is a fact an arbitration can
-- read, and a designer can point at.

-- ═══════════════════════════════════════════════════════════════════
-- What a design mission hands over
-- ═══════════════════════════════════════════════════════════════════
--
-- The four accepted formats were code shapes: a pull request, a repository, a
-- published library, a consulting report. A design mission delivers none of
-- them, and `consulting_report` would have made every design mission lie
-- about what it produced.
--
-- Widened here rather than in 0240, which seeded the twelve design mission
-- types: those types were unusable in practice, because every mission of one
-- still had to claim a code deliverable.

ALTER TABLE missions DROP CONSTRAINT IF EXISTS missions_deliverable_format_check;

ALTER TABLE missions
    ADD CONSTRAINT missions_deliverable_format_check
    CHECK (deliverable_format IN (
        'github_pr',
        'repository_handover',
        'library_published',
        'consulting_report',
        -- Editable sources plus whatever is needed to reopen them. A
        -- deliverable nobody can reopen is not delivered.
        'design_source_files',
        -- Marks, palette, type, and the rules for using them.
        'brand_package',
        -- A rendered animation and the project behind it.
        'motion_package',
        -- A prototype somebody can walk through at a link.
        'prototype_link',
        -- Tokens, components and their documentation, handed to a team that
        -- will build on them.
        'design_system_handover'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- What was handed in, and what was said about it
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE missions
    -- How many rounds of changes the brief includes. NULL on the missions
    -- that predate this and on the ones where it makes no sense — a
    -- consulting report is delivered once.
    ADD COLUMN included_rounds SMALLINT
        CHECK (included_rounds IS NULL OR included_rounds BETWEEN 1 AND 10);

COMMENT ON COLUMN missions.included_rounds IS
    'How many rounds of changes the brief includes. Not enforced — the '
    'platform is not party to the contract — but a request past it is marked, '
    'so an arbitration has the fact and a designer has something to point at.';

CREATE TABLE mission_deliveries (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mission_id   UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    -- Numbered from one, per mission.
    round        SMALLINT NOT NULL CHECK (round BETWEEN 1 AND 20),

    delivered_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Where the work is. An https link or a stored object, like everywhere
    -- else a design artefact is handed in.
    artifact_url TEXT NOT NULL CHECK (length(artifact_url) BETWEEN 4 AND 2048),
    -- What changed since the last round, and what the person wants looked at.
    notes_md     TEXT,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The client's answer. NULL while it is waiting.
    decision     VARCHAR(20)
        CHECK (decision IS NULL OR decision IN ('accepted', 'changes_requested')),
    -- Why, when changes are asked for. Required, for the same reason a
    -- critique needs words: "not quite" costs a round and teaches nothing.
    decision_reason TEXT,
    decided_by   UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at   TIMESTAMPTZ,

    -- True when this round is past what the brief said it included.
    beyond_agreed_rounds BOOLEAN NOT NULL DEFAULT FALSE,

    UNIQUE (mission_id, round),

    CONSTRAINT mission_delivery_changes_need_a_reason
        CHECK (
            decision IS DISTINCT FROM 'changes_requested'
            OR (decision_reason IS NOT NULL AND length(decision_reason) >= 20)
        ),
    CONSTRAINT mission_delivery_decision_is_dated
        CHECK ((decision IS NULL) = (decided_at IS NULL))
);

COMMENT ON TABLE mission_deliveries IS
    'Rounds of a paid mission: what was handed in, and what the client said. '
    'Two or three rounds is the normal case for design work, not a failure. '
    'The mission status never moves backwards — it reaches `delivered` only '
    'when a delivery is accepted.';

-- The client's queue, and the designer's history of one mission.
CREATE INDEX idx_mission_deliveries_waiting
    ON mission_deliveries (mission_id, round DESC)
    WHERE decision IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What each side thought of the other
-- ═══════════════════════════════════════════════════════════════════
--
-- ## Why both directions, and why hidden until both are in
--
-- A rating that one side can read before writing their own is not a rating,
-- it is a negotiation: a designer who sees three stars writes three back, and
-- a client who knows the designer has not rated yet has a lever.
--
-- So both are written blind and revealed together — or after fourteen days,
-- whichever comes first, because a client who never rates must not be able to
-- suppress the designer's rating for ever by staying silent.
--
-- ## Why the comment is optional and the score is not
--
-- A score with no comment is thin and still useful. A comment with no score
-- is an opinion nobody can aggregate, and the aggregate is the whole point:
-- somebody deciding whether to work with an enterprise wants the number
-- first.

CREATE TABLE mission_ratings (
    mission_id  UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    -- `client_to_talent` or `talent_to_client`. One of each per mission.
    direction   VARCHAR(20) NOT NULL
        CHECK (direction IN ('client_to_talent', 'talent_to_client')),

    rater_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Who is being rated. For `talent_to_client` this is whoever published
    -- the mission on the enterprise's behalf.
    rated_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    rating      SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment_md  TEXT CHECK (comment_md IS NULL OR length(comment_md) <= 4000),

    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (mission_id, direction),

    CONSTRAINT mission_rating_is_not_self_addressed
        CHECK (rater_id <> rated_id)
);

COMMENT ON TABLE mission_ratings IS
    'One rating per direction per mission, written blind. Revealed when both '
    'are in, or fourteen days after the first — so a silent client cannot '
    'suppress a designer''s rating for ever.';

-- Somebody's received ratings, for the aggregate on their profile.
CREATE INDEX idx_mission_ratings_received
    ON mission_ratings (rated_id, created_at DESC);
