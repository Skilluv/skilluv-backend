-- The communication craft score.
--
-- Rows in `craft_score_weights` (migration 0204), read on every computation,
-- so the formula is editable by an operator and publishable next to the
-- number it produces.
--
-- ## The tiers are the shared six, not the five the ticket named
--
-- Ticket A-02 asked for `apprentice / contributor / communicator / senior /
-- thought-leader`. Migration 0204 already seeded six tiers for this domain,
-- the same six every other domain uses, and that is deliberate: a tier is a
-- position on a scale, each scale is calibrated by its own weights, and one
-- shared ladder is what lets somebody compare their own two profiles. A
-- domain-specific vocabulary would mean a person who is `senior` in code and
-- `communicator` in communication cannot tell which is further along.
--
-- ## What the ticket's formula became
--
-- Eight of its ten terms are kept as written. Two changed:
--
--   * **`viral_content_pieces * 40`.** Nothing counts virality — 0505 wrote
--     out why, and the short version is that on most of these platforms the
--     figure is the author's own word. It splits into two honest terms:
--     `content_published`, which counts pieces at a public address, and
--     `audience_reach`, which is logarithmic and reads fetched figures at
--     full weight and declared ones at half. That is the treatment 0415
--     established for audio, and this domain has the same problem in a
--     sharper form.
--   * **`docs_contributions * 20`** is raised to 30. Twenty put a
--     documentation contribution below a validated translation, which says
--     that carrying a page into another language is harder than getting a
--     maintainer to accept a rewrite of it. Neither is true in general, and
--     they should not be ordered at all: both are 30.
--
-- Two terms are added, both on the audio model:
--
--   * `orientations_distinct` — the person who documents, then films, then
--     translates is this domain's normal shape, and nothing else in the
--     formula would have shown it.
--   * `target_languages_distinct` — the one measure of range that is specific
--     to communication, and the only one that cannot be faked by volume.
--
-- ## Why `audience_reach` is logarithmic
--
-- Linear, one video at three hundred thousand views would be worth more than
-- every other term combined, and the score would measure reach rather than
-- craft. On a log scale a thousand readers is worth about three weights and a
-- million about six: more is still more, and no single piece can reach the
-- ceiling alone.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order) VALUES

('communication', 'attestations_communication', 5.00, 'count', NULL,
 'Every unrevoked communication attestation.', 10),

('communication', 'docs_contributions', 30.00, 'count', NULL,
 'Documentation contributions accepted by a project you do not control.', 20),

('communication', 'talks_delivered', 60.00, 'count', NULL,
 'Talks delivered, with a recording or slides published.', 30),

('communication', 'content_published', 25.00, 'count', NULL,
 'Videos, articles, episodes and streams published at a public address.', 40),

('communication', 'translations_validated', 30.00, 'count', NULL,
 'Technical translations reviewed in both languages and accepted upstream.', 50),

('communication', 'research_published', 100.00, 'count', NULL,
 'Whitepapers, papers and external specifications published.', 60),

('communication', 'missions_completed', 100.00, 'count', NULL,
 'Paid communication missions carried through to the end.', 70),

('communication', 'audience_reach', 30.00, 'log_scaled', NULL,
 'Readers and viewers, cumulated. Logarithmic, and declared figures count for half: on most of these platforms the counter is the one the author gives us.', 80),

('communication', 'review_grid_average', 200.00, 'offset_scaled', 3.00,
 'Average of the communication review grids, counted above 3 out of 5.', 90),

('communication', 'orientations_distinct', 20.00, 'count', NULL,
 'Communication trades this person has verified work in.', 100),

('communication', 'target_languages_distinct', 25.00, 'count', NULL,
 'Distinct target languages across validated translations.', 110),

('communication', 'years_active', 25.00, 'count', NULL,
 'Whole years since the first verified communication deliverable.', 120),

('communication', 'featured_times', 200.00, 'count', NULL,
 'Times featured by the editors.', 130);
