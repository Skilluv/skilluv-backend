-- Where an educator's recorded career already lives, and two contest formats.
--
-- ## Portfolio platforms (tickets P-01 and P-02)
--
-- Rows in `portfolio_platforms` (migration 0415), which carries what each
-- platform's numbers mean — items are repositories on GitHub, tracks on
-- Bandcamp, articles on DEV, and courses here.
--
-- Both tickets asked for scraping: reading a public Udemy profile, or a
-- LinkedIn Learning instructor page, for figures those services do not
-- publish. 0415 refused that and 0510 restated the refusal: the terms forbid
-- it, and a figure obtained that way is indistinguishable in this table from
-- one somebody typed.
--
-- So none of them is fetched, all of them are declared, and
-- `education_profile` counts their enrolment figures at half. Which is the
-- honest description of this domain: almost none of the teaching that exists
-- happens somewhere with an API.
--
-- ## `student_testimonials` is not a platform
--
-- Ticket P-02 wanted testimonials imported as a portfolio row, "screenshots +
-- text + consent". A screenshot of somebody praising you is not a portfolio
-- account, it has no handle and no profile URL, and storing it here would
-- mean a table of images of third parties with a consent flag next to them.
--
-- Migration 0524 put testimonials where they belong: on the learner's own
-- outcome row, written by the learner, with consent as a timestamp and a
-- CHECK that refuses to hold text without one. A testimonial from somebody
-- who was never in a cohort here has no record, and that is correct — it is
-- an unverifiable quote, and this platform's whole argument is against those.
--
-- ## Contests (tickets C-01 and C-02)
--
-- One format rather than the jam alone. `curriculum_jam` is the ticket's, and
-- `teach_off` is added because a domain whose only contest is a writing
-- exercise would rank the design of teaching and never the doing of it.
--
-- Both are settled by the audience, and deliberately: 0416 refused to invent
-- a measurement for audio because pretending ranks the wrong thing. There is
-- no honest measurement of a lesson at contest scale — completion rates over
-- one weekend measure who picked the easier topic — so the people in the room
-- decide.

-- ═══════════════════════════════════════════════════════════════════
-- Where courses are already published
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label, has_public_api, sort_order) VALUES
    ('udemy', 'education', 'Udemy',
     'https://www.udemy.com/user/{handle}/', 'courses', 'enrolments', FALSE, 410),
    ('coursera', 'education', 'Coursera',
     'https://www.coursera.org/instructor/{handle}', 'courses', 'enrolments', FALSE, 420),
    ('linkedin_learning', 'education', 'LinkedIn Learning',
     NULL, 'courses', NULL, FALSE, 430),
    ('teachable', 'education', 'Teachable',
     NULL, 'courses', 'enrolments', FALSE, 440),
    ('openclassrooms', 'education', 'OpenClassrooms',
     'https://openclassrooms.com/fr/members/{handle}', 'courses', NULL, FALSE, 450),
    ('exercism', 'education', 'Exercism',
     'https://exercism.org/profiles/{handle}', 'tracks mentored', 'learners', FALSE, 460);

-- ═══════════════════════════════════════════════════════════════════
-- Two contest formats
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, is_juried, allows_community_vote, required_rule_keys, sort_order) VALUES

    ('curriculum_jam', 'education', 'Curriculum jam',
     'One imposed theme, one week, and everybody designs a programme for it. '
     'Settled by the people who would have to teach it, which is the only '
     'instrument there is: a curriculum has no measurement until somebody '
     'runs it, and that takes a term.',
     TRUE, FALSE, FALSE, FALSE, TRUE,
     '{theme,audience}', 410),

    ('teach_off', 'education', 'Teach-off',
     'The same concept, the same fifteen minutes, the same room of people who '
     'do not know it yet — and they say afterwards whether they do. A domain '
     'whose only contest is a writing exercise would rank the design of '
     'teaching and never the doing of it.',
     TRUE, FALSE, FALSE, FALSE, TRUE,
     '{concept,duration_minutes}', 420);

-- ═══════════════════════════════════════════════════════════════════
-- Five education categories, in the ceremony that already exists
-- ═══════════════════════════════════════════════════════════════════
--
-- Same reasoning as 0303, 0416 and 0511: one evening, categories from every
-- domain. `rookie-educator` is the one category on the platform aimed at
-- somebody in their first two years, and it is here because this is the
-- domain people arrive in sideways — from a job, from a school, from having
-- explained something once and found they could.

INSERT INTO award_categories (slug, name, description, subject_type, sort_order) VALUES

    ('best-trainer',
     'Best Trainer of the Year',
     'The person whose sessions people leave able to do the thing. Judged on what participants did afterwards, not on how the room felt at the time.',
     'user', 510),

    ('best-curriculum',
     'Best Curriculum of the Year',
     'A programme other trainers picked up and ran. The category where the evidence is that somebody else trusted it.',
     'deliverable', 520),

    ('best-coding-teacher',
     'Best Coding Teacher of the Year',
     'For the day-to-day work with beginners, which is the hardest teaching there is and the least visible.',
     'user', 530),

    ('rookie-educator',
     'Rookie Educator of the Year',
     'Under two years on the platform. This is the domain people arrive in sideways, and the first year is the one where they find out whether they can.',
     'user', 540),

    ('cross-domain-educator',
     'Cross-domain Educator of the Year',
     'Verified teaching in three or more subject domains. Evidence that the craft transferred rather than that the subject was easy.',
     'user', 550);
