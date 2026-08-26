-- Ten education distinctions.
--
-- ## Nine are counted, one is not
--
-- Migration 0212 set the standard: a rule that counts something else than
-- what the badge names awards it to people who never did the thing. Where a
-- rule can be written it is written, and where it cannot the row says a human
-- decides.
--
-- ## Where the backlog's thresholds moved, and why
--
-- **`education-teacher-1000-hours`** is not here, and its absence is the
-- whole argument of 0521 in badge form. A thousand cumulated teaching hours
-- is a real and impressive thing, and the platform has no register, no
-- attendance and no way to ask: the number would be whatever the person
-- typed. A badge awarded on a typed number is a badge people award
-- themselves.
--
-- What replaces it is `education-cohort-veteran` at five cohorts *delivered
-- on this platform*, which is a smaller claim and a true one. Teaching done
-- elsewhere still counts — as a portfolio entry marked as declared, and in
-- the craft score at a discount — and that is the honest place for it.
--
-- **Ten adopted curriculums** for `education-curriculum-architect` is lowered
-- to five. Ten curriculums picked up by other trainers is a career; five is
-- already somebody whose work travels, which is what the badge is for.
--
-- ## Where a badge was added
--
-- `education-cross-domain`, on the model of `audio-multi-trade`. Somebody who
-- has taught in three different subject domains has demonstrated the thing
-- that separates a teacher from a practitioner who occasionally presents:
-- the craft is transferable, and theirs has transferred.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('education-first-artifact', 'medal',
 'First session',
 'A first verified education deliverable. The moment the profile stops being a claim.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "education", "min_count": 1}', 'common'),

('education-craft-master', 'medal',
 'Established teacher',
 'Thirty verified education deliverables. Regularity, not one good day.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "education", "min_count": 30}', 'epic'),

('education-craft-legend', 'medal',
 'Master of the workshop',
 'One hundred verified education deliverables.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "education", "min_count": 100}', 'legendary'),

('education-cohort-veteran', 'medal',
 'Cohort veteran',
 'Five cohorts run to the end, with their outcomes on the record.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "education_cohort_delivered", "min_count": 5}', 'epic'),

('education-workshop-regular', 'medal',
 'Workshop regular',
 'Fifteen live sessions delivered, with their materials published.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "education_workshop_delivered", "min_count": 15}', 'rare'),

('education-curriculum-architect', 'medal',
 'Curriculum architect',
 'Five curriculums picked up and run by trainers other than their author.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "education_curriculum_authored", "min_count": 5}', 'epic'),

('education-cross-domain', 'medal',
 'Teaches across trades',
 'Verified teaching work in three different education trades.',
 '{"distinct_over": "orientation", "skill_domain": "education", "min_count": 3}', 'epic'),

('education-mission-veteran', 'medal',
 'Education mission veteran',
 'Ten paid education missions carried through to the end.',
 '{"proof_types": ["mission_completed"], "skill_domain": "education", "min_count": 10}', 'legendary'),

('education-featured', 'medal',
 'Featured',
 'Education work picked out by the editors as exemplary.',
 '{"proof_types": ["deliverable_featured"], "skill_domain": "education", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════
--
-- A turnaround is the best work in this domain and the least countable. The
-- learner who was going to leave in week three and finished the programme is
-- visible in a completion rate as one row among twenty; what made the
-- difference was an intervention somebody made, and the only evidence is the
-- case study they wrote about it. A curator reads that.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('education-turnaround', 'medal',
 'Kept somebody in the room',
 'A documented case of a learner who was going to give up and did not. Awarded by a curator who read the case study: a completion rate cannot show which row was the difficult one.',
 '{"manual": true}', 'legendary');
