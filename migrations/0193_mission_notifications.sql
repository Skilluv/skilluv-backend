-- What the mission board has to tell people.
--
-- Four kinds rather than one carrying the outcome as an argument: an outcome
-- passed as a word is a word in one language, and it would land untranslated
-- in the middle of a translated sentence.
--
-- The rejection is the one that matters. A board where only the winner hears
-- anything leaves everybody else refreshing a page, and the reason is already
-- required on the row — this is what carries it to them.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- To the enterprise: somebody applied. Not transactional — there is no
    -- clock on it, and an enterprise publishing widely would drown.
    ('mission.application_received',    'enterprise', TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, FALSE),
    -- To the applicant. All three channels: this is the answer they have
    -- been waiting for, and email is the one they will still find in a week.
    ('mission.application_selected',    'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE,  TRUE,  TRUE),
    ('mission.application_rejected',    'enterprise', TRUE, TRUE, TRUE, TRUE, FALSE, TRUE,  TRUE),
    ('mission.application_shortlisted', 'enterprise', TRUE, TRUE, TRUE, TRUE, TRUE,  FALSE, FALSE);

UPDATE notification_kinds
   SET cta_path = CASE kind
       WHEN 'mission.application_received' THEN '/missions/{mission_slug}/applications'
       ELSE '/me/missions'
   END
 WHERE kind LIKE 'mission.%';
