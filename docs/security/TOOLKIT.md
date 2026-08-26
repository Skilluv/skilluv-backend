# The toolkit

What to install, what it costs, and what the free edition actually does.

**The authoritative list is `external_resources`**, readable at
`GET /api/domains/security/toolkit` with `?category=` and `?orientation=`
filters. This page is the readable version and says the things a table cannot.

Two rules the list follows:

- **Nothing commercial is presented as a requirement.** Burp Suite is listed in
  its free edition with what that edition cannot do, because a toolkit that
  quietly assumes a four-hundred-euro licence is a toolkit for people who
  already have the job.
- **No exploit collections, no malware repositories, no credential dumps.** The
  line is: material built for practice, or a tool. Not somebody else's stolen
  data, whatever its educational value.

---

## Ranges — where to practise

| Tool | Cost | Note |
|---|---|---|
| **PortSwigger Web Security Academy** | Free, labs included | The reference curriculum. Start here. |
| **OWASP WebGoat** | Free | Explains the defect first, and ships its source. The gentlest entry there is. |
| **OWASP Juice Shop** | Free | One container, no account. The most-used vulnerable application in the world. |
| **DVWA** | Free | Narrower, and the clearest place to see one injection class at four difficulty levels. |
| **TryHackMe** | Freemium | Guided rooms with hints. Where to start if a bare machine with no instructions is discouraging. |
| **Hack The Box** | Freemium | Machines with no instructions. Retired ones have community write-ups, which makes them the ones to learn on — and they need a subscription. |
| **VulnHub** | Free | Downloadable vulnerable machines. Runs entirely offline, which matters where bandwidth is the constraint. |
| **CyberDefenders** | Freemium | Blue-team labs: a real artefact and questions. |
| **Malware Traffic Analysis** | Free | Years of real captures with exercises and answers. The best free source of traffic that looks like an infection. |
| **`staging.skill-uv.com`** | Free | This platform, in scope, with a safe harbour. Read `SCOPE.md`. |

---

## Offensive

| Tool | Cost | What the free edition does |
|---|---|---|
| **Burp Suite Community** | Free | Interception, repeater, manual testing — which is the work. No active scanner; the intruder is throttled to a demonstration. Neither is needed to learn. |
| **OWASP ZAP** | Free, Apache 2.0 | Genuinely free including the active scan, and scriptable. The one to automate with. |
| **nmap** | Free | Host and service discovery, plus a scripting engine. Scanning anything you lack permission to scan is the line this whole domain is about. |
| **ffuf** | Free | Content and parameter discovery. Most of what an engagement finds starts with a path nobody linked to. |
| **sqlmap** | Free | Worth learning *after* doing injection by hand. Default settings are noisy and destructive enough to break a target. |
| **Metasploit Framework** | Free framework | Most useful here for post-exploitation and for seeing what an exploit actually consists of. |
| **Impacket** | Free | Protocol implementations and the scripts on them. The toolkit for anything Active Directory. |
| **Kali Linux** | Free | Convenient, not required. Everything in it installs anywhere. |

## Code security

| Tool | Cost | Note |
|---|---|---|
| **Semgrep** | Free engine and registry | Rules readable and writable in an afternoon. The tool a code auditor turns a found defect into a check with. |
| **CodeQL** | Free for open source | Steeper, finds what patterns cannot. **Read the licence before running it on private code.** |
| **Trivy** | Free | Images, filesystems, repositories, secrets. One binary. |
| **Gitleaks** | Free | Credentials in a repository *and its history*. A rotated key is not a removed one. |

## Defensive

| Tool | Cost | Note |
|---|---|---|
| **Wireshark / tshark** | Free | Same engine, and `tshark` is how analysis stops being clicking. |
| **Suricata** | Free | Also replays a capture offline, which is how a rule gets tested without a network. Start with the free ET Open rule set. |
| **Sigma** | Free | Vendor-neutral detection rules, and a public repository that is the best free reading on what separates a rule that works from one that alarms. |
| **Wazuh** | Free, self-hosted | The cheapest way to have somewhere for alerts to arrive. Expect a few gigabytes of memory. |
| **OpenSearch** | Free, Apache 2.0 | Where logs go when there are too many to grep. Reads Sigma rules directly. One node is manageable; a cluster is a job. |

## Forensics

| Tool | Cost | Note |
|---|---|---|
| **Volatility 3** | Free | Memory images. Needs a symbol table for the image's operating system, which is the usual first obstacle. |
| **Autopsy / The Sleuth Kit** | Free | Disk images, deleted content, timelines. |
| **YARA** | Free | How a triage conclusion becomes something another team can run across their estate. |
| **CyberChef** | Free | Decoding and converting in a browser. Runs client-side — read that twice before pasting a client's data anywhere. |

## Emulation and validation

| Tool | Cost | Note |
|---|---|---|
| **Atomic Red Team** | Free | Single-technique tests mapped to ATT&CK, each with cleanup. Run them somewhere you are allowed to break. |
| **MITRE Caldera** | Free | Automated chains on a schedule. Needs an environment of its own — never point it at production. |

## Governance

| Tool | Cost | Note |
|---|---|---|
| **OWASP ASVS** | Free | Verifiable requirements at three levels. Turns "is it secure" into an auditable list. |
| **OWASP Cheat Sheet Series** | Free, and open to contributions | The first place to look when a finding needs a fix proposal. |
| **CIS Benchmarks** | Free after registration | Hardening baselines nobody writes themselves. |
| **NIST OSCAL** | Free | Control catalogues as data rather than spreadsheets. |
| **Open Policy Agent** | Free, CNCF | Policy as code. How a written control becomes something that refuses. |

## Vocabulary

**MITRE ATT&CK**, **CWE** and the official **CVSS 3.1 calculator**. All free, all
worth having open. Without a shared vocabulary, coverage is a feeling and a
severity is an adjective.

## Bounty platforms

**HackerOne**, **Bugcrowd**, **Intigriti**, **YesWeHack** — all free to join.
Reputation gates access to some programmes on the largest of them, which is why
the first few reports matter more than their bounties.

**disclose.io** is what to read before writing a programme's terms, or before
testing anybody at all.

---

## What runs on a modest machine

The question nobody answers. Roughly:

- **A browser and nothing else**: PortSwigger Academy, TryHackMe, CyberChef,
  every hosted lab. A complete first two months.
- **8 GB of memory**: Juice Shop, WebGoat, DVWA in containers; Wireshark;
  Semgrep; Volatility. Everything in weeks 1–8 of the curriculum.
- **16 GB**: one vulnerable virtual machine alongside your own tooling. VulnHub
  becomes available.
- **More than that, or a second machine**: Atomic Red Team and anything purple,
  because you need somewhere you are allowed to break.

The onboarding wizard asks which of those you have
(`security_lab_setup`) precisely so nothing recommends you a week you cannot
run.
