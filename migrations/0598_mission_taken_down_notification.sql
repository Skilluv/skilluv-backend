-- Telling the two sides that the platform removed their mission.
--
-- ## Why this kind exists at all
--
-- A mission is published by the enterprise that wrote it, with no review in
-- between: the control is the KYC an enterprise passes before it can post
-- anything, which checks *who* may publish once rather than *what* is
-- published every time. The consequence is that the platform's only move
-- against a mission that should not be up is to take it down afterwards, and a
-- takedown nobody is told about is an advert that silently stops existing.
--
-- ## Why it is transactional
--
-- Money may be sitting in escrow against it, and the takedown returns that
-- escrow. That is not something a person may have turned off in their
-- settings.
--
-- ## Why both sides hear it
--
-- The enterprise because it is their mission, the assignee because they may
-- have been working on it this morning. Same kind, same wording: the reason
-- given is the platform's and it is the same reason for both — a takedown
-- explained one way to the client and another way to the contractor is two
-- decisions.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional)
VALUES
    ('mission.taken_down', 'enterprise',
     TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE)
ON CONFLICT (kind) DO NOTHING;

-- The mission page rather than the board: it still exists, it is cancelled,
-- and its own page is where the reason and the refund are.
UPDATE notification_kinds
   SET cta_path = '/missions/{mission_slug}'
 WHERE kind = 'mission.taken_down';
