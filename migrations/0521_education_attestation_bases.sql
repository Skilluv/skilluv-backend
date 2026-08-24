-- What an education attestation can rest on.
--
-- Five rows in the table 0406 created, and one of the backlog's five turned
-- into something the platform can actually see.
--
-- ## `education_students_taught` counted the wrong thing
--
-- Ticket F-07 asked for a basis issued on "X students taught in a period,
-- measurable". Nothing here can measure that. A trainer says they taught two
-- hundred people; the platform has no register, no attendance and no way to
-- ask. An attestation is the one artefact a stranger relies on without
-- checking, and resting one on a self-reported headcount would make it worth
-- exactly what the headcount is worth.
--
-- What *is* visible is a cohort that ran on this platform, with members and
-- an outcome record — migration 0524 builds that — so the basis becomes
-- `education_cohort_delivered`, which the backlog also asked for, and the
-- headcount lives on the cohort where it can be counted rather than claimed.
--
-- Teaching done elsewhere is not erased: it is a portfolio entry and a
-- craft-score term, marked as declared, exactly as an audio play count is.
-- The difference between the two records is the whole point of having both.
--
-- ## Why the curriculum basis waits for adoption
--
-- `education_curriculum_authored` rests on a curriculum somebody else picked
-- up. Publishing a learning path is easy; having another trainer run it is
-- the fact worth attesting, and it is one this database can see because the
-- adoption is a row.
--
-- ## The fifth is editorial
--
-- `featured_educator`, like `featured_coder`, `featured_audio_creator` and
-- `featured_communicator`. It names a person rather than an artefact, so it
-- carries no deliverable.

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order) VALUES

('education_cohort_delivered', 'education',
 'Cohort delivered',
 'A cohort run to the end, with its completion rate and its measured outcomes on the record.',
 TRUE, 510),

('education_workshop_delivered', 'education',
 'Workshop delivered',
 'A live session delivered, with its materials, its exercises and what participants left able to do.',
 TRUE, 520),

('education_curriculum_authored', 'education',
 'Curriculum authored and adopted',
 'A curriculum published and run by at least one trainer other than its author.',
 TRUE, 530),

('education_assessment_framework_published', 'education',
 'Assessment framework published',
 'Rubrics and criteria published in a form another assessor can apply and reach the same grade with.',
 TRUE, 540),

('featured_educator', 'education',
 'Featured',
 'Education work picked out by the editors as exemplary.',
 FALSE, 550);
