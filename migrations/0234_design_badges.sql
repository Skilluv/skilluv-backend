-- Fourteen design distinctions.
--
-- ## All fourteen are counted
--
-- Migration 0177 had to mark six code badges manual, and 0212 one AI badge,
-- because nothing in the schema could count what they described. Design
-- arrives after `attestations.basis` (0233), after the validation rounds of
-- 0184, and after contests have a table — so every rule below reads a row
-- that already exists. None of them asks an operator to decide.
--
-- ## What the design backlog asked for and does not get
--
-- `design-portfolio-100`, awarded for "a hundred projects on the profile,
-- Behance import included". Migration 0145 makes external signals
-- display-only on purpose: importing an off-platform portfolio must never
-- move a rank, a badge or a search score, or "proven on Skilluv" stops
-- meaning anything. Counting Skilluv work only would have duplicated
-- `design-craft-legend`.
--
-- It is replaced by `design-range`, which rewards something no import can
-- fake and no volume badge expresses: having been validated across five
-- different design trades.
--
-- ## Why the volume thresholds sit where they do
--
-- Between the code ones and the AI ones. A validated design deliverable
-- costs more than a merged pull request and less than a published model: it
-- is days of work and a critique conversation, not an afternoon and not a
-- quarter.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

-- ── Volume ──────────────────────────────────────────────────────────
('design-first-artifact', 'medal',
 'Premier artefact design',
 'Un premier livrable design validé. Le moment où le profil cesse d''être déclaratif.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "design", "min_count": 1}', 'common'),

('design-craft-apprentice', 'medal',
 'Apprenti de l''atelier design',
 'Cinq livrables design validés. Le métier devient une habitude.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "design", "min_count": 5}', 'rare'),

('design-craft-master', 'medal',
 'Maître d''œuvre design',
 'Vingt-cinq livrables design validés. La régularité, pas le coup d''éclat.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "design", "min_count": 25}', 'epic'),

('design-craft-legend', 'medal',
 'Légende de l''atelier design',
 'Quatre-vingts livrables design validés. Une œuvre, pas un portfolio.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "design", "min_count": 80}', 'legendary'),

-- ── Étendue ─────────────────────────────────────────────────────────
('design-range', 'medal',
 'Polyvalence design',
 'Des livrables validés dans cinq métiers design différents. Ce qu''aucun import de portfolio ne peut simuler.',
 '{"proof_types": ["deliverable_verified"], "distinct_over": "orientation", "skill_domain": "design", "min_count": 5}', 'epic'),

-- ── Ce que la marque garde ──────────────────────────────────────────
('design-brand-delivered', 'medal',
 'Identité livrée',
 'Une identité de marque complète livrée, avec les guidelines qui permettent à d''autres de l''appliquer.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "design_brand_system_delivered", "min_count": 1}', 'rare'),

('design-typeface-released', 'medal',
 'Caractère publié',
 'Une famille de caractères publiée, avec ses fichiers de production.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "design_typeface_released", "min_count": 1}', 'epic'),

('design-system-adopted', 'medal',
 'Système adopté',
 'Un design system sur lequel une autre équipe construit. La preuve qu''il tient hors de son auteur.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "design_system_adopted", "min_count": 1}', 'epic'),

-- ── Concours ────────────────────────────────────────────────────────
('design-contest-winner', 'medal',
 'Vainqueur de concours',
 'Une première place dans un concours design Skilluv.',
 '{"proof_types": ["tournament_podium"], "skill_domain": "design", "rank_at_most": 1, "min_count": 1}', 'rare'),

('design-contest-champion', 'medal',
 'Champion des concours',
 'Trois concours design gagnés.',
 '{"proof_types": ["tournament_podium"], "skill_domain": "design", "rank_at_most": 1, "min_count": 3}', 'epic'),

('design-jury-member', 'medal',
 'Membre de jury',
 'Avoir jugé les propositions d''un concours design. Juger engage autant que concourir.',
 '{"proof_types": ["tournament_judged"], "skill_domain": "design", "min_count": 1}', 'rare'),

-- ── Ce que l''itération dit ──────────────────────────────────────────
('design-iteration-hero', 'medal',
 'Tenu jusqu''au bout',
 'Un livrable mené à la validation après cinq tours de critique. Recommencer quatre fois est plus rare que réussir du premier coup.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "design", "min_validation_rounds": 5, "min_count": 1}', 'rare'),

-- ── Missions payées ─────────────────────────────────────────────────
('design-mission-delivered', 'medal',
 'Mission livrée',
 'Une mission design payée, acceptée par le client.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "design_mission_delivered", "min_count": 1}', 'epic'),

-- ── Transmission ────────────────────────────────────────────────────
('design-mentor', 'medal',
 'Mentor design',
 'Trois personnes accompagnées jusqu''au bout d''une session. La transmission compte comme le métier.',
 '{"proof_types": ["mentorship_mentees_led"], "min_count": 3}', 'rare')

ON CONFLICT (slug) DO UPDATE SET
    display_name = EXCLUDED.display_name,
    description  = EXCLUDED.description,
    conditions   = EXCLUDED.conditions,
    rarity       = EXCLUDED.rarity,
    updated_at   = NOW();
