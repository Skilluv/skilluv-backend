-- A submission may be waiting on a person.
--
-- ## What it replaces
--
-- `evaluate_basic` held this, and it ran for every domain but `code`:
--
--     if challenge.skill_domain != "code" {
--         if code.len() >= 100 { return success + reward_fragments }
--     }
--
-- A hundred characters of anything passed a design, audio, leadership,
-- communication, education, quality or security challenge and credited its
-- fragments. On a public profile that pass is indistinguishable from one a
-- reviewer gave. The platform sells exactly one thing — that a proof here is
-- worth showing a recruiter — and this was a hole straight through it
-- (SKI-361).
--
-- The `code` domain had a smaller version of the same hole: when a challenge
-- declares no `expected_output`, the final fallback returned `success`
-- unconditionally. Same fix, same reason.
--
-- ## Why a status and not just a failure
--
-- Refusing the submission would be wrong in the other direction: the work may
-- be good, and nothing has read it. `pending_review` says what is true —
-- received, not yet judged — and it is what lets the submission land in the
-- human review queue that already exists (`review_tasks` +
-- `deliverables.verification_status = 'pending'` + `reviews`), rather than in
-- a queue that has to be invented per domain.
--
-- Fragments follow the verdict, not the submission: a `pending_review` row
-- carries `fragments_earned = 0` until a reviewer approves the deliverable,
-- and `ReviewsService::apply_verified_side_effects` awards them then.
--
-- ## Existing rows
--
-- None is rewritten. A submission already marked `success` by the character
-- count is a past decision of the platform, and quietly demoting people's
-- profiles is not this migration's call to make; what this changes is that no
-- new one can be created that way.

ALTER TABLE challenge_submissions
    DROP CONSTRAINT challenge_submissions_status_check;

ALTER TABLE challenge_submissions
    ADD CONSTRAINT challenge_submissions_status_check
    CHECK (status IN ('in_progress', 'submitted', 'pending_review', 'success', 'failure'));

COMMENT ON COLUMN challenge_submissions.status IS
    'in_progress = started, not submitted. submitted = legacy, unused by the '
    'current handler. pending_review = submitted and waiting on a human '
    'verdict, which is the terminal state of every submission no evaluator can '
    'score; its deliverable sits in the review queue and fragments are awarded '
    'on approval, not here. success / failure = judged, by Judge0 for code or '
    'by a reviewer.';
