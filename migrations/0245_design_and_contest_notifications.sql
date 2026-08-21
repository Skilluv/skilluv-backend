-- What the design loop and the contest format have to tell people.
--
-- ## Not three booleans on `email_preferences`
--
-- The design backlog asked for `design_contest_notifications`,
-- `design_iteration_notifications` and `design_featured_notifications` as
-- columns. That system was replaced by migrations 0155-0164: preferences are
-- per kind and per channel, with the ceiling and the default recorded on the
-- kind itself. Three design-shaped booleans would have given design its own
-- settings screen, out of step with every other category, and a fourth
-- domain would have added three more.
--
-- So design gets what mentorship, guilds and missions have: kinds in the
-- catalogue, and the same toggles.
--
-- ## Two of the fifteen requested are deliberately absent
--
-- `design_challenge_claimed` would tell somebody that they have just clicked
-- claim. A notification of your own action is the fastest way to teach people
-- to ignore the bell.
--
-- `portfolio_import_completed` announces the end of an import that does not
-- happen. Migration 0145 keeps external signals display-only and 0241 seeds
-- the design providers as declarations, never imports — a notification for it
-- would be the only place in the platform claiming the opposite.
--
-- ## And one is reused rather than duplicated
--
-- `contest_winner` is `tournament.podium`, which already exists, already
-- buzzes, and already carries the place. A second kind for the same moment
-- would mean two notifications for one podium.
--
-- ## Categories
--
-- `learning` for the critique loop and contests: that is where somebody's
-- own progress already lives, and a designer's settings screen should not
-- have a section nobody else has.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- ── The critique loop ──────────────────────────────────────────
    -- Somebody is waiting at each end of this, which is why both directions
    -- buzz. A critique asking for another round is the single most
    -- time-sensitive thing in the loop: the designer cannot act until they
    -- have read it, and the reviewer cannot finish until they do.
    ('design.iteration_requested', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    -- To the reviewers of the trade: a version is in the queue. No email by
    -- default — a reviewer opens the queue on purpose, and one mail per
    -- version handed in is how a volunteer stops reviewing.
    ('design.version_submitted',   'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE, FALSE),
    -- The moment the whole programme exists to produce. Rare, and worth
    -- keeping in a mailbox.
    ('design.validated',           'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    -- A refusal lands, and is read when the person is ready for it. Same
    -- rule as `community.challenge_rejected`: no buzz.
    ('design.rejected',            'learning', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),

    -- ── Contests ───────────────────────────────────────────────────
    -- A new brief is an invitation, not an emergency: it goes to people
    -- whose declared trades match, and it does not interrupt their day.
    ('contest.published',          'learning', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    -- A deadline is the one contest event that is worth a buzz and a mail:
    -- missing it costs the week of work already done.
    ('contest.deadline_soon',      'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('contest.closed',             'learning', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    -- An invitation expires, so it travels further than the app — the same
    -- reasoning as `guild.invitation`.
    ('contest.jury_invited',       'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    ('contest.jury_deadline_soon', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    -- The result, to everybody who took part and to whoever was watching.
    -- The winners also get `tournament.podium`, which is the one that buzzes.
    ('contest.concluded',          'learning', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE),
    -- Taking part is recorded rather than celebrated: it is a line on a
    -- profile, not an achievement, and migration 0508 says so.
    ('contest.participation_recorded', 'learning', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),

    -- ── Recognition ────────────────────────────────────────────────
    -- Being put forward by the platform is rare and public. It buzzes.
    --
    -- Not `design.featured`: one person per domain per week is put forward,
    -- and three copies of this kind would have to be kept in step with each
    -- other for the rest of the platform's life.
    ('talent.featured',            'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),

    -- ── The mission board ──────────────────────────────────────────
    -- Not design-specific, deliberately. A cyber mission matching somebody's
    -- trade is the same event as a design one, and a `design_mission_*` kind
    -- would have to be copied for every domain that ever ships.
    ('mission.matching_published', 'enterprise', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE);

-- Where each one takes the reader. `{slice_id}` and `{tournament_slug}` are
-- filled from the notification's payload.
UPDATE notification_kinds
   SET cta_path = CASE kind
       WHEN 'design.iteration_requested'     THEN '/design/slices/{slice_id}'
       WHEN 'design.version_submitted'       THEN '/design/reviews/queue'
       WHEN 'design.validated'               THEN '/design/slices/{slice_id}'
       WHEN 'design.rejected'                THEN '/design/slices/{slice_id}'
       WHEN 'talent.featured'                THEN '/me'
       WHEN 'contest.jury_invited'           THEN '/tournaments/{tournament_slug}/jury'
       WHEN 'contest.jury_deadline_soon'     THEN '/tournaments/{tournament_slug}/jury'
       WHEN 'mission.matching_published'     THEN '/missions'
       ELSE '/tournaments/{tournament_slug}'
   END
 WHERE kind IN (
     'design.iteration_requested', 'design.version_submitted', 'design.validated',
     'design.rejected', 'talent.featured',
     'contest.published', 'contest.deadline_soon', 'contest.closed',
     'contest.jury_invited', 'contest.jury_deadline_soon', 'contest.concluded',
     'contest.participation_recorded',
     'mission.matching_published'
 );
