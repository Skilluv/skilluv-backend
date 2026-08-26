-- The security badges, in the rules engine every domain shares.
--
-- ## What the engine had to learn
--
-- Two proof types are added to `services::badge_engine` alongside this
-- migration, because two of the distinctions this domain cares about most were
-- not expressible:
--
--   * `security_finding_confirmed` with `min_severity`. "Three high findings"
--     and "any three findings" are different claims, and the attestation basis
--     carries no severity. The engine reads `security_findings.severity_tier`,
--     which is the validator's figure and not the reporter's — a badge keyed
--     to a self-rated severity is a badge you award yourself.
--   * `security_ctf_first_solve`. Being first is a property of a timestamp
--     across all users, not of an attestation, and no existing proof type can
--     see it.
--
-- Everything else here is expressible with the grammar as it stood, which is
-- the point of having one.
--
-- ## Why "twenty flags" is `rare` and "one critical" is `legendary`
--
-- Deliberate, and the same editorial line the craft-score weights take. Flags
-- are planted answers on a target somebody built to be broken; a confirmed
-- critical on a live system is the thing this domain exists to recognise. A
-- badge scheme that rewarded volume of training would fill profiles with
-- evidence of practice and none of work.
--
-- ## No `security-first-blood-{challenge}` badge
--
-- Ticket C-08 asked for one unique badge per challenge, minted on the fly.
-- Refused: `badge_rules` is a curated catalogue that an operator reads and
-- edits, and a rule per challenge turns it into a log. One
-- `security-first-blood` badge counts the first solves, and which ones they
-- were is on the profile, where a reader can see the challenge names rather
-- than a wall of near-identical medals.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Findings
-- ═══════════════════════════════════════════════════════════════════

('security-first-finding', 'medal',
 'First finding',
 'A vulnerability reported and reproduced by somebody else. The moment a security profile stops being a list of courses.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_finding_confirmed", "min_count": 1}', 'common'),

('security-bug-hunter', 'medal',
 'Bug hunter',
 'Five confirmed findings. Regularity, which is harder than one good afternoon.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_finding_confirmed", "min_count": 5}', 'rare'),

('security-serious-findings', 'medal',
 'Serious findings',
 'Three confirmed findings rated high or critical. The severity counted is the validator''s, never the reporter''s.',
 '{"proof_types": ["security_finding_confirmed"], "min_severity": "high", "min_count": 3}', 'epic'),

('security-critical-hero', 'medal',
 'Critical finding',
 'A confirmed critical. One is enough: this is the artefact the whole domain is built to produce.',
 '{"proof_types": ["security_finding_confirmed"], "min_severity": "critical", "min_count": 1}', 'legendary'),

('security-disclosed', 'medal',
 'Published a disclosure',
 'A finding taken all the way: embargo held, write-up published, and readable by somebody who was not there.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_finding_published", "min_count": 1}', 'rare'),

('security-disclosure-author', 'medal',
 'Disclosure author',
 'Ten published disclosures. At this point the write-ups are a body of teaching, not a by-product.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_finding_published", "min_count": 10}', 'epic'),

('security-external-bounty', 'medal',
 'Paid elsewhere',
 'A bounty confirmed on another platform against its public disclosure. Recognised here without being pretended to be ours.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_external_bounty_confirmed", "min_count": 1}', 'rare'),

-- ═══════════════════════════════════════════════════════════════════
-- Practice
-- ═══════════════════════════════════════════════════════════════════

('security-first-flag', 'medal',
 'First flag',
 'A capture-the-flag challenge solved. The answer was planted; finding it still took the tools and the method.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_ctf_solved", "min_count": 1}', 'common'),

('security-flag-veteran', 'medal',
 'Flag veteran',
 'Twenty challenges solved. Deliberately rare rather than epic: this is evidence of practice, and practice is not work.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_ctf_solved", "min_count": 20}', 'rare'),

('security-first-blood', 'medal',
 'First blood',
 'Solved a challenge before anybody else had. Nobody had written it up yet, which is a different afternoon entirely.',
 '{"proof_types": ["security_ctf_first_solve"], "min_count": 1}', 'rare'),

('security-first-blood-hunter', 'medal',
 'First blood hunter',
 'Five challenges solved before anybody else.',
 '{"proof_types": ["security_ctf_first_solve"], "min_count": 5}', 'epic'),

('security-blue-analyst', 'medal',
 'Blue analyst',
 'Ten defensive labs passed: logs, captures and memory images read to a conclusion that followed from them.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_blue_lab_completed", "min_count": 10}', 'rare'),

('security-walkthrough-author', 'medal',
 'Walkthrough author',
 'Five machine or training-ground write-ups a senior accepted. Judged on the teaching, not on the box.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_machine_walkthrough_validated", "min_count": 5}', 'rare'),

-- ═══════════════════════════════════════════════════════════════════
-- The trades
-- ═══════════════════════════════════════════════════════════════════

('security-first-artifact', 'medal',
 'First security artefact',
 'A first verified security deliverable — an audit, a hunt, a threat model, a policy set.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "security", "min_count": 1}', 'common'),

('security-craft-master', 'medal',
 'Established practitioner',
 'Twenty-five verified security deliverables.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "security", "min_count": 25}', 'epic'),

('security-craft-legend', 'medal',
 'Master of the trade',
 'Eighty verified security deliverables.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "security", "min_count": 80}', 'legendary'),

('security-code-auditor', 'medal',
 'Code auditor',
 'Five security audits handed over and accepted: paths traced, reachability established, fixes proposed.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_code_audit_delivered", "min_count": 5}', 'rare'),

('security-governance-guru', 'medal',
 'Governance',
 'Five policies, risk assessments or control mappings accepted against the framework they claim to answer.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_policy_validated", "min_count": 5}', 'rare'),

('security-purple-master', 'medal',
 'Purple',
 'Three exercises run where attack and defence produced a detection between them.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "security_purple_exercise_facilitated", "min_count": 3}', 'epic'),

('security-multi-trade', 'medal',
 'Across the trades',
 'Verified work in three of the five security trades. Rare because most people specialise, and the ones who do not are who you want in a purple exercise.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "security", "distinct_over": "orientation", "min_count": 3}', 'epic'),

-- ═══════════════════════════════════════════════════════════════════
-- Competitions, missions, community
-- ═══════════════════════════════════════════════════════════════════

('security-competition-winner', 'medal',
 'Competition winner',
 'First place in a hosted security competition.',
 '{"proof_types": ["tournament_podium"], "rank_at_most": 1, "skill_domain": "security", "min_count": 1}', 'rare'),

('security-competition-champion', 'medal',
 'Competition champion',
 'Three wins. Once is a good weekend; three is a habit.',
 '{"proof_types": ["tournament_podium"], "rank_at_most": 1, "skill_domain": "security", "min_count": 3}', 'epic'),

('security-jury-member', 'medal',
 'Judged a competition',
 'Sat on the jury of a security competition. Reading forty submissions is service, and it is invisible unless somebody records it.',
 '{"proof_types": ["tournament_judged"], "skill_domain": "security", "min_count": 1}', 'rare'),

('security-first-mission', 'medal',
 'First paid engagement',
 'A commissioned security engagement carried through and accepted by the client.',
 '{"proof_types": ["mission_completed"], "skill_domain": "security", "min_count": 1}', 'epic'),

('security-mission-veteran', 'medal',
 'Engagement veteran',
 'Ten paid security engagements delivered.',
 '{"proof_types": ["mission_completed"], "skill_domain": "security", "min_count": 10}', 'legendary'),

('security-mentor-active', 'medal',
 'Mentoring',
 'Three juniors accompanied to a finished session. A first finding is where most people give up, and the ones who do not usually had somebody to ask.',
 '{"proof_types": ["mentee_guided"], "skill_domain": "security", "min_count": 3}', 'rare'),

('security-mentor-veteran', 'medal',
 'Mentor',
 'Ten juniors accompanied. Counts people, never sessions.',
 '{"proof_types": ["mentee_guided"], "skill_domain": "security", "min_count": 10}', 'epic'),

('security-featured', 'medal',
 'Featured',
 'Put forward by the security community for a week.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "featured_security_researcher", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The one nobody can count
-- ═══════════════════════════════════════════════════════════════════
--
-- Awarded by a curator who read the case, because the thing being recognised
-- is a judgement and inventing a rule for it would award it to the wrong
-- people — the argument 0522 makes for its own manual badge.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('security-restraint', 'medal',
 'Stopped at the boundary',
 'Reached the edge of the authorised scope, stopped, and said so in the report — including what they believed was on the other side. The single most valuable habit in this trade and the one nothing can measure: a scope respected leaves no evidence.',
 '{"manual": true}', 'legendary');
