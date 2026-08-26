-- Retrospectives, and the half everybody skips.
--
-- ## Why a retrospective needs a table at all
--
-- Every other leadership artefact is a document: it is written, reviewed, and
-- that is the whole of it. A retrospective is not finished when the notes are
-- published — it is finished when the things it decided actually happened, or
-- were explicitly dropped.
--
-- That is also the only honest way to attest one. "Facilitated a retro" is a
-- claim anybody can make about any hour they spent in a room. "Facilitated a
-- retro whose action items were owned, dated and mostly closed inside a
-- quarter" is a claim with rows behind it, and it is what
-- `leadership_retrospective_facilitated` rests on.
--
-- ## Why the actions are not `ops_incident_actions`
--
-- They are the same shape — a description, an owner, a date, and a reason when
-- it is dropped — and merging them was considered. It was not done, and the
-- reason is the parent: an incident action hangs off an incident, a
-- retrospective action hangs off a retrospective, and a single table would
-- need either two nullable foreign keys or a polymorphic parent this schema
-- avoids everywhere.
--
-- Two real foreign keys beat one nullable pair while there are two of them. If
-- a third domain arrives with the same need, the generalisation earns its cost
-- and this comment is where to start.
--
-- ## Blameless, as a constraint
--
-- Same as `ops_incidents` and for the same reason: there is no column for who
-- caused what. A retrospective that names a person is one nobody speaks
-- honestly in the second time, and the honest second one is the entire point.

CREATE TABLE leadership_retrospectives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The slice this was filed under, when it was filed as a Skilluv
    -- artefact. NULL for a retrospective recorded to carry its actions
    -- without being submitted for review — somebody tracking their own team's
    -- follow-through, which is a use worth allowing.
    slice_id UUID REFERENCES project_slices(id) ON DELETE CASCADE,
    facilitator_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),

    -- Which shape was run. Named rather than left free, because the shape
    -- decides what the notes can contain: a sailboat produces risks and a
    -- 4Ls does not, and a synthesis across two formats compares different
    -- things.
    format VARCHAR(30) NOT NULL CHECK (format IN (
        'start_stop_continue',
        'four_ls',        -- liked, learned, lacked, longed for
        'sailboat',       -- wind, anchors, rocks, island
        'mad_sad_glad',
        'timeline',       -- what happened, in order, before opinions
        'other'
    )),
    format_note TEXT,

    -- How many people were in the room. A retrospective of two is a
    -- conversation, and the number is what lets a reader calibrate the
    -- findings rather than being told they are representative.
    participants_count SMALLINT NOT NULL
        CHECK (participants_count BETWEEN 2 AND 200),

    held_on DATE NOT NULL,

    -- What was said, in the room's own words, with nobody named. Long enough
    -- to be a record: a heading and three bullet points is a meeting that
    -- happened, not a retrospective that was facilitated.
    insights_md TEXT NOT NULL CHECK (length(btrim(insights_md)) >= 200),

    -- Whether the notes went back to the people who were in the room. A
    -- retrospective whose output the participants never saw is one they will
    -- not speak in next time.
    shared_with_participants_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- `other` is allowed and has to say what it was.
    CONSTRAINT an_other_format_says_which CHECK (
        format <> 'other' OR (format_note IS NOT NULL AND btrim(format_note) <> '')
    )
);

COMMENT ON TABLE leadership_retrospectives IS
    'Blameless is a constraint here, not a value statement: there is no column '
    'for who caused what. A retrospective that names a person is one nobody '
    'speaks honestly in the second time.';

COMMENT ON COLUMN leadership_retrospectives.participants_count IS
    'What lets a reader calibrate the findings. A retrospective of two is a '
    'conversation, and saying so is more useful than asserting the findings '
    'are representative.';

CREATE INDEX idx_retrospectives_facilitator
    ON leadership_retrospectives (facilitator_user_id, held_on DESC);

CREATE INDEX idx_retrospectives_slice
    ON leadership_retrospectives (slice_id)
    WHERE slice_id IS NOT NULL;

CREATE TRIGGER trg_leadership_retrospectives_updated_at
    BEFORE UPDATE ON leadership_retrospectives
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- What the retrospective said would be done
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE leadership_retrospective_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    retrospective_id UUID NOT NULL
        REFERENCES leadership_retrospectives(id) ON DELETE CASCADE,

    description TEXT NOT NULL CHECK (btrim(description) <> ''),
    -- Who is doing it. Nullable because an action can be owned by somebody
    -- who has no account here, and forcing a user id would mean either
    -- inventing accounts or losing the action.
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    -- The name of the owner when they are not on this platform. One of the
    -- two is required — see the constraint below — because an action with no
    -- owner is an intention.
    owner_label VARCHAR(120),
    due_on DATE,

    done_at TIMESTAMPTZ,
    -- Why it was dropped, when it was. An action item that quietly disappears
    -- is how the same retrospective happens twice.
    abandoned_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT an_action_has_an_owner CHECK (
        owner_user_id IS NOT NULL
        OR (owner_label IS NOT NULL AND btrim(owner_label) <> '')
    ),
    CONSTRAINT an_action_is_done_or_dropped_not_both CHECK (
        abandoned_reason IS NULL OR done_at IS NULL
    ),
    CONSTRAINT abandoning_says_why CHECK (
        abandoned_reason IS NULL OR btrim(abandoned_reason) <> ''
    )
);

COMMENT ON TABLE leadership_retrospective_actions IS
    'The half everybody skips. An action item that quietly disappears is how '
    'the same retrospective happens twice — which is why dropping one has to '
    'say why rather than being a deletion.';

COMMENT ON COLUMN leadership_retrospective_actions.owner_label IS
    'The owner''s name when they have no account here. One of the two owner '
    'columns is required: an action with nobody on it is an intention.';

CREATE INDEX idx_retro_actions_open
    ON leadership_retrospective_actions (due_on)
    WHERE done_at IS NULL AND abandoned_reason IS NULL;

CREATE INDEX idx_retro_actions_by_retro
    ON leadership_retrospective_actions (retrospective_id);

-- ═══════════════════════════════════════════════════════════════════
-- Whether the retrospective actually landed
-- ═══════════════════════════════════════════════════════════════════
--
-- A view rather than a stored counter, for the reason the craft score is not
-- stored either: a counter is wrong from the moment an action is closed, and
-- a trigger keeping it right is a second place the rule can be different.
--
-- The threshold — seventy per cent within ninety days — is the backlog's, and
-- it is right. It is applied here rather than in Rust so that the attestation
-- generator and any dashboard read the same number.
--
-- An abandoned action counts as resolved rather than as a failure. Deciding
-- not to do something, in writing, with a reason, is a decision — and a rule
-- that punished it would teach people to leave actions open forever instead.

CREATE VIEW leadership_retrospective_followthrough AS
SELECT r.id AS retrospective_id,
       r.facilitator_user_id,
       r.slice_id,
       r.held_on,
       count(a.id) AS actions_total,
       count(a.id) FILTER (
           WHERE a.done_at IS NOT NULL OR a.abandoned_reason IS NOT NULL
       ) AS actions_resolved,
       count(a.id) FILTER (
           WHERE (a.done_at IS NOT NULL AND a.done_at <= r.held_on + INTERVAL '90 days')
              OR (a.abandoned_reason IS NOT NULL)
       ) AS actions_resolved_in_window,
       -- The claim `leadership_retrospective_facilitated` rests on. Requires
       -- at least one action, because a retrospective that decided nothing
       -- cannot have followed through on it.
       (count(a.id) > 0
        AND count(a.id) FILTER (
                WHERE (a.done_at IS NOT NULL
                       AND a.done_at <= r.held_on + INTERVAL '90 days')
                   OR a.abandoned_reason IS NOT NULL
            )::NUMERIC / count(a.id) >= 0.70
       ) AS followed_through
  FROM leadership_retrospectives r
  LEFT JOIN leadership_retrospective_actions a ON a.retrospective_id = r.id
 GROUP BY r.id, r.facilitator_user_id, r.slice_id, r.held_on;

COMMENT ON VIEW leadership_retrospective_followthrough IS
    'Whether a retrospective landed: at least one action, and seventy per '
    'cent of them resolved within ninety days. An abandoned action counts as '
    'resolved — deciding not to do something, in writing, with a reason, is a '
    'decision, and punishing it would teach people to leave actions open.';

-- ═══════════════════════════════════════════════════════════════════
-- Revision rounds for leadership work
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0412 built the mechanism for every domain, so this is rows. The
-- kinds are the backlog's list, and the limit is its number.

INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
    ('leadership_alternatives_thin', 'leadership', 'Alternatives not explored',
     'The proposal names one option and calls it a decision. The most common '
     'round on an RFC, and the one that changes the outcome most often.', 310),
    ('leadership_rationale_missing', 'leadership', 'Rationale missing',
     'A choice was made and the reason is not written down, so nobody can '
     'disagree with it later or revisit it when the facts change.', 320),
    ('leadership_prioritisation_disputed', 'leadership', 'Prioritisation disputed',
     'The order is contested. Settled by naming what is given up, not by '
     'reordering until everybody stops objecting.', 330),
    ('leadership_actions_vague', 'leadership', 'Actions not concrete',
     'The retrospective produced sentiments rather than items with an owner '
     'and a date.', 340),
    ('leadership_redaction_incomplete', 'leadership', 'Redaction incomplete',
     'The document still identifies an organisation, a team or a person. The '
     'only round here that blocks publication outright.', 350),
    ('leadership_measurement_missing', 'leadership', 'No way of telling',
     'Nothing in the document will move if it works — or worse, nothing will '
     'move if it does not.', 360)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO revision_round_limits (skill_domain, max_rounds, rationale) VALUES
    ('leadership', 4,
     'Four. One pass on the alternatives, one on the rationale, one on the '
     'measurement and one on form is the journey of a written decision. A '
     'fifth round means the reviewer and the author disagree about what the '
     'organisation should do, which is not something a review settles.')
ON CONFLICT (skill_domain) DO NOTHING;
