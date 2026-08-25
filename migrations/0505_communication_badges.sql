-- Twelve communication distinctions.
--
-- ## Ten are counted, two are not
--
-- Migration 0212 set the standard: a rule that counts something else than
-- what the badge names awards it to people who never did the thing. Where a
-- rule can be written it is written, and where it cannot the row says a human
-- decides.
--
-- The bases created in 0504 make most of these countable. The two that stay
-- manual are the two whose evidence lives entirely outside this database, and
-- both were asked for as automatic rules:
--
--   * **`communication-viral-content`.** The backlog wanted it fired at ten
--     thousand views. On the platforms this domain actually publishes on, the
--     view count is usually the person's own word — 0415 wrote the rule for
--     that, and 0504 restated it. A badge awarded on a self-declared number
--     is a badge people award themselves. A curator who opens the link and
--     sees the counter is the honest version, and it is what `manual` means.
--   * **`communication-research-cited`.** Citation counts live in Google
--     Scholar and Semantic Scholar. Nothing here can see them, and the proxy
--     available — how many people clicked — measures a different thing
--     entirely.
--
-- ## Where the backlog's thresholds moved, and why
--
-- **Ten documentation contributions** for `communication-docs-published` is
-- kept: a documentation pull request accepted upstream is a day's work and
-- ten is a genuine record, not a career.
--
-- **Five conference talks** for `communication-devrel-veteran` is kept for
-- the same reason, with the note that the basis counts talks *delivered* —
-- an accepted proposal is not one.
--
-- ## Where a badge was added
--
-- `communication-multi-trade`, on the model of `audio-multi-trade` and
-- `ai-multi-modal`. The person who documents a project, then films the
-- tutorial, then translates both is the normal shape of this domain rather
-- than the exception, and nothing else in the set would have shown it.
--
-- ## `communication-polyglot` needs the engine to learn one dimension
--
-- Counting distinct target languages is not something `distinct_over` could
-- do when this was written; the dimension is added to `badge_engine` in the
-- same change. The rule is written here in its final form rather than
-- deferred: a badge seeded with a condition nothing implements is a badge
-- that silently never fires, which is precisely the failure the constant
-- `PROOF_TYPES` exists to catch.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('communication-first-artifact', 'medal',
 'First publication',
 'A first verified communication deliverable. The moment the profile stops being a claim.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "communication", "min_count": 1}', 'common'),

('communication-craft-master', 'medal',
 'Established writer',
 'Thirty verified communication deliverables. Regularity, not one good day.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "communication", "min_count": 30}', 'epic'),

('communication-craft-legend', 'medal',
 'Voice of the community',
 'One hundred verified communication deliverables.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "communication", "min_count": 100}', 'legendary'),

('communication-docs-published', 'medal',
 'Upstream documentarian',
 'Ten documentation contributions accepted by projects you do not control.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "communication_docs_contribution", "min_count": 10}', 'epic'),

('communication-devrel-veteran', 'medal',
 'Stage veteran',
 'Five talks delivered. An accepted proposal is not one.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "communication_talk_delivered", "min_count": 5}', 'epic'),

('communication-content-regular', 'medal',
 'Regular publisher',
 'Twenty technical pieces published: videos, articles, episodes, streams.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "communication_content_published", "min_count": 20}', 'rare'),

('communication-polyglot', 'medal',
 'Polyglot',
 'Validated technical translations into five different target languages.',
 '{"distinct_over": "target_language", "skill_domain": "communication", "min_count": 5}', 'epic'),

('communication-multi-trade', 'medal',
 'Multi-trade communicator',
 'Verified work in three different communication trades.',
 '{"distinct_over": "orientation", "skill_domain": "communication", "min_count": 3}', 'epic'),

('communication-mission-veteran', 'medal',
 'Communication mission veteran',
 'Ten paid communication missions carried through to the end.',
 '{"proof_types": ["mission_completed"], "skill_domain": "communication", "min_count": 10}', 'legendary'),

('communication-featured', 'medal',
 'Featured',
 'Communication work picked out by the editors as exemplary.',
 '{"proof_types": ["deliverable_featured"], "skill_domain": "communication", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The two a human decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('communication-viral-content', 'medal',
 'Content that carried',
 'A publication whose audience went far beyond the usual circle. Awarded by a curator who opened the link: on most of these platforms the counter is the author''s own word.',
 '{"manual": true}', 'epic'),

('communication-research-cited', 'medal',
 'Work cited',
 'Research writing taken up and cited by others. Citation counters live outside this database, and no figure available here measures the same thing.',
 '{"manual": true}', 'legendary');
