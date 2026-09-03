-- A contest entry records whether anybody checked it belongs to the entrant.
--
-- ## The hole
--
-- `tournament_submissions.artifact_url` is free text, validated only as an
-- https URL under 2000 characters. So an entrant could hand in somebody
-- else's repository — a well-known project, a rival's entry, anything — and
-- nothing looked. On a contest with a prize pool that is not a rough edge, it
-- is the whole integrity of the ranking.
--
-- The same hole was closed on challenge submissions, where an attachment is a
-- reference to a row the platform holds and is checked against its owner. A
-- contest entry cannot work that way: what it points at usually lives on
-- GitHub, and the platform does not host it.
--
-- ## What is checkable, and what is not
--
-- A `github.com` URL names its owner in the path, and the platform already
-- knows every account's GitHub login from `github_connections`. So any
-- github.com URL in an entry can be required to be the entrant's own, and
-- that covers the artifact types a code contest actually uses — repository,
-- pull request, gist.
--
-- A link to a deployed demo, a hosted design file or a video cannot be
-- verified this way, and pretending otherwise would be worse than saying so.
-- Those are accepted and recorded as unchecked, which is what a juror needs
-- to know: this one, somebody vouched for; that one, nobody did.
--
-- ## Why a column rather than a refusal
--
-- Refusing everything unverifiable would forbid whole domains from entering —
-- an audio entry, a design entry, a deployed site — for a property only code
-- artifacts can have. The column lets the ranking carry the distinction
-- instead of the rules pretending it does not exist.

ALTER TABLE tournament_submissions
    ADD COLUMN artifact_verified BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN tournament_submissions.artifact_verified IS
    'TRUE when the platform checked the artifact belongs to the entrant. Only '
    'a github.com URL can be checked — its owner segment against the '
    'entrant''s connected GitHub login — so this is FALSE for a deployed '
    'demo, a hosted design file or a video, which nobody can attribute from a '
    'URL alone. FALSE means unchecked, never rejected: a juror reads it as '
    '"take this one on trust".';

-- Reading a field by what was and was not checked is a juror's first
-- question, so it is indexed rather than scanned.
CREATE INDEX idx_tournament_submissions_verified
    ON tournament_submissions (tournament_id, artifact_verified);
