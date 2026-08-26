-- One number for what somebody has done in security, and the words for it.
--
-- ## Why there is no `users.cyber_trust_score`
--
-- Ticket A-02 asked for two columns on `users` — a stored score and the date
-- it was computed — recomputed hourly by a cron, with the formula in Rust.
-- Both halves are refused, for the reasons `craft_score` (0195) and the ops
-- profile (0424) already wrote down and which this domain makes sharper than
-- any other:
--
--   * A stored score is wrong from the moment the next finding lands, and —
--     worse — it keeps its points when a proof is revoked. In a domain whose
--     whole product is "this proof can be checked", a number that survives the
--     revocation of what it rests on is the exact failure being sold against.
--   * A formula in Rust is one nobody outside the team can argue with. A
--     researcher who thinks a critical finding is worth more than four
--     capture-the-flag solves should be able to read the weights and say so.
--
-- So `security` gets rows in `craft_score_weights`, the score is computed on
-- read like every other domain's, and `services::security_profile` contributes
-- the one thing that cannot be a row: what each term counts.
--
-- The five display tiers A-02 asked for (Beginner, Confirmé, Expert, Elite,
-- Legend) become the six this platform already has, renamed for this trade.
-- Six rather than five because the thresholds are shared across domains and a
-- "Senior" has to mean the same distance travelled everywhere.
--
-- ## What the weights say, in one sentence
--
-- One confirmed critical finding outweighs twenty solved capture-the-flag
-- challenges. That is the whole editorial position of the domain, and it is
-- readable in the numbers rather than asserted in a charter: 180 against 160,
-- and the flags took a month.
--
-- ## The three terms that are somebody else's judgement
--
-- `findings_high_or_critical` counts the severity a validator settled, never
-- the one the reporter proposed — the argument 0451 made about self-rated
-- severity, which every bounty programme has already learned. `co_credits`
-- counts findings ruled duplicate by a human. `review_grid_average` is the
-- grids received. Nothing here can be raised by filing more of your own
-- opinion.
--
-- ## Why `findings_published` is worth more than `findings_confirmed`
--
-- Because it is a different act. Confirming means somebody reproduced it;
-- publishing means the write-up was good enough to teach a stranger, after the
-- embargo, with the reporter's name on it. Plenty of strong researchers never
-- do the second, and the platform should be able to tell the two apart.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES

('security', 'attestations_security', 5.00, 'count', NULL,
 'Each security attestation issued.', 10),

('security', 'findings_confirmed', 60.00, 'count', NULL,
 'Each reported vulnerability somebody else reproduced and accepted. Counts '
 'originals only: a duplicate has its own, smaller term.', 20),

('security', 'findings_high_or_critical', 120.00, 'count', NULL,
 'Each confirmed finding whose final severity is high or critical. The '
 'severity counted is the validator''s, never the reporter''s.', 30),

('security', 'findings_published', 80.00, 'count', NULL,
 'Each finding published after its embargo with a write-up. A different act '
 'from confirming it: this one had to be readable by a stranger.', 40),

('security', 'co_credits', 20.00, 'count', NULL,
 'Each independent co-discovery — the same vulnerability, found second. Worth '
 'recording, worth a third of an original, and never worth a bounty.', 50),

('security', 'ctf_solved', 8.00, 'count', NULL,
 'Each capture-the-flag challenge solved. Deliberately small: the answer was '
 'planted, and twenty of them are worth less than one confirmed critical.', 60),

('security', 'ctf_first_solves', 25.00, 'count', NULL,
 'Each challenge solved before anybody else. Says something the solve count '
 'does not — nobody had written it up yet.', 70),

('security', 'labs_completed', 12.00, 'count', NULL,
 'Each defensive lab passed. The reading half of the trade, and the part '
 'hardest to demonstrate anywhere else.', 80),

('security', 'walkthroughs_validated', 30.00, 'count', NULL,
 'Each machine or training-ground walkthrough whose write-up a senior '
 'accepted. Judged on the teaching, not on the box.', 90),

('security', 'audits_delivered', 90.00, 'count', NULL,
 'Each security code audit handed over and accepted: paths traced, '
 'reachability established, fixes proposed.', 100),

('security', 'threat_models_validated', 70.00, 'count', NULL,
 'Each threat model accepted. Worth less than an audit of running code and '
 'more than most people expect: it is the only artefact in this domain '
 'produced before the defect exists.', 105),

('security', 'incidents_analysed', 60.00, 'count', NULL,
 'Each real incident written up and accepted. Counts the analysis, never the '
 'severity of what happened.', 107),

('security', 'governance_artefacts_validated', 80.00, 'count', NULL,
 'Each policy, risk assessment or control mapping accepted against the '
 'framework it claims to answer.', 110),

('security', 'purple_exercises', 50.00, 'count', NULL,
 'Each purple exercise run to a written outcome. Counts the exercise, not a '
 'score: the format works when both sides get something.', 120),

('security', 'detections_shipped', 55.00, 'count', NULL,
 'Each detection rule built, validated by re-running the technique, and '
 'shipped. A rule nobody fired is a hypothesis.', 130),

('security', 'external_bounties_confirmed', 40.00, 'count', NULL,
 'Each bounty confirmed on another platform against its public disclosure. '
 'Recognised, and worth less than a finding this platform saw end to end.', 140),

('security', 'missions_completed', 100.00, 'count', NULL,
 'Each paid security engagement carried through and accepted.', 150),

('security', 'review_grid_average', 200.00, 'offset_scaled', 3.00,
 'The average of the review grids received, counted from 3 out of 5.', 160),

('security', 'years_active', 25.00, 'count', NULL,
 'Each year since the first verified security artefact.', 170),

('security', 'featured_times', 200.00, 'count', NULL,
 'Each featuring by the community.', 180);

-- ═══════════════════════════════════════════════════════════════════
-- The words
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0204 seeded generic names for the eleven declared domains —
-- Apprentice, Contributor, Engineer, Senior, Staff, Principal — so that a
-- foreign key had something to point at. The thresholds stay exactly as they
-- are; only the names and descriptions become this trade's, which is what
-- quality (0450) and ops (0424) did when they opened.

UPDATE craft_score_tiers SET
    name = 'Apprentice',
    description = 'Learning on planted targets. Nothing found yet that nobody '
                  'had put there.'
 WHERE skill_domain = 'security' AND slug = 'apprentice';

UPDATE craft_score_tiers SET
    name = 'Researcher',
    description = 'Has had a finding reproduced by somebody else. The report '
                  'was followable, which is the whole first hurdle.'
 WHERE skill_domain = 'security' AND slug = 'contributor';

UPDATE craft_score_tiers SET
    name = 'Security Engineer',
    description = 'Findings land, severities hold up under argument, and the '
                  'write-ups teach. Trusted with a real target unsupervised.'
 WHERE skill_domain = 'security' AND slug = 'engineer';

UPDATE craft_score_tiers SET
    name = 'Senior',
    description = 'Reads other people''s findings and is right about them. '
                  'Can say what a clean scan does not prove, and get that '
                  'accepted.'
 WHERE skill_domain = 'security' AND slug = 'senior';

UPDATE craft_score_tiers SET
    name = 'Security Lead',
    description = 'How an organisation decides what to defend carries this '
                  'person''s mark. Engagements, disclosure policy, and the '
                  'juniors who came through them.'
 WHERE skill_domain = 'security' AND slug = 'staff';

UPDATE craft_score_tiers SET
    name = 'Principal',
    description = 'Work that changed what a class of systems does about a '
                  'class of defect, beyond any one organisation.'
 WHERE skill_domain = 'security' AND slug = 'principal';
