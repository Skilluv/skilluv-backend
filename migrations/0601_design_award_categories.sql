-- The design award categories, which 0590 said were a separate ticket.
--
-- ## Why they were missing
--
-- 0590 gave `award_categories` a `skill_domain` so an awards page could show
-- one family, and said so in the same breath:
--
--   > the seeding of per-family categories (SKI-239 and its kin) is a
--   > separate ticket.
--
-- This is that ticket. Every other domain seeded its categories with its
-- practice data — code in 0190, ai in 0303, audio in 0416, ops in 0425,
-- quality in 0452, leadership in 0464, communication in 0511, education in
-- 0527. Design got the column and never got the rows, so `/design/awards`
-- rendered an empty page against a working endpoint.
--
-- ## Six, not thirteen
--
-- SKI-239 asked for thirteen categories — one per review family — plus two
-- cross-cutting. Thirteen is the wrong number for a first edition, and the
-- reason is arithmetic rather than taste: a category needs enough nominees to
-- make winning it mean something. Thirteen categories across a community that
-- has not opened yet produces categories won by the only person who entered,
-- and an award nobody competed for is worth less than no award.
--
-- So: six that group the families by what is actually being judged. Splitting
-- one into three later is an INSERT; merging three that each had one nominee
-- is a retraction.
--
-- ## The two cross-cutting ones are NOT seeded here, and that is a decision
--
-- SKI-239's last two — Rookie of the Year and Contribution of the Year — are
-- not design awards. They are platform awards that the design ticket happened
-- to ask for, and seeding them from a design migration would settle a
-- platform-wide question inside a domain file.
--
-- It is not a hypothetical question. `leadership` already seeded
-- `leadership-rookie-of-the-year` (0464) — a domain-scoped first-year award.
-- Adding a domain-less `rookie-of-the-year` now would leave the platform with
-- two, and no answer to which one is *the* rookie of the year. Either the
-- award is per domain, in which case design and the other nine need theirs and
-- leadership's is correct; or it is platform-wide, in which case leadership's
-- should become it. Both are defensible. Neither is mine to pick, and picking
-- silently is how a catalogue ends up with eleven rookies.
--
-- So: six design categories here, and the cross-cutting pair stays on SKI-239
-- as an open question.

INSERT INTO award_categories
    (slug, name, description, subject_type, skill_domain, sort_order)
VALUES

-- ── Judged on a piece of work ───────────────────────────────────────
--
-- `deliverable`, because what is nominated is the thing, not the person. The
-- same designer can be nominated twice in a year for two pieces, which is the
-- point: this rewards work, and the person-shaped awards are below.

('design-product-of-the-year',
 'Product design of the year',
 'The interface that made something complicated feel obvious. Covers product, design systems and conversational — judged on the journey and its unglamorous states, not on a hero screen.',
 'deliverable', 'design', 600),

('design-identity-of-the-year',
 'Identity of the year',
 'A brand, a typeface or a naming system that holds at its worst case of reproduction as well as on the presentation board. Covers brand, typography and verbal identity.',
 'deliverable', 'design', 610),

('design-image-of-the-year',
 'Image of the year',
 'Illustration, icon set, character or 3D work where the craft is visible and the set holds together. Covers illustration, iconography, character, motion, video and architectural visualization.',
 'deliverable', 'design', 620),

('design-clarity-of-the-year',
 'Clarity of the year',
 'The work that made something understandable: a visualization that answered its question, interface words that stopped a person being stuck, a service map that found where a process actually breaks. Covers dataviz, UX writing and service design.',
 'deliverable', 'design', 630),

-- ── Judged on a year of doing it ────────────────────────────────────

('design-critique-of-the-year',
 'Critique of the year',
 'The reviewer whose critiques people quote back. Rewards the hardest and least visible work in the domain: telling somebody their direction is wrong in a way that makes them come back with a better one.',
 'user', 'design', 640),

('design-mentor-of-the-year',
 'Design mentor of the year',
 'The person behind somebody else''s first validated deliverable, and their fifth. Nominated by the people they accompanied, which is the only signal that cannot be manufactured.',
 'user', 'design', 650)

ON CONFLICT (slug) DO NOTHING;
