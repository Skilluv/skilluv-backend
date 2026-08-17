-- What a trial has to tell the person on it.
--
-- One kind, at the start. The rest of a trial is a conversation between two
-- people who are now working together; the platform's job is to say clearly
-- that it has begun and on what terms, and then get out of the way.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES
    ('trial.started', 'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE);

UPDATE notification_kinds
   SET cta_path = '/me/trials'
 WHERE kind = 'trial.started';
