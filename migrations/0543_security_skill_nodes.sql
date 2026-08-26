-- The skills the four new security trades are made of.
--
-- ## What the catalogue already held
--
-- Forty-eight nodes under eight roots, written by 0057 and extended since:
-- `web-app-security`, `auth-security`, `cryptography-basics`,
-- `infrastructure-security`, `offensive-basics`, `reporting`, `llm-security`,
-- `firmware-security`. That tree was built for one orientation — a developer
-- who does not want to ship the defect — and it shows: it is almost entirely
-- web and application security, and there is no node anywhere in it for
-- reading a log, writing a policy, or emulating an adversary.
--
-- So this migration adds four roots and leaves the eight alone. Nothing is
-- renamed and nothing moves: `web-app-security` is exactly what
-- `security-code-audit` and `security-red-team` both need, and a node used by
-- two orientations is what `orientation_skill_map` is for.
--
-- ## Four roots, one per gap
--
--   * `defensive-operations` — the blue-team gap. Everything about noticing.
--   * `secure-code-audit` — reading code for exploitable defects, as opposed
--     to `web-app-security`, which names the defect classes themselves. The
--     distinction matters: knowing what an IDOR is and being able to find one
--     in forty thousand lines are different competences, and only the second
--     is what a code-audit reviewer has to judge.
--   * `security-governance-practice` — the documents and the audit.
--   * `adversary-emulation` — the purple gap: building the detection and then
--     testing it against the technique it was built for.
--
-- ## Slugs that collide, and what was done instead
--
-- `incident-response` is an ops node since 0057 and stays there — running a
-- production incident is the same competence whether the cause was an
-- attacker or a bad deploy. This domain adds
-- `incident-response-command-security`, which is the part that is not shared:
-- preserving evidence while restoring service, and the reporting duty.
--
-- `sast-rule-authoring`, `dast-orchestration`, `false-positive-triage` and
-- `rules-of-engagement` belong to the quality domain since 0454, which got
-- there first and where they are about a pipeline rather than an engagement.
-- The security nodes below are the reading half — `sast-finding-triage`,
-- `scan-result-to-finding` — and the two domains reach across through
-- `orientation_skill_map`.
--
-- ## `ON CONFLICT (slug) DO NOTHING` on every statement
--
-- For the reason 0518 gives: a migration that refuses to run because somebody
-- named a skill first is a merge conflict discovered at deploy time.

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('defensive-operations', 'Defensive operations',
 'Noticing that something happened, and being able to say how you noticed.',
 'security'),
('secure-code-audit', 'Auditing code for security',
 'Finding the exploitable defect in code somebody else wrote, and proving the path to it.',
 'security'),
('security-governance-practice', 'Security governance',
 'What an organisation says it does about risk, written so that an auditor accepts it.',
 'security'),
('adversary-emulation', 'Adversary emulation',
 'Reproducing a known technique on purpose, so that the detection for it can be built and tested.',
 'security')
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Defensive operations
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('log-triage', 'Reading a log you did not write',
     'Finding the three lines that matter in four million, and knowing which fields the format is lying about.'),
    ('detection-rule-authoring', 'Writing a detection rule',
     'Sigma, KQL, Suricata, a SIEM query. The rule that fires on the sample and stays quiet on a week of normal traffic.'),
    ('detection-tuning', 'Tuning a rule that fires too often',
     'The work after the rule ships. An alert nobody reads any more is a deleted rule that still costs money.'),
    ('threat-hunting', 'Hunting without an alert',
     'Starting from a hypothesis rather than a notification, and being able to say what would have disproved it.'),
    ('ioc-extraction', 'Extracting indicators',
     'Turning one incident into hashes, domains and patterns another team can search their own estate for.'),
    ('network-traffic-analysis', 'Reading captured traffic',
     'Wireshark and tshark on a real capture: reassembling a session, finding the exfiltration in what looks like DNS.'),
    ('memory-forensics', 'Reading a memory image',
     'Volatility on a dump: what was running, what was injected, and what was in memory that was never on disk.'),
    ('disk-timeline-forensics', 'Building a timeline from a disk',
     'Filesystem timestamps, artefacts, registry or journal. The order of events, defensible to somebody who will contest it.'),
    ('malware-triage', 'Triaging a suspicious file',
     'Static and dynamic first pass in a sandbox: what it is, what it talks to, and whether it needs a specialist.'),
    ('incident-response-command-security', 'Commanding a security incident',
     'Restoring service without destroying the evidence, and knowing what the notification duty is before the clock starts.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'defensive-operations') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Auditing code for security
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('injection-path-tracing', 'Tracing a path from input to sink',
     'Following untrusted data through the layers that were supposed to clean it, and finding the branch where nothing did.'),
    ('authorization-logic-review', 'Reviewing who is allowed to do what',
     'Reading the check rather than the endpoint. The bug is almost always a missing comparison, not a missing middleware.'),
    ('threat-modeling-stride', 'Threat modelling a design',
     'STRIDE or equivalent on an architecture, before the code exists. Output is a prioritised list, not a diagram.'),
    ('crypto-misuse-review', 'Finding misused cryptography',
     'The right primitive used wrongly: reused nonces, unauthenticated encryption, comparison that leaks timing.'),
    ('deserialization-review', 'Reviewing deserialisation and parsing',
     'Where a byte string becomes an object, and what that object is allowed to do on the way in.'),
    ('dependency-supply-chain-audit', 'Auditing what the project pulls in',
     'Lockfiles, transitive trees, typosquats, unmaintained packages, and the difference between a CVE and a reachable one.'),
    ('secrets-in-history', 'Finding credentials in history',
     'A key rotated is not a key removed. Reading a repository''s past, and knowing what to do about what is in it.'),
    ('sast-finding-triage', 'Triaging what a scanner reported',
     'Deciding which of two hundred findings is real, with the reason written down. An untriaged report is tool output.'),
    ('scan-result-to-finding', 'Turning a scan hit into a finding',
     'The work between "the tool flagged line 40" and a report somebody can act on: reachability, impact, proof.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'secure-code-audit') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Security governance
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('gdpr-compliance', 'GDPR in practice',
     'Lawful basis, retention, subject rights, and the register that says which is which for each processing purpose.'),
    ('dpia-authoring', 'Writing a data protection impact assessment',
     'The assessment for the processing that needs one, including the part where a mitigation is refused and why.'),
    ('iso-27001-isms', 'Running an ISO 27001 management system',
     'Scope, risk treatment plan, statement of applicability, and the internal audit that keeps it true.'),
    ('soc2-readiness', 'Getting ready for a SOC 2',
     'Choosing the trust criteria, mapping the controls onto what the company actually does, and closing the gap list.'),
    ('risk-assessment-method', 'Assessing risk with a method',
     'A repeatable scale rather than an opinion: likelihood, impact, and two assessors landing in the same place.'),
    ('security-policy-authoring', 'Writing a policy people follow',
     'Short enough to be read, specific enough to be audited, and possible to comply with on an ordinary day.'),
    ('third-party-risk-review', 'Reviewing a supplier',
     'What the vendor questionnaire does not ask. Subprocessors, data location, and what happens at termination.'),
    ('audit-evidence-collection', 'Producing evidence for an auditor',
     'The screenshot, the export, the ticket. Evidence that is dated, attributable and reproducible next year.'),
    ('breach-notification-duty', 'Meeting a notification deadline',
     'Seventy-two hours, who decides, what goes in the letter, and what is said before the facts are complete.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'security-governance-practice') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Adversary emulation
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('mitre-attack-mapping', 'Mapping onto ATT&CK',
     'Naming what was done in a vocabulary both sides share, so that coverage can be argued about rather than felt.'),
    ('adversary-emulation-plan', 'Writing an emulation plan',
     'A sequence of techniques with a stated objective and a stop condition. Not a penetration test with a theme.'),
    ('detection-validation', 'Proving a detection works',
     'Running the technique the rule was written for and showing the alert. A rule nobody fired is a hypothesis.'),
    ('purple-exercise-facilitation', 'Facilitating a purple exercise',
     'Keeping attack and defence in the same room and the same timeline, and getting a written outcome out of both.'),
    ('breach-simulation-tooling', 'Driving simulation tooling',
     'Atomic Red Team, Caldera and the like: reproducible execution, and cleanup that actually cleans up.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'adversary-emulation') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What the offensive and reporting roots were missing
-- ═══════════════════════════════════════════════════════════════════
--
-- Both roots exist since 0057 and were written for a developer avoiding
-- defects rather than a red teamer producing findings. These are the nodes a
-- report is actually judged on.

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('ctf-methodology', 'Working a capture-the-flag challenge',
     'Enumerating before exploiting, keeping notes that survive the session, and knowing when to leave one alone.'),
    ('intercepting-proxy-workflow', 'Driving an intercepting proxy',
     'Burp or ZAP as a working environment rather than a button: scope, match-and-replace, repeater discipline.'),
    ('business-logic-abuse', 'Abusing logic rather than code',
     'The flow that is implemented correctly and can still be used to get something for nothing.'),
    ('race-condition-exploitation', 'Exploiting a race',
     'Finding the window, and hitting it reliably enough that somebody else can hit it too.'),
    ('web-privilege-escalation', 'Escalating privilege in a web application',
     'From the account you were given to the one you were not, through what the application trusts about itself.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'offensive-basics') p
  ON CONFLICT (slug) DO NOTHING;

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'security', p.id
  FROM (VALUES
    ('severity-argument', 'Arguing a severity',
     'Defending a score against somebody who has a reason to want it lower, using the vector rather than adjectives.'),
    ('embargo-discipline', 'Holding an embargo',
     'Not publishing while the clock runs, and knowing what may be said in the meantime and to whom.'),
    ('duplicate-attribution', 'Handling a duplicate honestly',
     'Two people found the same thing. Reading the timestamps, and crediting the second one without paying twice.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'reporting') p
  ON CONFLICT (slug) DO NOTHING;
