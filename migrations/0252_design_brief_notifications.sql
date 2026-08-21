-- Telling somebody what became of the brief they wrote.
--
-- Two kinds, and both matter more than they look. Writing a brief is unpaid,
-- invisible work that produces something for other people; the one thing it
-- costs nothing to give back is an answer.
--
-- A refusal without a reason is a refusal that comes back next week as the
-- same brief, so `design_brief_rejection_says_why` makes the reason a
-- constraint and this carries it.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- Somebody's brief became work other people can claim. Rare, and the
    -- whole return on having written it.
    ('design.brief_published', 'learning', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  FALSE),
    -- A refusal lands and is read when the person is ready for it. No buzz,
    -- for the same reason `community.challenge_rejected` has none.
    ('design.brief_rejected',  'learning', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  FALSE);

UPDATE notification_kinds
   SET cta_path = CASE kind
       WHEN 'design.brief_published' THEN '/design/slices/{slice_id}'
       ELSE '/design/briefs/mine'
   END
 WHERE kind IN ('design.brief_published', 'design.brief_rejected');
