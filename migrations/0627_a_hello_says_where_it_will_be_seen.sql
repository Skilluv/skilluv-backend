-- A HELLO says where it will be seen, before anybody uploads a file.
--
-- The design rite asks for an artefact and receives it as a
-- `design_upload_sessions` row. Those are private by construction:
-- `design_uploads::load_for_reader` lets the owner read one, and a reviewer
-- only while the review task is open or claimed, so an upload stops being
-- readable once the verdict is in. The comment there says so, and it is a
-- deliberate stance rather than an oversight.
--
-- The wall of HELLOs contradicts that stance for one kind of artefact. That
-- is defensible: a self introduction is made to be seen, and the argument for
-- this rite is that it becomes a first portfolio piece rather than an
-- exercise thrown away. It is not defensible in silence. Somebody who uploads
-- a file to a platform that has told them uploads are private has not agreed
-- to publish it.
--
-- So the brief says it before the upload, and it says the useful half too:
-- what not to put in.
--
-- Appended rather than retyped. Steps 1 to 3 are migration 0623 word for
-- word, and rewriting them here to add a fourth would be two copies of the
-- same sentence drifting apart at the first edit.
--
-- One disagreement this does not fix and does not hide:
-- `deliverables.public` is already TRUE on every challenge submission,
-- written by `services::deliverables`, while the artefact behind it stops
-- being readable after the verdict. The flag claims something the storage
-- refuses. The wall reads the flag AND the rite AND the verdict together
-- rather than trusting the flag alone, which is why that default can be left
-- as it is instead of changed under eleven other domains.

UPDATE challenge_templates SET
    instructions = instructions || E'\n4. It is shown publicly on the wall of HELLOs once a reviewer has read it, under your name. That is the point of it: it is the first thing anybody looking you up will see. Put nothing in it you would not hand to a stranger, and nothing that belongs to a client.',
    instructions_i18n = jsonb_set(
        jsonb_set(
            instructions_i18n,
            '{en}',
            to_jsonb((instructions_i18n ->> 'en') || E'\n4. It is shown publicly on the wall of HELLOs once a reviewer has read it, under your name. That is the point of it: it is the first thing anybody looking you up will see. Put nothing in it you would not hand to a stranger, and nothing that belongs to a client.')
        ),
        '{fr}',
        to_jsonb((instructions_i18n ->> 'fr') || E'\n4. Il est montré publiquement sur le mur des HELLO une fois qu''un relecteur l''a lu, sous ton nom. C''est justement l''intérêt : c''est la première chose que verra quelqu''un qui te cherche. N''y mets rien que tu ne tendrais pas à un inconnu, et rien qui appartienne à un client.')
    ),
    updated_at = NOW()
WHERE is_domain_rite = TRUE
  AND skill_domain = 'design'
  AND status = 'published'
  AND instructions NOT LIKE '%wall of HELLOs%';

-- `NOT LIKE` on the guard column, so running this twice does not append the
-- paragraph twice.

DO $guard$
DECLARE
    warned BIGINT;
BEGIN
    SELECT count(*) INTO warned
      FROM challenge_templates
     WHERE is_domain_rite AND skill_domain = 'design' AND status = 'published'
       AND instructions_i18n ->> 'fr' LIKE '%mur des HELLO%'
       AND instructions_i18n ->> 'en' LIKE '%wall of HELLOs%'
       AND instructions LIKE '%wall of HELLOs%';
    IF warned <> 1 THEN
        RAISE EXCEPTION
            'the design rite does not say where the artefact will be shown (% row(s))',
            warned;
    END IF;
END $guard$;
