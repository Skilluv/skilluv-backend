-- Which skills each security trade is made of.
--
-- ## Why the overlap is deliberate
--
-- `web-app-security` and its children appear under three of the five trades.
-- That is not sloppiness: a red teamer, a code auditor and a purple teamer all
-- have to know what an IDOR is, and what separates them is what they do next.
-- The distinction is carried by the nodes that are *not* shared —
-- `injection-path-tracing` is code-audit only, `intercepting-proxy-workflow`
-- is red-team only — and by `is_core`, which is what a recommendation reads.
--
-- ## Where a trade points outside the domain
--
-- Four places, and each one is a claim worth being able to argue with:
--
--   * red team and purple team reach into `ops` for
--     `container-security-basics`, because the target is infrastructure at
--     least as often as it is a form field;
--   * code audit reaches into `quality` for `sast-rule-authoring` and
--     `false-positive-triage` (0454), which are the pipeline half of what a
--     code auditor does by hand;
--   * blue team reaches into `ops` for `incident-response`, which is the
--     shared competence 0543 explains is not duplicated here;
--   * governance reaches into `leadership` for `written-decisions` and
--     `risk-registers`, because a policy nobody agreed to and a risk nobody
--     wrote down are both just documents.
--
-- ## The guard at the end
--
-- Same reason 0519 gives: a JOIN drops an unknown slug silently, and a map
-- short of three edges reads exactly like a map that was written that way.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
SELECT o.id, s.id, v.is_core, TRUE, v.weight
  FROM (VALUES

-- ── security-red-team ──────────────────────────────────────────────
('security-red-team', 'offensive-basics',              TRUE,  1.0),
('security-red-team', 'web-app-security',              TRUE,  1.0),
('security-red-team', 'intercepting-proxy-workflow',   TRUE,  1.0),
('security-red-team', 'ctf-methodology',               TRUE,  1.0),
('security-red-team', 'recon-passive',                 TRUE,  0.9),
('security-red-team', 'recon-active',                  TRUE,  0.9),
('security-red-team', 'sql-injection-detection',       TRUE,  1.0),
('security-red-team', 'xss-stored-reflected-dom',      TRUE,  1.0),
('security-red-team', 'idor-detection',                TRUE,  1.0),
('security-red-team', 'authz-vs-authn',                TRUE,  1.0),
('security-red-team', 'web-privilege-escalation',      TRUE,  1.0),
('security-red-team', 'writeup-quality',               TRUE,  1.0),
('security-red-team', 'business-logic-abuse',          FALSE, 0.9),
('security-red-team', 'race-condition-exploitation',   FALSE, 0.8),
('security-red-team', 'ssrf-detection',                FALSE, 0.9),
('security-red-team', 'file-upload-security',          FALSE, 0.8),
('security-red-team', 'jwt-security-pitfalls',         FALSE, 0.8),
('security-red-team', 'exploit-crafting',              FALSE, 0.8),
('security-red-team', 'post-exploitation-basics',      FALSE, 0.7),
('security-red-team', 'sandboxed-lab-practice',        FALSE, 0.7),
('security-red-team', 'cvss-scoring',                  FALSE, 0.9),
('security-red-team', 'severity-argument',             FALSE, 0.8),
('security-red-team', 'responsible-disclosure',        FALSE, 0.9),
('security-red-team', 'embargo-discipline',            FALSE, 0.8),
-- Outside the domain: the target is infrastructure as often as it is a form.
('security-red-team', 'container-security-basics',     FALSE, 0.7),

-- ── security-blue-team ─────────────────────────────────────────────
('security-blue-team', 'defensive-operations',         TRUE,  1.0),
('security-blue-team', 'log-triage',                   TRUE,  1.0),
('security-blue-team', 'detection-rule-authoring',     TRUE,  1.0),
('security-blue-team', 'detection-tuning',             TRUE,  1.0),
('security-blue-team', 'threat-hunting',               TRUE,  1.0),
('security-blue-team', 'ioc-extraction',               TRUE,  0.9),
('security-blue-team', 'network-traffic-analysis',     TRUE,  1.0),
('security-blue-team', 'malware-triage',               TRUE,  0.9),
('security-blue-team', 'incident-response-command-security', TRUE, 1.0),
('security-blue-team', 'memory-forensics',             FALSE, 0.9),
('security-blue-team', 'disk-timeline-forensics',      FALSE, 0.9),
('security-blue-team', 'session-hijacking-defense',    FALSE, 0.7),
('security-blue-team', 'security-headers',             FALSE, 0.6),
('security-blue-team', 'breach-notification-duty',     FALSE, 0.8),
('security-blue-team', 'mitre-attack-mapping',         FALSE, 0.9),
('security-blue-team', 'writeup-quality',              FALSE, 0.9),
-- Outside the domain: running the incident is the same competence whoever
-- caused it. 0543 explains why the node is not duplicated here.
('security-blue-team', 'incident-response',            FALSE, 0.9),

-- ── security-code-audit ────────────────────────────────────────────
('security-code-audit', 'secure-code-audit',           TRUE,  1.0),
('security-code-audit', 'injection-path-tracing',      TRUE,  1.0),
('security-code-audit', 'authorization-logic-review',  TRUE,  1.0),
('security-code-audit', 'threat-modeling-stride',      TRUE,  1.0),
('security-code-audit', 'crypto-misuse-review',        TRUE,  1.0),
('security-code-audit', 'deserialization-review',      TRUE,  0.9),
('security-code-audit', 'dependency-supply-chain-audit', TRUE, 1.0),
('security-code-audit', 'sast-finding-triage',         TRUE,  1.0),
('security-code-audit', 'scan-result-to-finding',      TRUE,  1.0),
('security-code-audit', 'web-app-security',            TRUE,  1.0),
('security-code-audit', 'owasp-top10-2025',            FALSE, 0.9),
('security-code-audit', 'secrets-in-history',          FALSE, 0.9),
('security-code-audit', 'cryptography-basics',         FALSE, 0.9),
('security-code-audit', 'hmac-verification',           FALSE, 0.7),
('security-code-audit', 'password-storage',            FALSE, 0.8),
('security-code-audit', 'dependency-scanning',         FALSE, 0.9),
('security-code-audit', 'file-upload-security',        FALSE, 0.8),
('security-code-audit', 'llm-security',                FALSE, 0.6),
('security-code-audit', 'writeup-quality',             FALSE, 0.9),
-- Outside the domain: the pipeline half of the same reading (0454).
('security-code-audit', 'sast-rule-authoring',         FALSE, 0.8),
('security-code-audit', 'false-positive-triage',       FALSE, 0.8),

-- ── security-governance ────────────────────────────────────────────
('security-governance', 'security-governance-practice', TRUE, 1.0),
('security-governance', 'gdpr-compliance',             TRUE,  1.0),
('security-governance', 'risk-assessment-method',      TRUE,  1.0),
('security-governance', 'security-policy-authoring',   TRUE,  1.0),
('security-governance', 'audit-evidence-collection',   TRUE,  1.0),
('security-governance', 'iso-27001-isms',              TRUE,  0.9),
('security-governance', 'dpia-authoring',              TRUE,  0.9),
('security-governance', 'breach-notification-duty',    TRUE,  1.0),
('security-governance', 'third-party-risk-review',     TRUE,  0.9),
('security-governance', 'soc2-readiness',              FALSE, 0.8),
('security-governance', 'threat-modeling-stride',      FALSE, 0.8),
('security-governance', 'secrets-management',          FALSE, 0.7),
('security-governance', 'responsible-disclosure',      FALSE, 0.8),
-- Outside the domain: a policy nobody agreed to is a document.
('security-governance', 'written-decisions',            FALSE, 0.8),
('security-governance', 'risk-registers',              FALSE, 0.8),

-- ── security-purple-team ───────────────────────────────────────────
('security-purple-team', 'adversary-emulation',        TRUE,  1.0),
('security-purple-team', 'mitre-attack-mapping',       TRUE,  1.0),
('security-purple-team', 'adversary-emulation-plan',   TRUE,  1.0),
('security-purple-team', 'detection-validation',       TRUE,  1.0),
('security-purple-team', 'purple-exercise-facilitation', TRUE, 1.0),
('security-purple-team', 'detection-rule-authoring',   TRUE,  1.0),
('security-purple-team', 'breach-simulation-tooling',  TRUE,  0.9),
('security-purple-team', 'log-triage',                 TRUE,  0.9),
('security-purple-team', 'offensive-basics',           FALSE, 0.9),
('security-purple-team', 'post-exploitation-basics',   FALSE, 0.9),
('security-purple-team', 'threat-hunting',             FALSE, 0.9),
('security-purple-team', 'ioc-extraction',             FALSE, 0.8),
('security-purple-team', 'detection-tuning',           FALSE, 0.9),
('security-purple-team', 'web-app-security',           FALSE, 0.8),
('security-purple-team', 'writeup-quality',            FALSE, 0.8),
-- Outside the domain: emulation lands on infrastructure.
('security-purple-team', 'container-security-basics',  FALSE, 0.8)

  ) AS v(orientation_slug, skill_slug, is_core, weight)
  JOIN orientations o ON o.slug = v.orientation_slug
  JOIN skill_nodes  s ON s.slug = v.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;

-- Every slug above has to exist. The JOIN would otherwise drop an unknown one
-- silently, and a skill map short of three edges reads exactly like a skill
-- map that was written that way.
DO $$
DECLARE
    expected INT := 94;
    actual   INT;
BEGIN
    SELECT count(*) INTO actual
      FROM orientation_skill_map m
      JOIN orientations o ON o.id = m.orientation_id
     WHERE o.slug LIKE 'security-%' AND o.slug <> 'security-engineer';

    IF actual <> expected THEN
        RAISE EXCEPTION
            'security skill map: expected % edges, got % — a slug above does not exist',
            expected, actual;
    END IF;
END $$;
