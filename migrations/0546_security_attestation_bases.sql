-- What a security attestation can be issued for.
--
-- ## Why `attestation_type` is not extended
--
-- Ticket A-01 asked for four new values on `attestations.attestation_type`:
-- `security_audit`, `vulnerability_disclosed`, `ctf_winner`,
-- `mission_completed`. That column stopped being the place for this in
-- migration 0406, which added `artefact` as the fourth and last type and moved
-- the specifics into `attestation_bases` as rows — for the reason 0305 wrote
-- down about CHECK constraints, and because every domain that has arrived
-- since would otherwise have restated the list.
--
-- So the four requested values become rows below, with thirteen more, and the
-- column keeps its four types. An attestation for a confirmed finding is
-- `attestation_type = 'artefact'`, `basis = 'security_finding_confirmed'`.
--
-- ## Seventeen bases, and why the finding has three of them
--
-- A finding passes through three states that are worth attesting separately,
-- because they are three different claims about the person:
--
--   * `security_finding_confirmed` — somebody reproduced it. This is the
--     technical claim, and it is the one that is true even if the vendor never
--     patches.
--   * `security_finding_published` — it went public with a write-up, after the
--     embargo, and the write-up teaches. That is a different skill and quite
--     often a different person's strength.
--   * `security_finding_co_credit` — somebody found the same thing
--     independently and second. Industry practice pays first-to-file only, and
--     paying nothing while also recording nothing is how a platform teaches
--     researchers not to report. The basis records the work without pretending
--     it was the original.
--
-- ## Why training grounds attest differently from real targets
--
-- `security_training_completed` and `security_machine_walkthrough_validated`
-- exist so that Juice Shop, WebGoat, DVWA and a retired HackTheBox machine do
-- not produce the same attestation as a finding on a live system. Solving a
-- deliberately vulnerable application is real learning and it is not
-- discovery: the answer was placed there. Conflating the two would make the
-- domain's strongest proof — "this person found something nobody had planted"
-- — unreadable.
--
-- ## `requires_deliverable`, and why a captured flag does not get one
--
-- TRUE wherever a human read an artefact and accepted it: a confirmed
-- finding, a reviewed walkthrough, an audit handed over, a mission delivery, a
-- contest submission. A `deliverables` row is what the rank and the craft
-- score count, so this flag decides what counts towards a rank.
--
-- FALSE for the two that are graded by comparing hashes — a captured flag and
-- a defensive lab — and that is a deliberate refusal rather than an
-- omission. Twenty Juice Shop challenges are twenty planted answers; if each
-- one produced a deliverable, a weekend on a training ground would outrank a
-- year of merged work, and the rank would stop meaning anything. They are
-- attested, they are counted by their own craft-score terms, and they do not
-- create deliverables.
--
-- Also FALSE for the three that rest on something outside the platform — a
-- co-credit with no fix of its own, a bounty confirmed elsewhere, a featuring
-- — and for taking part in a purple exercise, which is a room somebody was in
-- rather than a document they produced.

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order)
VALUES

('security_finding_confirmed', 'security', 'Vulnerability confirmed',
 'A vulnerability reported, reproduced by somebody else, and accepted as real. '
 'The attestation names the severity and the class, and says nothing about '
 'whether it was ever fixed — that is the owner''s work, not the reporter''s.',
 TRUE, 610),

('security_finding_published', 'security', 'Vulnerability disclosed',
 'A confirmed finding published after its embargo, with a write-up that '
 'explains the defect and the fix well enough for somebody to avoid the same '
 'class. Held to a higher standard than the confirmation: this one is read by '
 'strangers.',
 TRUE, 615),

('security_finding_co_credit', 'security', 'Independent co-discovery',
 'The same vulnerability, found independently and reported second. No bounty — '
 'first-to-file decides that — and a record that the work was done, with the '
 'timestamps that show it was not copied.',
 FALSE, 620),

('security_ctf_solved', 'security', 'Capture-the-flag challenge solved',
 'A flag captured on a hosted target and verified automatically. The '
 'attestation carries the difficulty tier and whether it was a first solve. '
 'No deliverable: the answer was planted, and a planted answer does not move a '
 'rank.',
 FALSE, 630),

('security_blue_lab_completed', 'security', 'Defensive analysis completed',
 'An artefact — logs, a capture, a memory image — analysed offline, with the '
 'answers verified against their hashes. Attests the reading, which is the '
 'part of defensive work that is hardest to demonstrate on a CV. Machine '
 'graded, so no deliverable; a lab written up and reviewed is a separate '
 'submission that does get one.',
 FALSE, 640),

('security_machine_walkthrough_validated', 'security', 'Machine walkthrough validated',
 'A retired machine or deliberately vulnerable virtual machine completed '
 'elsewhere, with a write-up submitted here and judged on its teaching value '
 'by a senior reviewer. The machine is not hosted by this platform and the '
 'attestation is about the write-up.',
 TRUE, 645),

('security_training_completed', 'security', 'Training ground completed',
 'A deliberately vulnerable application worked through end to end — WebGoat, '
 'DVWA, Juice Shop — with the walkthrough to show it. Real learning on a '
 'target whose answers were planted, which is why it is a separate basis from '
 'a finding.',
 TRUE, 650),

('security_code_audit_delivered', 'security', 'Security code audit delivered',
 'A codebase read for exploitable defects and the findings handed over: each '
 'one with its path traced, its reachability established and a fix proposed. '
 'Includes what the scanner said and was wrong about.',
 TRUE, 655),

('security_threat_model_validated', 'security', 'Threat model validated',
 'An architecture modelled before or alongside the code, with the threats named '
 'in a shared taxonomy, ranked, and each mitigation owned by somebody. Separate '
 'from an audit because the artefact does not exist yet — this is the trade '
 'practised at the only moment it is cheap.',
 TRUE, 656),

('security_detection_shipped', 'security', 'Detection shipped',
 'A detection rule built, validated by re-running the technique it was written '
 'for, and shipped somewhere it fires. The evidence is both halves: it triggers '
 'on the sample and stays quiet on a week of ordinary traffic.',
 TRUE, 657),

('security_incident_analysis_validated', 'security', 'Incident analysed',
 'A real incident written up: timeline, scope, indicators, and what would have '
 'caught it earlier. Reviewed on whether the conclusions follow from the '
 'artefacts rather than on how bad the incident was.',
 TRUE, 658),

('security_policy_validated', 'security', 'Governance artefact validated',
 'A policy, risk assessment, impact assessment or control mapping reviewed '
 'against the framework it claims to answer, and accepted. The artefact is a '
 'document and the test is whether an auditor would take it.',
 TRUE, 660),

('security_purple_exercise_facilitated', 'security', 'Purple exercise run',
 'An exercise where attack and defence ran against each other and a detection '
 'came out of it. Attests the facilitation and the outcome, not a score: the '
 'point of the format is that both sides win or neither does.',
 FALSE, 665),

('security_external_bounty_confirmed', 'security', 'Bounty confirmed elsewhere',
 'A finding paid or credited on another platform — HackerOne, Bugcrowd, '
 'Intigriti, YesWeHack — verified here against its public disclosure. Worth '
 'less than a finding this platform saw end to end, and recorded rather than '
 'ignored.',
 FALSE, 670),

('security_competition_won', 'security', 'Competition won',
 'A placing in a hosted security competition — jeopardy, attack-defence, bug '
 'bash, code-audit rally. The generic `contest_finalist` basis says somebody '
 'reached the final of something; this one says which security format, which '
 'is what a reader of this domain wants.',
 TRUE, 675),

('security_mission_delivered', 'security', 'Paid security mission delivered',
 'A commissioned engagement carried through and accepted by the client: '
 'penetration test, code audit, threat model, compliance work. Redacted to the '
 'type, the duration and the finding counts when the engagement was '
 'confidential, which most are.',
 TRUE, 680),

('featured_security_researcher', 'security', 'Featured',
 'Put forward by the security community for a week. Not a measure of skill — a '
 'measure of what the community wanted other people to read.',
 FALSE, 690);
