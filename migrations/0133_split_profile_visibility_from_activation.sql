-- SKI-70 — `users.profile_active` carried two unrelated meanings:
--   1. "the user cleared onboarding" — DEFAULT FALSE, flipped TRUE on the first
--      successful challenge submission.
--   2. "the profile is visible" — read by talent search, leaderboards, digest,
--      portfolio and the public profile page.
--
-- A user who finished signup but hasn't completed a challenge was therefore
-- indistinguishable from one who deliberately hid their profile, and
-- GET /api/profile/{username} answered 404 for both.
--
-- `profile_hidden` now carries meaning 2 on its own for the public profile
-- page. `profile_active` keeps meaning 1 and keeps gating the listing surfaces
-- (talent search, leaderboard, digest), so no recruiter-facing surface changes.

ALTER TABLE users ADD COLUMN profile_hidden BOOLEAN NOT NULL DEFAULT FALSE;

-- Users who had explicitly hidden themselves are the ones that were active and
-- then deactivated. We cannot tell those apart from never-activated accounts
-- retroactively, so nobody is backfilled as hidden: opting out again is one
-- click, whereas wrongly hiding an existing public profile is a regression.

CREATE INDEX idx_users_profile_hidden ON users (profile_hidden) WHERE profile_hidden = TRUE;
