-- Where a security person practises, and what they practise with.
--
-- ## Two tables, and the line between them
--
-- Migration 0533 drew it and this domain is the case it was drawn for. A
-- terrain is an upstream project that wants contributions; a toolkit entry is
-- something you use or something you attack. Juice Shop is both, and the
-- distinction is what you are doing there: breaking it is practice and belongs
-- in the toolkit, adding a challenge to it is a contribution and belongs in
-- terrains. It appears in both, which is correct and was not possible to say
-- before the two tables existed.
--
-- ## Six entries are tagged rather than moved or copied
--
-- Juice Shop, DVWA, ZAP, Semgrep, CodeQL and Trivy already sit in
-- `external_resources` under `quality`, put there by 0459 and deliberately
-- left there by 0533. They stay. What changes is `orientation_slugs`, which
-- gains the security trades — so that the security toolkit finds them without
-- a second row existing for the same tool.
--
-- That works because the toolkit listing was widened alongside this migration:
-- it now returns a resource whose domain matches *or* whose orientations
-- belong to the domain asked for. A tool that serves two trades in two domains
-- was previously visible from one of them, arbitrarily, and nothing said which.
--
-- ## What is deliberately not here
--
-- No commercial platform is listed as a requirement. Burp Suite is listed in
-- its free edition with what that edition actually cannot do, because a
-- toolkit that quietly assumes a 400 EUR licence is a toolkit for people who
-- already have a job.
--
-- No exploit collections, no malware repositories, no credential dumps. The
-- line is: material built for practice, or a tool. Not somebody else's stolen
-- data, whatever its educational value.

-- ═══════════════════════════════════════════════════════════════════
-- Categories
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resource_categories (slug, skill_domain, name, description, sort_order) VALUES
('attack_range', 'security', 'Ranges and vulnerable targets',
 'Applications and machines built to be broken into, so that practising does '
 'not require anybody''s permission. The first thing a beginner needs and the '
 'only place they should be working for the first few months.', 610),
('offensive_tooling', 'security', 'Offensive tooling',
 'Proxies, scanners, fuzzers, exploitation frameworks. The access note says '
 'what the free edition still does, which for the main proxy is most of it.', 620),
('defensive_tooling', 'security', 'Defensive tooling',
 'Detection, log pipelines, intrusion detection, endpoint. Almost all of it is '
 'free to run and expensive to run at scale, and the note says where that '
 'line falls.', 630),
('forensics_tooling', 'security', 'Forensics and analysis',
 'Memory, disk, network, malware. The tools that read what is left behind.', 640),
('purple_tooling', 'security', 'Emulation and validation',
 'Running a known technique on purpose, and proving the detection for it '
 'fires.', 650),
('governance_tooling', 'security', 'Governance and compliance',
 'Frameworks, control catalogues, policy-as-code. Mostly documents, and the '
 'documents are the work.', 660),
('bounty_platform', 'security', 'Bounty and disclosure platforms',
 'Where somebody takes what they have learned and gets paid or credited for '
 'it. Listed with what it takes to be accepted, because the first rejection '
 'discourages more people than the difficulty does.', 670),
('threat_intel', 'security', 'Threat intelligence and taxonomies',
 'The shared vocabulary — techniques, weaknesses, advisories — without which '
 'two teams cannot compare notes.', 680);

-- ═══════════════════════════════════════════════════════════════════
-- The six that already exist get their security trades
-- ═══════════════════════════════════════════════════════════════════
--
-- Juice Shop and DVWA are ranges; ZAP, Semgrep, CodeQL and Trivy were filed
-- by 0459 under quality's `security_scanner` category, which is exactly right
-- for a testing pipeline and is also where a code auditor goes looking. None
-- of the six is duplicated: a second `semgrep` row would mean two entries for
-- one tool, drifting apart at the first correction.

UPDATE external_resources
   SET orientation_slugs = orientation_slugs
       || ARRAY['security-red-team', 'security-code-audit'],
       updated_at = NOW()
 WHERE slug IN ('owasp-juice-shop', 'dvwa', 'owasp-zap');

UPDATE external_resources
   SET orientation_slugs = orientation_slugs || ARRAY['security-code-audit'],
       updated_at = NOW()
 WHERE slug IN ('semgrep', 'codeql', 'trivy');

-- ═══════════════════════════════════════════════════════════════════
-- Ranges
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('owasp-webgoat', 'OWASP WebGoat', 'attack_range', 'security',
     'https://owasp.org/www-project-webgoat/',
     'A teaching application rather than a target: each lesson isolates one '
     'defect class, explains it, and then asks you to exploit it. The gentlest '
     'first step there is.',
     'Free, open source. One container, no account.',
     ARRAY['security-red-team', 'security-code-audit'], 10),

    ('portswigger-web-security-academy', 'PortSwigger Web Security Academy',
     'attack_range', 'security',
     'https://portswigger.net/web-security',
     'The reference free curriculum for web application security: every topic '
     'has an explanation, a set of labs on hosted instances, and a solution. '
     'Nothing else free is this thorough.',
     'Free, including the labs. An account is required and no card is asked '
     'for.',
     ARRAY['security-red-team', 'security-code-audit'], 20),

    ('tryhackme', 'TryHackMe', 'attack_range', 'security',
     'https://tryhackme.com/',
     'Guided rooms with hints, for both offence and defence. The place most '
     'people should start if a bare machine with no instructions is '
     'discouraging.',
     'Freemium. A substantial free tier; the newest rooms and unlimited '
     'machine time are paid.',
     ARRAY['security-red-team', 'security-blue-team'], 30),

    ('hackthebox', 'Hack The Box', 'attack_range', 'security',
     'https://www.hackthebox.com/',
     'Machines with no instructions, which is the point: enumerate, find the '
     'way in, escalate. Retired machines have community write-ups, which makes '
     'them the ones to learn on.',
     'Freemium. Active machines are free with queueing; retired machines and '
     'their official write-ups need a subscription.',
     ARRAY['security-red-team'], 40),

    ('vulnhub', 'VulnHub', 'attack_range', 'security',
     'https://www.vulnhub.com/',
     'Downloadable vulnerable virtual machines, hundreds of them, with '
     'community walkthroughs. Runs entirely offline, which matters where '
     'bandwidth is the constraint.',
     'Free. Needs a hypervisor and a few gigabytes per image.',
     ARRAY['security-red-team'], 50),

    ('cyberdefenders', 'CyberDefenders', 'attack_range', 'security',
     'https://cyberdefenders.org/',
     'Blue-team labs: a real artefact — capture, log set, memory image — and '
     'questions to answer from it. The defensive equivalent of a machine.',
     'Freemium. A good number of labs are free with an account.',
     ARRAY['security-blue-team', 'security-purple-team'], 60),

    ('malware-traffic-analysis', 'Malware Traffic Analysis', 'attack_range', 'security',
     'https://www.malware-traffic-analysis.net/',
     'Years of real packet captures with exercises and answers, published by '
     'one analyst. The best free source of traffic that actually looks like an '
     'infection.',
     'Free. Archives are password-protected against automated scanners; the '
     'password is on the site.',
     ARRAY['security-blue-team'], 70);

-- ═══════════════════════════════════════════════════════════════════
-- Offensive tooling
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('burp-suite-community', 'Burp Suite Community', 'offensive_tooling', 'security',
     'https://portswigger.net/burp/communitydownload',
     'The intercepting proxy this trade is built around. Community edition '
     'does interception, repeater and manual testing — which is the work.',
     'Free. No active scanner, and the intruder is rate-limited to the point '
     'of being a demonstration. Neither is needed to learn.',
     ARRAY['security-red-team'], 10),


    ('nmap', 'Nmap', 'offensive_tooling', 'security',
     'https://nmap.org/',
     'Host and service discovery, and the scripting engine that turns it into '
     'a first-pass vulnerability check.',
     'Free, open source. Scanning anything you do not have permission to scan '
     'is the line this whole domain is about.',
     ARRAY['security-red-team'], 30),

    ('ffuf', 'ffuf', 'offensive_tooling', 'security',
     'https://github.com/ffuf/ffuf',
     'Content and parameter discovery by brute force, fast. Most of what an '
     'engagement finds starts with a path nobody linked to.',
     'Free, open source. One binary, no runtime.',
     ARRAY['security-red-team'], 40),

    ('sqlmap', 'sqlmap', 'offensive_tooling', 'security',
     'https://sqlmap.org/',
     'Automated detection and exploitation of injection. Worth learning after '
     'doing it by hand, not before: it will find things you cannot explain.',
     'Free, open source. Default settings are noisy and destructive enough to '
     'break a target — read the flags.',
     ARRAY['security-red-team'], 50),

    ('metasploit-framework', 'Metasploit Framework', 'offensive_tooling', 'security',
     'https://github.com/rapid7/metasploit-framework',
     'Exploit and payload framework. Most useful here for post-exploitation '
     'and for understanding what an exploit actually consists of.',
     'Free, open source framework. The commercial Pro edition adds automation '
     'nobody learning needs.',
     ARRAY['security-red-team', 'security-purple-team'], 60),

    ('impacket', 'Impacket', 'offensive_tooling', 'security',
     'https://github.com/fortra/impacket',
     'Protocol implementations in Python — SMB, Kerberos, MSRPC — and the '
     'scripts built on them. The toolkit for anything Windows or Active '
     'Directory.',
     'Free, open source.',
     ARRAY['security-red-team'], 70),

    ('kali-linux', 'Kali Linux', 'offensive_tooling', 'security',
     'https://www.kali.org/',
     'A distribution with the tooling preinstalled. Convenient, and not a '
     'requirement: everything in it installs on any distribution, and knowing '
     'which package you installed is worth more than having all of them.',
     'Free. Available as a virtual machine image and as a WSL2 install.',
     ARRAY['security-red-team'], 80);

-- ═══════════════════════════════════════════════════════════════════
-- Defensive tooling
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('wireshark', 'Wireshark', 'defensive_tooling', 'security',
     'https://www.wireshark.org/',
     'Reading captured traffic, packet by packet or by reassembled stream. '
     '`tshark` is the same engine on the command line, which is how it goes '
     'into a script.',
     'Free, open source. Capturing on an interface needs privileges; reading a '
     'file does not.',
     ARRAY['security-blue-team'], 10),

    ('suricata', 'Suricata', 'defensive_tooling', 'security',
     'https://suricata.io/',
     'Intrusion detection that also replays a capture offline, which is how a '
     'rule gets tested without a network.',
     'Free, open source. Rule sets are separate; the free ET Open set is where '
     'to start.',
     ARRAY['security-blue-team', 'security-purple-team'], 20),

    ('sigma', 'Sigma', 'defensive_tooling', 'security',
     'https://github.com/SigmaHQ/sigma',
     'A vendor-neutral format for detection rules, and a large public rule '
     'repository. Write once, convert to whichever query language the SIEM '
     'speaks.',
     'Free, open source. The rule repository is also the best free reading on '
     'what detections look like.',
     ARRAY['security-blue-team', 'security-purple-team'], 30),

    ('wazuh', 'Wazuh', 'defensive_tooling', 'security',
     'https://wazuh.com/',
     'Open-source endpoint monitoring and log analysis with a working default '
     'rule set. The cheapest way to have somewhere for alerts to arrive.',
     'Free, open source, self-hosted. Expect a few gigabytes of memory for a '
     'single-node install.',
     ARRAY['security-blue-team'], 40),

    ('opensearch-security-analytics', 'OpenSearch', 'defensive_tooling', 'security',
     'https://opensearch.org/',
     'Where logs go when there are too many to grep. Its security analytics '
     'plugin reads Sigma rules directly.',
     'Free, open source, Apache 2.0. Self-hosting one node is manageable; '
     'operating a cluster is a job.',
     ARRAY['security-blue-team'], 50);

-- ═══════════════════════════════════════════════════════════════════
-- Forensics
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('volatility3', 'Volatility 3', 'forensics_tooling', 'security',
     'https://github.com/volatilityfoundation/volatility3',
     'Reading a memory image: processes, injected code, network state, and '
     'what was never written to disk.',
     'Free, open source. Needs a symbol table for the operating system of the '
     'image, which is the usual first obstacle.',
     ARRAY['security-blue-team'], 10),

    ('autopsy-sleuthkit', 'Autopsy and The Sleuth Kit', 'forensics_tooling', 'security',
     'https://www.autopsy.com/',
     'Disk images: file systems, deleted content, timelines. The graphical '
     'front end to a command-line toolkit that has been the reference for '
     'twenty years.',
     'Free, open source.',
     ARRAY['security-blue-team'], 20),

    ('yara', 'YARA', 'forensics_tooling', 'security',
     'https://github.com/VirusTotal/yara',
     'Pattern matching for files. How a triage conclusion becomes something '
     'another team can run across their own estate.',
     'Free, open source.',
     ARRAY['security-blue-team'], 30),

    ('cyberchef', 'CyberChef', 'forensics_tooling', 'security',
     'https://gchq.github.io/CyberChef/',
     'Decoding, deobfuscating and converting, in a browser, without writing a '
     'script for each step.',
     'Free, open source. Runs entirely client-side, which is why it is safe to '
     'paste an artefact into it — but read that sentence twice before pasting '
     'a client''s data anywhere.',
     ARRAY['security-blue-team', 'security-red-team'], 40);

-- ═══════════════════════════════════════════════════════════════════
-- Code audit, emulation, governance
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES



    ('gitleaks', 'Gitleaks', 'offensive_tooling', 'security',
     'https://github.com/gitleaks/gitleaks',
     'Finding credentials in a repository and in its history. A rotated key is '
     'not a removed one.',
     'Free, open source. Already in this platform''s own pipeline.',
     ARRAY['security-code-audit'], 120),

    ('atomic-red-team', 'Atomic Red Team', 'purple_tooling', 'security',
     'https://github.com/redcanaryco/atomic-red-team',
     'Small, single-technique tests mapped to ATT&CK, each with its cleanup. '
     'The straightforward way to find out whether a detection fires.',
     'Free, open source. Every test says what it changes; run them somewhere '
     'you are allowed to break.',
     ARRAY['security-purple-team', 'security-blue-team'], 10),

    ('caldera', 'MITRE Caldera', 'purple_tooling', 'security',
     'https://github.com/mitre/caldera',
     'Automated adversary emulation: chains of techniques run against an '
     'environment on a schedule.',
     'Free, open source. Needs an environment of its own — this is not '
     'something to point at production.',
     ARRAY['security-purple-team'], 20),

    ('mitre-attack', 'MITRE ATT&CK', 'threat_intel', 'security',
     'https://attack.mitre.org/',
     'The shared vocabulary for what an attacker did. Without it, coverage is '
     'a feeling.',
     'Free. Also available as machine-readable data.',
     ARRAY['security-red-team', 'security-blue-team', 'security-purple-team'], 10),

    ('cwe', 'CWE', 'threat_intel', 'security',
     'https://cwe.mitre.org/',
     'The catalogue of weakness types. What a finding cites so that two '
     'reports about the same class of defect are recognisably about the same '
     'thing.',
     'Free.',
     ARRAY['security-code-audit', 'security-red-team'], 20),

    ('cvss-calculator', 'CVSS calculator', 'threat_intel', 'security',
     'https://www.first.org/cvss/calculator/3-1',
     'The official calculator. Producing the vector rather than the number is '
     'what makes a severity arguable.',
     'Free.',
     ARRAY['security-red-team', 'security-code-audit'], 30),

    ('owasp-asvs', 'OWASP ASVS', 'governance_tooling', 'security',
     'https://owasp.org/www-project-application-security-verification-standard/',
     'A checklist of verifiable application security requirements at three '
     'levels. Turns "is it secure" into a list somebody can be audited '
     'against.',
     'Free, open licence.',
     ARRAY['security-code-audit', 'security-governance'], 10),

    ('owasp-cheat-sheets', 'OWASP Cheat Sheet Series', 'governance_tooling', 'security',
     'https://cheatsheetseries.owasp.org/',
     'Short, current, per-topic guidance on how to do the defensive thing '
     'properly. The first place to look when a finding needs a fix proposal.',
     'Free, open source, and open to contributions.',
     ARRAY['security-code-audit', 'security-governance'], 20),

    ('cis-benchmarks', 'CIS Benchmarks', 'governance_tooling', 'security',
     'https://www.cisecurity.org/cis-benchmarks',
     'Hardening baselines per platform, at a level of detail nobody writes '
     'themselves. What a configuration audit is usually measured against.',
     'Free to download for personal and internal use after registration.',
     ARRAY['security-governance'], 30),

    ('nist-oscal', 'NIST OSCAL', 'governance_tooling', 'security',
     'https://pages.nist.gov/OSCAL/',
     'Control catalogues, baselines and assessment results as data rather than '
     'as spreadsheets. Where compliance work stops being copy-paste.',
     'Free, open source.',
     ARRAY['security-governance'], 40),

    ('open-policy-agent', 'Open Policy Agent', 'governance_tooling', 'security',
     'https://www.openpolicyagent.org/',
     'Policy as code, evaluated at the point of decision. How a written '
     'control becomes something that actually refuses.',
     'Free, open source, CNCF.',
     ARRAY['security-governance', 'security-code-audit'], 50),

    ('hackerone-directory', 'HackerOne', 'bounty_platform', 'security',
     'https://hackerone.com/directory/programs',
     'The largest public directory of disclosure programmes, and the one with '
     'the most published reports — which makes it also the best free reading '
     'on what a good report looks like.',
     'Free to join. Reputation gates access to some programmes, which is why '
     'the first few reports matter more than their bounties.',
     ARRAY['security-red-team'], 10),

    ('bugcrowd-programs', 'Bugcrowd', 'bounty_platform', 'security',
     'https://bugcrowd.com/programs',
     'The other large platform. Different programme mix, and a published '
     'taxonomy of vulnerability ratings worth reading on its own.',
     'Free to join.',
     ARRAY['security-red-team'], 20),

    ('intigriti-programs', 'Intigriti', 'bounty_platform', 'security',
     'https://app.intigriti.com/researcher/programs',
     'European platform, with a good proportion of programmes from smaller '
     'organisations — which in practice means less competition per report.',
     'Free to join.',
     ARRAY['security-red-team'], 30),

    ('yeswehack-programs', 'YesWeHack', 'bounty_platform', 'security',
     'https://yeswehack.com/programs',
     'French platform with a substantial francophone community and programmes '
     'from organisations that publish in French.',
     'Free to join.',
     ARRAY['security-red-team'], 40),

    ('disclose-io', 'disclose.io', 'bounty_platform', 'security',
     'https://disclose.io/',
     'Standard safe-harbour and disclosure policy language, and a directory of '
     'who has adopted it. What to read before writing a programme''s terms — '
     'or before testing anybody.',
     'Free, open source.',
     ARRAY['security-red-team', 'security-governance'], 50);

-- ═══════════════════════════════════════════════════════════════════
-- Terrains — projects that want contributions (T-11)
-- ═══════════════════════════════════════════════════════════════════
--
-- The labels are a researched starting point, not a guarantee: 0533 already
-- wrote down that upstream label sets get renamed, and a steward confirms them
-- at adoption. Where a project's taxonomy was not certain the list stays to
-- the labels that are stable across the ecosystem.

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order)
VALUES

('owasp-zap-project', 'OWASP ZAP', 'security', 'oss_repo',
 'https://github.com/zaproxy/zaproxy',
 ARRAY['good first issue', 'help wanted'],
 'The open-source proxy and scanner most people who cannot afford Burp Pro end '
 'up using, which makes every improvement to it improvement for beginners '
 'everywhere. Contributions do not require writing scanner internals: the '
 'add-on ecosystem, the automation framework documentation and the rule '
 'descriptions are all chronically short of people, and the rule descriptions '
 'in particular are read by every user who does not already know what the '
 'finding means.', 610),

('owasp-webgoat-project', 'OWASP WebGoat', 'security', 'oss_repo',
 'https://github.com/WebGoat/WebGoat',
 ARRAY['good first issue', 'help wanted'],
 'The teaching application. Writing a new lesson is the clearest possible '
 'demonstration that somebody understands a defect class: you have to build a '
 'working vulnerability, an explanation, a hint sequence and a solution check. '
 'Also the friendliest place in this domain to make a first pull request, '
 'because a lesson is self-contained.', 620),

('owasp-juice-shop-project', 'OWASP Juice Shop', 'security', 'oss_repo',
 'https://github.com/juice-shop/juice-shop',
 ARRAY['good first issue', 'help wanted'],
 'The most-used vulnerable application in the world, and unusually welcoming: '
 'new challenges, translations and tutorial improvements are all accepted, and '
 'the maintainers document how to add a challenge properly. Contributing here '
 'is also the fastest way to understand how the challenges you have been '
 'solving are actually detected.', 630),

('sigma-rules', 'SigmaHQ — detection rules', 'security', 'oss_repo',
 'https://github.com/SigmaHQ/sigma',
 ARRAY['good first issue', 'help wanted'],
 'A public repository of detection rules, reviewed in the open. This is the '
 'single best terrain for blue-team contribution: a rule is small, its quality '
 'is arguable on evidence, and the review comments are an education in what '
 'separates a rule that fires from a rule that fires too often. Contributions '
 'are also citable — a merged rule is a detection running in other people''s '
 'estates.', 640),

('semgrep-rules', 'Semgrep rules', 'security', 'oss_repo',
 'https://github.com/semgrep/semgrep-rules',
 ARRAY['good first issue', 'help wanted'],
 'The community rule registry. For a code auditor this closes the loop: find a '
 'defect by hand, write the rule that catches the class, get it reviewed by '
 'people who will argue about the false positives. A merged rule is a finding '
 'that keeps working after you have moved on.', 650),

('atomic-red-team-project', 'Atomic Red Team', 'security', 'oss_repo',
 'https://github.com/redcanaryco/atomic-red-team',
 ARRAY['good first issue', 'help wanted'],
 'Single-technique tests mapped to ATT&CK. Adding one is purple-team work in '
 'its purest form: name the technique, write the smallest thing that performs '
 'it, write the cleanup, and say what a detection should see. Small, '
 'reviewable, and immediately useful to anybody validating coverage.', 660),

('volatility3-project', 'Volatility 3', 'security', 'oss_repo',
 'https://github.com/volatilityfoundation/volatility3',
 ARRAY['good first issue', 'help wanted'],
 'Memory forensics. Harder than the others and worth naming anyway: a plugin '
 'is a self-contained contribution, the maintainers review carefully, and '
 'there is no faster way to actually understand what a memory image contains '
 'than to write something that reads one.', 670),

('owasp-cheat-sheets-project', 'OWASP Cheat Sheet Series', 'security', 'oss_repo',
 'https://github.com/OWASP/CheatSheetSeries',
 ARRAY['good first issue', 'help wanted'],
 'Markdown. The lowest barrier to a first contribution in this domain, and not '
 'a trivial one: these pages are what a developer reads at the moment they are '
 'about to implement something, and several of them are years behind the '
 'framework advice they give. Fixing one is real defensive work.', 680),

('nuclei-templates', 'Nuclei templates', 'security', 'oss_repo',
 'https://github.com/projectdiscovery/nuclei-templates',
 ARRAY['good first issue', 'help wanted'],
 'A template is a checkable description of one vulnerability. Writing one for '
 'a defect you have understood turns a single finding into something everybody '
 'else can test for, and the review will tell you quickly whether your '
 'detection logic actually distinguishes the vulnerable case.', 690);
