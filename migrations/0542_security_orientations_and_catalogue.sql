-- The security domain opens, with five trades instead of one.
--
-- ## What was there
--
-- One orientation: `security-engineer`, written by migration 0088 alongside
-- the twenty-two others, with `reviewer_group` left null. A person who
-- attacks web applications for a living, a person who reads logs at three in
-- the morning, and a person who writes a records-retention policy all had to
-- pick it, and a reviewer had no family to be granted rights on — so
-- `security_reviewer:all` was the only review capability the domain had, and
-- nobody could hold anything narrower.
--
-- That is the failure the five rows below end. The domain was declared active
-- by 0400 and had no catalogue behind it, which is the state 0400's own
-- comment says a domain should not be offered in.
--
-- ## Why five and not three
--
-- The split is by what a reviewer has to be able to do, which is the line
-- 0176 settled and 0517 restated. Five families, and each one is a different
-- reading:
--
--   * `red-team` — judged on whether the exploit actually works. A report is
--     read by somebody who will try to reproduce it, and the question is
--     binary in a way it is in no other domain here.
--   * `blue-team` — judged on whether the conclusion follows from the
--     artefact. There is no green build: a detection that fires on the sample
--     and on nothing else is the proof, and reading that requires having
--     tuned one.
--   * `code-audit` — read as code, not run. The reviewer needs to be able to
--     tell a real injection path from a scanner hit, which is a code-reading
--     skill and not an offensive one.
--   * `governance` — read as documents against a framework. A person who can
--     find an IDOR is not thereby able to say whether a DPIA covers its
--     purpose, and the reverse is just as true.
--   * `purple-team` — judged on whether the exercise produced a detection
--     that did not exist before. Needs somebody who has sat on both sides,
--     which is why it is not the union of the first two.
--
-- Splitting further would have meant splitting by tool or by target — mobile,
-- cloud, ICS — which describes an engagement rather than a craft. The same
-- person does web and cloud in the same year with the same reading.
--
-- ## The four that were there are archived rather than kept
--
-- The domain had four orientations, all with `reviewer_group` null:
--
--   * `security-engineer` — "threat modeling, secure code review, app sec by
--     design", which is `security-code-audit`;
--   * `pentester-web` — which is `security-red-team` with one target named;
--   * `pentester-mobile` — the same, with a different target named;
--   * `soc-analyst` — "SIEM, threat detection, incident response, forensics",
--     which is `security-blue-team`.
--
-- All four are archived and pointed at what replaces them. Keeping any of them
-- would have meant two ways to say one thing and two answers to how many
-- people in the domain do it — and `pentester-web` alongside
-- `security-red-team` is the worst case, because a person choosing would have
-- no way to tell which one a reviewer reads.
--
-- Archiving is the mechanism 0089 built for exactly this: the people who
-- already chose one keep the row in their history, `replaced_by` says where it
-- went, and nobody chooses it again.
--
-- ## Why `pentester-mobile` does not survive as a trade
--
-- Because it names a target rather than a craft. The same argument as above
-- about not splitting by tool or platform: the person who tests a mobile
-- application this quarter tests its API next quarter, with the same reading.
-- Mobile is a `tags` entry on `security-red-team`, which is what tags are for.
--
-- ## The slugs keep their `security-` prefix
--
-- Against the convention of every other domain, where the domain is not
-- repeated in the orientation slug. It is kept because the domain already had
-- `security-engineer` in it, because the five names are ordinary English
-- words that would collide with the other domains' vocabulary if bare
-- (`governance` is a leadership resource category, `code-audit` reads as a
-- code orientation), and because `red-team` and `blue-team` mean nothing
-- outside this domain while `security-red-team` is unambiguous anywhere it is
-- quoted.

-- ═══════════════════════════════════════════════════════════════════
-- The five trades
-- ═══════════════════════════════════════════════════════════════════
--
-- The reviewer capabilities `security_reviewer:{family}` are derived from
-- these rows by the trigger of 0404. Nothing below writes them.

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated, reviewer_group)
VALUES

('security-red-team', 'Red team',
 'Attacking a system you have written permission to attack, and proving it. '
 'Web and API intrusion, exploit development, capture-the-flag, bug bounty. '
 'The trade is not finding something surprising: it is producing a report '
 'somebody else can follow to the same result.',
 'security', ARRAY['code', 'ops'],
 ARRAY['pentest', 'exploitation', 'ctf', 'bug-bounty', 'offensive',
       'web', 'mobile', 'api'],
 TRUE, 'red-team'),

('security-blue-team', 'Blue team',
 'Noticing, and being able to say how you noticed. Log and network analysis, '
 'detection engineering, incident response, malware triage, forensics. There '
 'is no passing build in this trade — the proof is a detection that fires on '
 'the real thing and stays quiet on everything else.',
 'security', ARRAY['ops', 'ai'],
 ARRAY['soc', 'detection', 'incident-response', 'forensics', 'defensive'],
 TRUE, 'blue-team'),

('security-code-audit', 'Code security',
 'Reading code for the defects that get exploited: injection paths, broken '
 'authorisation, misused cryptography, unsafe deserialisation, dependencies '
 'nobody looked at. Judged on the findings a scanner did not produce.',
 'security', ARRAY['code'],
 ARRAY['secure-code-review', 'sast', 'threat-modeling', 'cryptography', 'supply-chain'],
 TRUE, 'code-audit'),

('security-governance', 'Security governance',
 'Writing down what an organisation actually does about risk, and then being '
 'audited on it. GDPR, ISO 27001, SOC 2, policies, third-party risk, '
 'evidence. The artefact is a document, and the test is whether an auditor '
 'accepts it.',
 'security', ARRAY['leadership'],
 ARRAY['gdpr', 'iso-27001', 'soc2', 'risk', 'compliance', 'policy'],
 TRUE, 'governance'),

('security-purple-team', 'Purple team',
 'Running the attack and the defence as one exercise, so that each one '
 'produces something for the other. Adversary emulation, ATT&CK mapping, '
 'detection built and then tested against the technique it was built for. '
 'The output is a detection that did not exist that morning.',
 'security', ARRAY['ops', 'code'],
 ARRAY['adversary-emulation', 'mitre-attack', 'detection-engineering', 'exercise'],
 TRUE, 'purple-team');

-- ═══════════════════════════════════════════════════════════════════
-- The one it replaces
-- ═══════════════════════════════════════════════════════════════════

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'security-code-audit'),
       updated_at = NOW()
 WHERE slug = 'security-engineer';

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'security-red-team'),
       updated_at = NOW()
 WHERE slug IN ('pentester-web', 'pentester-mobile');

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'security-blue-team'),
       updated_at = NOW()
 WHERE slug = 'soc-analyst';

-- Every security orientation now either carries a review family or is
-- archived. A curated orientation with a null `reviewer_group` is one nobody
-- can be granted review rights for — visible rather than silently open, as the
-- column's own comment says — and this domain had four of them.
DO $$
DECLARE
    orphans INT;
BEGIN
    SELECT count(*) INTO orphans
      FROM orientations
     WHERE primary_domain = 'security'
       AND is_curated
       AND NOT is_archived
       AND reviewer_group IS NULL;

    IF orphans > 0 THEN
        RAISE EXCEPTION
            '% live security orientation(s) have no review family', orphans;
    END IF;
END $$;
