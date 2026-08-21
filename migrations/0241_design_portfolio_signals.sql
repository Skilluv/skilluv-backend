-- A designer's portfolio, as a link and not as an import.
--
-- ## What the backlog asked for
--
-- "Import portfolio Behance OAuth + fetch projects" and the same for
-- Dribbble, so that an arriving designer's existing work appears on their
-- Skilluv profile.
--
-- ## Why it is three rows here instead
--
-- Two reasons, and the second is the one that decides it.
--
-- The practical one: Behance's public API was withdrawn in 2020, and
-- Dribbble's requires a partnership approval that a pre-launch platform will
-- not get. An "import" built on either would be a scraper, which means the
-- backend issuing HTTP requests to arbitrary user-supplied URLs — the SSRF
-- vector migration 0145 already refused for blog references, aimed straight
-- at the internal network.
--
-- The deciding one: an imported portfolio must not count for anything, and
-- 0145 exists to keep that true. Fetching a hundred Behance projects would
-- put a hundred artefacts on a profile with no verified deliverable behind
-- any of them — a list somebody typed, sitting next to work that went through
-- a critique. The distinction between the two is the only thing this platform
-- sells.
--
-- So: a designer links their portfolio, a moderator confirms it is theirs,
-- and it shows as what it is. The craft score does not read it, the rank does
-- not read it, and recruiter search does not score it. That is the same deal
-- a developer's GitHub gets, and it is honest for both.
--
-- ## Why `behance_dribbble` is not one provider
--
-- They are different accounts a reader may want to open separately, and a
-- moderator confirming "this Dribbble is yours" has checked a different thing
-- from "this Behance is yours".

ALTER TABLE external_signals DROP CONSTRAINT IF EXISTS external_signals_provider_check;

ALTER TABLE external_signals
    ADD CONSTRAINT external_signals_provider_check
    CHECK (provider IN (
        -- Migration 0145
        'github',
        'medium',
        'dev_to',
        'conf_ref',
        -- Design portfolios. Declared, reviewed, never imported.
        'behance',
        'dribbble',
        'artstation',
        -- Where motion and film work actually lives.
        'vimeo',
        -- Where a type designer's families are published.
        'foundry'
    ));

COMMENT ON COLUMN external_signals.provider IS
    'Where the signal lives. Only `github` self-verifies, through an OAuth '
    'flow the person already completed. Everything else is confirmed by a '
    'moderator, because the alternative is the backend fetching arbitrary '
    'user-supplied URLs.';

-- ═══════════════════════════════════════════════════════════════════
-- The rule, said in the schema rather than only in a comment
-- ═══════════════════════════════════════════════════════════════════
--
-- 0145 states that external signals never feed a proof table, and relies on
-- an integration test to keep it true. That test covers the tables that
-- existed then. `craft_scores` arrived in 0204 and is exactly the kind of
-- thing somebody would wire this into next — a number that sorts a recruiter
-- listing is the most tempting place to put a follower count.
--
-- The comment below is where the next person looks.

COMMENT ON TABLE external_signals IS
    'Reputation earned elsewhere, shown and never counted. Nothing here may '
    'feed user_skills, user_ranks, badge rules, craft_scores or talent '
    'search scoring: importing a portfolio must not make somebody a Doyen, '
    'and "proven on Skilluv" has to stay literal.';
