-- Being put forward by the platform, once a week, on the record.
--
-- ## Why a table and not a boolean on the account
--
-- A flag would answer "is this person featured now" and lose every week
-- before. The whole value of being featured is that it happened and stays
-- true — it is what the `featured_designer` attestation rests on, and an
-- attestation whose evidence was overwritten by the next week's choice would
-- be a claim nobody can check.
--
-- ## Why it is not design-specific
--
-- `featured_coder`, `featured_ai_researcher` and `featured_designer` are
-- already three attestation bases. A `featured_designers` table would have
-- needed two siblings, with three copies of the same "one per week" rule and
-- three admin screens. The domain is a column.
--
-- ## One per domain per week
--
-- The primary key. Two people featured in the same week in the same domain
-- means neither was featured — the scarcity is the point, and it is the only
-- thing that makes the attestation worth anything.
--
-- Weeks are stored as the Monday of the week, in UTC, computed by the caller.
-- A DATE rather than a week number: ISO week numbering disagrees with itself
-- across new year boundaries, and a date is unambiguous in every locale.
--
-- ## Why nothing is posted automatically
--
-- The design backlog asked for automatic publication to social networks. That
-- needs credentials for accounts that do not exist yet, and — more to the
-- point — it would publish somebody's name and work to a third-party platform
-- on a schedule, with no human between the decision and the post. What this
-- stores instead is everything a post needs; who presses send is a person.

CREATE TABLE featured_talents (
    skill_domain  VARCHAR(30) NOT NULL,
    -- The Monday of the week being awarded, UTC.
    week_of       DATE NOT NULL,
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Why this person, this week. Written by whoever chose them, published
    -- as-is. A featuring with no stated reason is a popularity contest, and
    -- the sentence is what stops it becoming one.
    reason_md     TEXT NOT NULL CHECK (length(reason_md) BETWEEN 40 AND 4000),
    -- The work being pointed at. Optional: somebody can be put forward for a
    -- body of work rather than one piece.
    deliverable_id UUID REFERENCES deliverables(id) ON DELETE SET NULL,

    chosen_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (skill_domain, week_of)
);

COMMENT ON TABLE featured_talents IS
    'Who the platform put forward, by domain and by week. Kept rather than '
    'overwritten: the `featured_*` attestations rest on these rows, and an '
    'attestation whose evidence was replaced would be uncheckable.';

COMMENT ON COLUMN featured_talents.reason_md IS
    'Why this person this week, published as written. A featuring with no '
    'stated reason is a popularity contest.';

-- A profile shows "featured 3 times"; this is that count.
CREATE INDEX idx_featured_talents_user ON featured_talents (user_id, week_of DESC);

-- Nobody is featured twice in the same domain in the same quarter.
--
-- Not a constraint, because a quarter is a moving window and a CHECK cannot
-- see other rows without a trigger that would have to be maintained. It is a
-- rule the service enforces and this index makes cheap to check.
CREATE INDEX idx_featured_talents_recent ON featured_talents (skill_domain, week_of DESC);
