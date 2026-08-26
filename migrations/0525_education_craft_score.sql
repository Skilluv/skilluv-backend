-- The education craft score.
--
-- Rows in `craft_score_weights` (migration 0204), read on every computation,
-- so the formula is editable by an operator and publishable next to the
-- number it produces.
--
-- ## The tiers are the shared six, not the five the ticket named
--
-- Ticket A-02 asked for `apprentice / trainer / senior-trainer /
-- master-educator / luminary`. Migration 0204 already seeded six tiers for
-- this domain, the same six every other domain uses, and 0508 wrote out why
-- that is deliberate: a tier is a position on a scale, each scale is
-- calibrated by its own weights, and one shared ladder is what lets somebody
-- compare their own two profiles.
--
-- ## What the ticket's formula became
--
-- Seven of its ten terms are kept as written. Three changed:
--
--   * **`students_taught_hours * 0.5`** is gone, and 0521 and 0522 already
--     wrote out why at length: the platform has no register, no attendance
--     and no way to ask. The term would have measured what somebody typed,
--     and at half a point per hour a typed four thousand is two thousand
--     points — twice what every other term can produce together.
--
--     It is replaced by `learners_reached`, logarithmic, counting the members
--     of cohorts this person actually led at full weight and declared
--     enrolments from an outside platform at half. That is the treatment 0415
--     established for audience figures, and the same argument applies: most
--     teaching happens off this platform, excluding it would erase a real
--     career, and counting a typed number at face value would make the score
--     a self-assessment.
--
--   * **`curriculums_authored * 60`** is folded into the adoption term. The
--     basis of 0521 is only issued once somebody else has run it, so a
--     separate term for authoring would have counted the same fact twice.
--
--   * **`curriculums_adopted_by_others * 30`** is raised to 50 and counts
--     adoptions rather than curriculums. Five trainers running one programme
--     and one trainer running five are different achievements, and the
--     adoption is the countable one.
--
-- Two terms are added:
--
--   * `assessment_frameworks_published` — the artefact of the curriculum
--     trade that is not a curriculum, and the one nothing else in the formula
--     would have seen.
--   * `orientations_distinct` — on the audio model. A trainer who also
--     designs the programme is the normal shape of a career here.
--
-- ## Why `learners_reached` is logarithmic
--
-- Linear, one bootcamp instructor with two thousand alumni would outweigh
-- every other term combined, and the score would measure volume rather than
-- craft. On a log scale a hundred learners is worth about two weights and ten
-- thousand about four: more is still more, and no single figure reaches the
-- ceiling alone.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order) VALUES

('education', 'attestations_education', 5.00, 'count', NULL,
 'Every unrevoked education attestation.', 10),

('education', 'cohorts_delivered', 80.00, 'count', NULL,
 'Cohorts run to the end, with their completion and outcomes on the record.', 20),

('education', 'workshops_delivered', 30.00, 'count', NULL,
 'Live sessions delivered, with their materials published.', 30),

('education', 'curriculum_adoptions', 50.00, 'count', NULL,
 'Times a trainer other than the author has run one of their curriculums.', 40),

('education', 'assessment_frameworks_published', 40.00, 'count', NULL,
 'Rubrics and criteria published in a form another assessor can apply.', 50),

('education', 'missions_completed', 100.00, 'count', NULL,
 'Paid education missions carried through to the end.', 60),

('education', 'learners_reached', 25.00, 'log_scaled', NULL,
 'Learners taught, cumulated. Logarithmic, and figures declared from an outside platform count for half: this platform has no register of a class it did not host.', 70),

('education', 'review_grid_average', 200.00, 'offset_scaled', 3.00,
 'Average of the education review grids, counted above 3 out of 5.', 80),

('education', 'orientations_distinct', 20.00, 'count', NULL,
 'Education trades this person has verified work in.', 90),

('education', 'years_active', 25.00, 'count', NULL,
 'Whole years since the first verified education deliverable.', 100),

('education', 'featured_times', 200.00, 'count', NULL,
 'Times featured by the editors.', 110);
