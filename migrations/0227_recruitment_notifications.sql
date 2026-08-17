-- What a recruitment campaign has to tell people.
--
-- One kind, and it is the important one: the request for consent. Somebody
-- is being put forward to a client, and this is what asks them. Transactional
-- and on every channel — a consent request that arrives only in an app
-- somebody opens once a week is a consent request that goes unanswered, and
-- an unanswered one blocks the whole campaign by design.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES
    ('recruitment.shortlisted', 'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE);

UPDATE notification_kinds
   SET cta_path = '/me/recruitment-invitations'
 WHERE kind = 'recruitment.shortlisted';
