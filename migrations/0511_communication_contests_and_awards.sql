-- Two communication contest formats, and five categories in the ceremony
-- that already exists.
--
-- ## The formats
--
-- **`docs_jam`** — a weekend, a list of projects that want documentation, and
-- whoever turns up. Measured rather than judged: what counts is documentation
-- changes accepted upstream.
--
-- The entrant states the number, as they do in a code golf — the platform
-- does not count merges for them. What makes it a measurement rather than an
-- opinion is that every entry links the contributions it counts, and each one
-- is a public page showing whether a maintainer pressed the button. A wrong
-- number is refutable by anybody who opens the links, which is not true of a
-- judgement.
--
-- `evidence_rule` is in `required_rule_keys` for exactly that: a jam that does
-- not say how entries evidence their count is a jam ranked on typed numbers.
--
-- **`content_sprint`** — one imposed theme, one week, everybody publishes.
-- Judged by whoever shows up, because the thing being tested is whether a
-- piece landed on an audience, and an audience is the only instrument for
-- that.
--
-- ## Why the jam is measured and the sprint is voted
--
-- 0416 refused to invent a measurement for audio, on the grounds that
-- pretending — loudest, longest, most downloaded — ranks the wrong thing.
-- The inverse applies here: the jam has a real measurement lying around, and
-- putting a jury in front of it would replace a fact with an opinion.
--
-- The two flags are mutually exclusive by CHECK, which is what
-- `a_format_is_not_judged_two_ways` says: a format is ranked one way, and a
-- contest that wanted both would be two contests.
--
-- ## Five categories in one ceremony
--
-- Same reasoning as 0303 and 0416: one evening, categories from every domain.
-- A translator and a compiler author named at the same ceremony is what makes
-- the communication categories visible to people who would never have gone
-- looking for them.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, is_juried, allows_community_vote, required_rule_keys, sort_order) VALUES

    ('docs_jam', 'communication', 'Documentation jam',
     'One weekend, a list of projects short of documentation, and the number '
     'of contributions accepted upstream. Ranked on a measurement rather than '
     'an opinion: a maintainer pressed the button, and that is not arguable.',
     TRUE, TRUE, FALSE, FALSE, FALSE,
     '{project_list,duration_hours,evidence_rule}', 310),

    ('content_sprint', 'communication', 'Content sprint',
     'One imposed theme, one week, and everybody publishes what they like on '
     'it. Settled by the audience, because what is being tested here is '
     'precisely the meeting with an audience.',
     TRUE, FALSE, FALSE, FALSE, TRUE,
     '{theme,publication_deadline}', 320);

-- ═══════════════════════════════════════════════════════════════════
-- Five communication categories
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO award_categories (slug, name, description, subject_type, sort_order) VALUES

    ('best-docs-contribution',
     'Best Documentation Contribution of the Year',
     'The documentation contribution that unblocked the most people. Often a page nobody wants to write.',
     'deliverable', 410),

    ('best-conference-talk',
     'Best Conference Talk of the Year',
     'A talk remembered because something was understood, not because it was impressive.',
     'deliverable', 420),

    ('best-tech-content-creator',
     'Best Tech Content Creator of the Year',
     'The person whose every publication gets watched. Rewards a standard and a cadence held for a year.',
     'user', 430),

    ('best-translation-contribution',
     'Best Translation Contribution of the Year',
     'The translation that opened a tool to people it had shut out. The category where the best work is the work its beneficiaries never notice happened.',
     'deliverable', 440),

    ('best-research-writing',
     'Best Research Writing of the Year',
     'A whitepaper, report or specification whose method holds and whose limits were written by its author.',
     'deliverable', 450);
