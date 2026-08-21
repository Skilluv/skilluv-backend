-- The marathon badge.
--
-- Thirty days, a number of merged pull requests somebody committed to in
-- advance, and the badge for having reached it.
--
-- ## Why it is `manual` rather than counted
--
-- The engine counts what somebody has done in total. A marathon asks what
-- they did between two dates, against a target that differs per edition —
-- five in one, twenty in another. There is no global count that answers it,
-- and pretending there is would grant the badge to somebody who merged the
-- same number of pull requests over three years.
--
-- So it is granted when the marathon concludes, by whoever concludes it, with
-- the edition and the count written into the reason. That reason is what a
-- reader gets when they ask what the badge means.

INSERT INTO badge_rules
    (slug, output_type, display_name, description, conditions)
VALUES
    ('code-oss-marathon-hero',
     'medal',
     'Marathonien open source',
     'A tenu l''engagement d''un marathon de contributions : le nombre de pull '
     'requests fusionnées annoncé au départ, atteint dans la fenêtre.',
     '{"manual": true, "skill_domain": "code"}'::JSONB);
