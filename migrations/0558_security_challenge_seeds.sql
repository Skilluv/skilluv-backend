-- The security practice catalogue: forty-six challenges across the five
-- trades.
--
-- ## Why they are drafts
--
-- The same reason 0185, 0219, 0417, 0512 and 0528 give, and it is sharper
-- here: a security brief that is vague about what is in scope is not a weak
-- brief, it is an instruction to do something unauthorised. The objective and
-- the intent come from the backlog; the exact scope line, the hint sequence
-- and the pass bar need an author who has done the thing. `draft` is the state
-- the workflow already has for that, and a curator publishes.
--
-- ## Why none of these is a `ctf_flag` or a `defensive_lab`
--
-- Both of those kinds are machine-checked, and 0549 explains that machine
-- checking requires the platform to own the secret. Every target seeded here
-- belongs to somebody else — Juice Shop, WebGoat, DVWA, PortSwigger, VulnHub,
-- HackTheBox, published forensic datasets — so there is no secret to hold, and
-- a flag hash invented by the author of this migration would produce a
-- challenge nobody can ever pass.
--
-- Tickets C-04 and B-04 asked for twenty flag challenges and ten hash-graded
-- labs. What is seeded instead is the same content, verified by a reviewer
-- reading a write-up. The machine-checked kinds are not unused: they are how a
-- range this platform hosts gets published, through the admin endpoint, by
-- somebody who plants the flag and knows the answers. Seeding them from here
-- would have meant guessing.
--
-- ## Licences and attribution
--
-- Every external target is linked, never rehosted, and
-- `security_attribution_md` carries who owns the material and under what
-- terms. That is not decoration: several of the forensic datasets are CC-BY,
-- which requires the attribution to travel with the use, and an attribution
-- that lives in a migration comment travels nowhere.
--
-- ## The paragraph every security brief ends on
--
-- Two things. Nothing is tested outside the target the brief names — a finding
-- against something else is refused however real it is. And a write-up here is
-- read by somebody who will try to follow it, so it says what was done
-- precisely enough for that to work. Both are in every brief rather than in a
-- charter nobody opens.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, ai_policy, evaluation_rubric,
     security_kind, security_difficulty_tier, security_external_source,
     security_external_url, security_writeup_required, security_attribution_md,
     duration_minutes)
SELECT
    c.title,
    c.objective,
    '## What there is to do' || E'\n\n' ||
    c.objective || E'.\n\n' ||
    '## Where' || E'\n\n' ||
    'On ' || c.target_label || ' — ' || c.external_url || E'.\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'In every case: nothing is tested outside the target named above. A ' ||
    'finding against anything else is refused however real it is, and that ' ||
    'is the line this whole trade rests on.' || E'\n\n' ||
    'And the write-up is read by somebody who will try to follow it. Steps, ' ||
    'requests and payloads precise enough that they get to the same place. ' ||
    '"I fuzzed it and it broke" is a story.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid for the family applies, and it is public: you can read ' ||
    'it before you submit.',
    'security', c.difficulty, NULL,
    'draft', TRUE, 'disclosure_required',
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'security' AND g.reviewer_group = c.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'security' AND g.reviewer_group IS NULL)
    ),
    c.kind, c.tier, c.source, c.external_url, TRUE, c.attribution,
    c.minutes
FROM (VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Juice Shop — twenty objectives (C-04)
-- ═══════════════════════════════════════════════════════════════════
--
-- Described by what has to be achieved rather than by Juice Shop's internal
-- challenge keys. The keys are an implementation detail of somebody else's
-- application and they change between releases; "log in as the administrator
-- without the password" does not.

('red-team', 'Juice Shop — log in as the administrator',
 'Authenticate as the administrator of the shop without knowing the password',
 'The request that worked, the response that proves you are the administrator, and one sentence on which check was missing rather than which payload you used.',
 2, 'easy', 15, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence. Not hosted by this platform: run it yourself with one container, or use a public instance.'),

('red-team', 'Juice Shop — find the score board',
 'Find the page the application does not link to',
 'How you found it, and what in the delivered application told you it existed. The answer is not "I guessed the URL".',
 1, 'easy', 10, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — make it leak a stack trace',
 'Provoke an unhandled error and read what the response gives away',
 'The input that caused it, the information disclosed, and what an attacker gets from that information specifically.',
 2, 'easy', 15, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — log in as another customer',
 'Authenticate as a named customer other than the administrator, by a route that is not their password',
 'Which account, how, and whether the same route works for every account or only that one.',
 2, 'easy', 20, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — change a password without the old one',
 'Reset or change an account password without supplying the current one',
 'The request, and the check that should have been there. This is the class of defect that ships most often in real applications.',
 2, 'easy', 20, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — reflected cross-site scripting',
 'Get script of your own to execute from a value the application reflects back',
 'The parameter, the payload, a screenshot of execution, and which encoding step was missing.',
 2, 'medium', 25, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — cross-site scripting in the page itself',
 'Get script to execute through a sink in the client-side code rather than through the server response',
 'The sink, the source, and why the server-side defence did not apply. Naming the sink is the whole exercise.',
 3, 'medium', 30, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — stored cross-site scripting',
 'Get script to persist and execute for another user of the application',
 'Where it is stored, who sees it, and the difference in impact from the reflected case.',
 3, 'medium', 30, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — act on behalf of a logged-in user',
 'Cause a state-changing request to be made by a user who did not intend it',
 'The proof of concept page, the request it produces, and why the application accepted it.',
 3, 'medium', 30, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — read a file you were not offered',
 'Retrieve a file from the server that no page links to',
 'The path, how you found it, and what the directory disclosed that it should not have.',
 3, 'medium', 25, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — read somebody else''s basket',
 'Access a resource belonging to another user by changing an identifier',
 'The request, the identifier, and the one line of authorisation logic that is missing. This defect class is the most common finding in real engagements.',
 2, 'medium', 20, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — undo the "encryption"',
 'Recover a plaintext from a value the application treats as protected',
 'What the value actually was, how you recovered it, and what should have been used instead.',
 3, 'medium', 25, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — forge a session token',
 'Get the application to accept a token you produced yourself',
 'The original token, the one you forged, the property of the verification that allowed it, and the fix.',
 4, 'medium', 40, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — buy something for the wrong price',
 'Complete a purchase at a price the application did not intend, without touching the database directly',
 'The sequence of requests. This is a business logic defect: no injection, no script, and it is the kind that costs real money.',
 3, 'medium', 35, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — get the list of every customer address',
 'Extract data from a table the application never intended to expose, through an interface it does expose',
 'The payload, the shape of the data returned, and how much of it you stopped at. Take the minimum that proves the finding.',
 4, 'hard', 45, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — upload something that should have been refused',
 'Get the application to accept a file its rules were meant to reject',
 'The file, the rule that was bypassed, and what an attacker would do next with that capability.',
 4, 'hard', 45, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — make the server fetch a URL for you',
 'Get the application to make a request to a destination of your choosing',
 'The parameter, the destination, what came back, and why this matters more on a cloud host than on a laptop.',
 4, 'hard', 40, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — run code on the server',
 'Achieve execution of code of your own on the application host',
 'The full chain, each step reproducible. Say explicitly what you did not do once you had it — nothing persistent, nothing destructive.',
 5, 'insane', 90, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — recover a password from its hash',
 'Recover one account password from the stored hash you have obtained',
 'The hash, the method, the time it took, and what storage choice would have made it infeasible.',
 5, 'insane', 120, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

('red-team', 'Juice Shop — chain three defects into one impact',
 'Combine at least three separate defects into a single attack with an impact none of them has alone',
 'The chain in order, each link reproducible, and the impact statement. Chaining is what separates a report from a list.',
 5, 'insane', 180, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-juice-shop/', 'OWASP Juice Shop',
 'OWASP Juice Shop, MIT licence.'),

-- ═══════════════════════════════════════════════════════════════════
-- The other ranges (T-11)
-- ═══════════════════════════════════════════════════════════════════

('code-audit', 'WebGoat — one defect class, end to end',
 'Work through one WebGoat lesson group and explain the defect class it teaches from the code up',
 'The lesson group, what the vulnerable code does, why the exploit works, and the corrected code. WebGoat ships the source: read it.',
 2, 'easy', 60, 'training_ground', 'owasp_project',
 'https://owasp.org/www-project-webgoat/', 'OWASP WebGoat',
 'OWASP WebGoat, GPL-2.0. Run it locally in a container.'),

('red-team', 'DVWA — the same defect at four difficulty levels',
 'Exploit one DVWA vulnerability at every security level it offers, and explain what changed each time',
 'Four working payloads and, more importantly, what defence was added at each level and why it was insufficient until the last one.',
 3, 'medium', 90, 'training_ground', 'owasp_project',
 'https://github.com/digininja/DVWA', 'DVWA',
 'Damn Vulnerable Web Application, GPL-3.0.'),

('red-team', 'PortSwigger Academy — one topic, all its labs',
 'Complete every lab in one PortSwigger Academy topic and write the topic up as if teaching it',
 'The labs completed, and a write-up that would let somebody who has not done them understand the defect class and recognise it in the wild.',
 3, 'medium', 240, 'training_ground', 'own_instance',
 'https://portswigger.net/web-security', 'PortSwigger Web Security Academy',
 'Labs hosted by PortSwigger and free to use with an account. Their terms apply.'),

('code-audit', 'Audit this platform''s authentication',
 'Read the authentication and session code of the Skilluv backend and report what you find',
 'Findings with file and line, the path traced from request to sink, and a proposed fix for each. Findings against the deployed service go through the disclosure programme, not here — this exercise is about reading the code.',
 4, 'hard', 180, 'audit_exercise', 'own_instance',
 'https://github.com/skilluv/skilluv-backend', 'the Skilluv backend',
 'This platform''s own source, AGPL-3.0. Reading it is invited; testing the deployed service needs the published scope.'),

('code-audit', 'Audit this platform''s authorisation',
 'Read the capability and admin-gate code and report where an authorisation check is missing or wrong',
 'Every route you checked, not only the ones that were wrong. An audit that lists three findings and no coverage tells a reader nothing about how carefully it was done.',
 4, 'hard', 180, 'audit_exercise', 'own_instance',
 'https://github.com/skilluv/skilluv-backend', 'the Skilluv backend',
 'This platform''s own source, AGPL-3.0.'),

('code-audit', 'Audit this platform''s file handling',
 'Read the upload, storage and signed-URL code and report what an attacker could do with it',
 'The path from an uploaded byte to a served byte, every validation on it, and what is missing. Include the storage configuration in scope.',
 4, 'hard', 150, 'audit_exercise', 'own_instance',
 'https://github.com/skilluv/skilluv-backend', 'the Skilluv backend',
 'This platform''s own source, AGPL-3.0.'),

('governance', 'Audit this platform''s privacy documentation',
 'Read PRIVACY.md against the GDPR and report what is missing, wrong or unevidenced',
 'Article by article for the ones that apply: lawful basis, retention, subject rights, processors. Say what evidence would be needed for each claim and whether it exists.',
 3, 'medium', 120, 'audit_exercise', 'own_instance',
 'https://github.com/skilluv/skilluv-backend', 'this platform''s published documents',
 'This platform''s own documents.'),

('governance', 'Audit this platform''s threat model',
 'Read THREAT_MODEL.md and report what it does not consider',
 'Threats it misses, mitigations it claims that the code does not implement, and a ranking of what to fix first given what this platform actually holds.',
 4, 'hard', 150, 'audit_exercise', 'own_instance',
 'https://github.com/skilluv/skilluv-backend', 'this platform''s published documents',
 'This platform''s own documents.'),

-- ═══════════════════════════════════════════════════════════════════
-- Machines (T-12)
-- ═══════════════════════════════════════════════════════════════════
--
-- By tier rather than by name. A list of fifty specific machines is curation
-- work that has to be done by somebody who has run them — and machine names
-- and links change, retire and disappear, which is exactly the content a
-- migration should not be asserting. What is seeded is the shape: pick one at
-- this tier from the index, and write it up to a standard.

('red-team', 'VulnHub — an easy machine, written up',
 'Complete one machine rated easy from the VulnHub index and write it up',
 'Enumeration, the way in, escalation to root, and the two things you tried that did not work. A write-up with no dead ends in it is a write-up that has been tidied into uselessness.',
 2, 'easy', 180, 'machine_walkthrough', 'vulnhub',
 'https://www.vulnhub.com/', 'VulnHub',
 'Images published by their authors on VulnHub; each carries its own terms. Run locally, never on a network you share.'),

('red-team', 'VulnHub — a medium machine, written up',
 'Complete one machine rated medium and write it up',
 'As above, and one paragraph on the single insight that unlocked it. There is almost always exactly one.',
 3, 'medium', 300, 'machine_walkthrough', 'vulnhub',
 'https://www.vulnhub.com/', 'VulnHub',
 'Images published by their authors on VulnHub.'),

('red-team', 'HackTheBox — a retired easy machine, written up',
 'Complete one retired machine rated easy and write it up in your own words',
 'Your own write-up. Official and community walkthroughs exist for every retired machine, and a submission that reproduces one is refused — say which you read and when, and what you had done before reading it.',
 2, 'easy', 180, 'machine_walkthrough', 'hackthebox_retired',
 'https://app.hackthebox.com/machines/list/retired', 'HackTheBox retired machines',
 'Machines hosted by HackTheBox; a subscription is needed for retired ones. Their terms apply.'),

('red-team', 'HackTheBox — a retired medium machine, written up',
 'Complete one retired machine rated medium and write it up in your own words',
 'As above. The honesty about what you read and when is the part being assessed as much as the machine.',
 3, 'medium', 300, 'machine_walkthrough', 'hackthebox_retired',
 'https://app.hackthebox.com/machines/list/retired', 'HackTheBox retired machines',
 'Machines hosted by HackTheBox; a subscription is needed for retired ones.'),

('red-team', 'HackTheBox — a retired hard machine, written up',
 'Complete one retired machine rated hard and write it up in your own words',
 'The full chain, and an honest account of how long it took and where you were stuck. Hard machines are where write-ups start being worth reading.',
 4, 'hard', 600, 'machine_walkthrough', 'hackthebox_retired',
 'https://app.hackthebox.com/machines/list/retired', 'HackTheBox retired machines',
 'Machines hosted by HackTheBox.'),

('blue-team', 'TryHackMe — a defensive path, written up',
 'Complete one defensive learning path and write up what you can now do that you could not before',
 'The rooms, and one worked example of your own: take a log set or a capture from elsewhere and apply what the path taught. A path completed and never applied is a certificate.',
 3, 'medium', 480, 'machine_walkthrough', 'tryhackme',
 'https://tryhackme.com/', 'TryHackMe',
 'Rooms hosted by TryHackMe; substantial free tier. Their terms apply.'),

-- ═══════════════════════════════════════════════════════════════════
-- Analysis exercises (B-04)
-- ═══════════════════════════════════════════════════════════════════
--
-- Datasets published by other people, linked and not rehosted, which is both
-- the licence position and the honest one: these archives are maintained,
-- corrected and versioned by their authors.

('blue-team', 'A brute-force attempt in a web server log',
 'Take one public web server log set and identify a credential brute-force attempt in it',
 'The source address, the window, the request pattern, how many attempts, and whether any succeeded. Then the detection rule you would write, and what ordinary traffic it would have fired on.',
 2, 'easy', 60, 'analysis_exercise', 'public_dataset',
 'https://www.secrepo.com/', 'the SecRepo public dataset collection',
 'Samples collected and published at secrepo.com; each links its original source and licence.'),

('blue-team', 'A password in cleartext on the wire',
 'Take one public HTTP capture and extract a credential transmitted in the clear',
 'The packet, the reassembled stream, the credential redacted in your write-up, and the two changes that would have prevented it.',
 2, 'easy', 45, 'analysis_exercise', 'public_dataset',
 'https://www.malware-traffic-analysis.net/training-exercises.html',
 'the Malware Traffic Analysis training exercises',
 'Captures published by Brad Duncan at malware-traffic-analysis.net. Archives are password-protected against scanners; the password is on the site.'),

('blue-team', 'Which account was compromised, and when',
 'Take one public authentication log set and establish which account was compromised and at what time',
 'The timeline with each event sourced, the distinction between "authenticated from an unusual address" and "was compromised", and what would have detected it sooner.',
 2, 'easy', 60, 'analysis_exercise', 'public_dataset',
 'https://www.secrepo.com/', 'the SecRepo public dataset collection',
 'Samples collected and published at secrepo.com.'),

('blue-team', 'Data leaving over DNS',
 'Take one public capture containing DNS tunnelling and establish what was exfiltrated and how much',
 'The indicators, the volume, the reconstruction of at least part of the payload, and a detection that distinguishes this from a busy resolver.',
 3, 'medium', 120, 'analysis_exercise', 'public_dataset',
 'https://www.malware-traffic-analysis.net/training-exercises.html',
 'the Malware Traffic Analysis training exercises',
 'Captures published at malware-traffic-analysis.net.'),

('blue-team', 'Credential theft in a memory image',
 'Take one public memory image and establish whether a credential-dumping tool ran on it',
 'The Volatility output that shows it, what else was running, and what the same evidence would look like if the tool had been renamed.',
 4, 'hard', 180, 'analysis_exercise', 'public_dataset',
 'https://github.com/volatilityfoundation/volatility3',
 'the public memory samples referenced by the Volatility project',
 'Images published by their original authors; the Volatility documentation links each with its terms.'),

('blue-team', 'A web shell in a server log',
 'Take one public web server log set containing web shell activity and reconstruct what the operator did',
 'The initial upload, the requests that followed, the commands you can infer and the ones you cannot, and the detection for the pattern rather than for the filename.',
 3, 'medium', 120, 'analysis_exercise', 'public_dataset',
 'https://www.secrepo.com/', 'the SecRepo public dataset collection',
 'Samples collected and published at secrepo.com.'),

('blue-team', 'Triage a day of intrusion detection alerts',
 'Take one public capture, run an intrusion detection system over it offline, and triage the alerts',
 'The ranked list with a reason for each ranking, the false positives named as such, and the one alert you would have woken somebody up for.',
 3, 'medium', 150, 'analysis_exercise', 'public_dataset',
 'https://www.malware-traffic-analysis.net/training-exercises.html',
 'the Malware Traffic Analysis training exercises',
 'Captures published at malware-traffic-analysis.net.'),

('blue-team', 'A command-and-control channel hiding in normal traffic',
 'Take one public capture containing command-and-control traffic over an encrypted or common protocol and characterise the channel',
 'The beaconing evidence, the interval and jitter, what is and is not visible without decryption, and a detection that does not depend on breaking it.',
 4, 'hard', 180, 'analysis_exercise', 'public_dataset',
 'https://www.malware-traffic-analysis.net/training-exercises.html',
 'the Malware Traffic Analysis training exercises',
 'Captures published at malware-traffic-analysis.net.'),

('blue-team', 'Triage a suspicious file without running it',
 'Take one publicly published sample and triage it statically, then dynamically in a sandbox',
 'What it is, what it contacts, what it changes, the indicators you would share, and the point at which you would hand it to a specialist. Nothing is executed outside an isolated environment, and the write-up says which one.',
 4, 'hard', 180, 'analysis_exercise', 'public_dataset',
 'https://www.malware-traffic-analysis.net/training-exercises.html',
 'samples published with the Malware Traffic Analysis exercises',
 'Samples published at malware-traffic-analysis.net for training. Handle in an isolated environment; do not redistribute.'),

('blue-team', 'One incident, three sources, one timeline',
 'Take a public dataset that includes more than one artefact type and correlate them into a single incident timeline',
 'One timeline, every entry sourced, the gaps named, and a short executive summary a non-technical reader could act on. Correlation across sources is the skill this exercise exists for.',
 5, 'insane', 360, 'analysis_exercise', 'public_dataset',
 'https://thedfirreport.com/', 'the published incident reports of The DFIR Report',
 'Reports and, where offered, artefacts published by The DFIR Report. Read their terms; several reports link samples hosted elsewhere.'),

-- ═══════════════════════════════════════════════════════════════════
-- Purple (P-04 practice, outside a live session)
-- ═══════════════════════════════════════════════════════════════════

('purple-team', 'Run a technique, then prove you can see it',
 'Execute one ATT&CK technique in an environment of your own and build the detection that catches it',
 'The technique identifier, how you executed it, the telemetry it produced, the rule, and the evidence of both halves: the rule firing on the technique and staying quiet on a period of ordinary activity.',
 3, 'medium', 180, 'analysis_exercise', 'public_dataset',
 'https://github.com/redcanaryco/atomic-red-team', 'Atomic Red Team',
 'Tests published by Red Canary, MIT licence. Every test says what it changes: run them somewhere you are allowed to break.'),

('purple-team', 'Five techniques, and an honest coverage statement',
 'Execute five techniques from one ATT&CK tactic and report what your detection actually covers',
 'Five techniques, five results, and a coverage statement that says what you cannot see — which is the half of this work that gets skipped and the half a defender needs.',
 4, 'hard', 300, 'analysis_exercise', 'public_dataset',
 'https://github.com/redcanaryco/atomic-red-team', 'Atomic Red Team',
 'Tests published by Red Canary, MIT licence.')

  ) AS c(reviewer_group, title, objective, expected, difficulty, tier, minutes,
         kind, source, external_url, target_label, attribution);
