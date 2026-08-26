-- What a security reviewer reads for, per family.
--
-- ## Why the domain had no grid at all
--
-- `review_grids` has carried one default grid per domain and one per reviewer
-- family since 0180, and `security` had neither: the domain had a single
-- orientation with no `reviewer_group`, so there was no family to key a grid
-- to, and nobody wrote the default. A review task in this domain therefore
-- reached a reviewer with no criteria attached, which is the state where the
-- score means whatever the reviewer already thought.
--
-- ## Six criteria, as everywhere else
--
-- Not because six is a magic number, but because the other nine domains
-- settled on it and a grid that is twice as long as its neighbours gets
-- skimmed. Where a seventh mattered it was folded into another line rather
-- than appended.
--
-- ## The one criterion every grid here shares in spirit
--
-- Somebody else must be able to get to the same result. In red team that is a
-- replay; in blue team it is a query that fires on the sample; in code audit
-- it is a path a reader can follow; in governance it is evidence an auditor
-- would accept; in purple it is a detection that can be re-run. This domain
-- is unusually easy to be plausible in and unusually easy to check, and the
-- grids are written to make the reviewer check rather than be convinced.
--
-- ## What is deliberately absent
--
-- Tool inventories. A finding is not better because it came out of Burp Pro,
-- and a detection is not better because the SIEM was expensive. No line here
-- rewards equipment.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

-- ═══════════════════════════════════════════════════════════════════
-- The domain default
-- ═══════════════════════════════════════════════════════════════════
--
-- Read when a security artefact reaches review without a family — a
-- cross-family exercise, or an orientation added later that has not been
-- assigned one yet.

('security', NULL, 'Security work — the common floor', '[
  {"criterion": "There was permission",
   "looks_like": "The target was in a written scope before anything was sent to it. A finding on something out of scope is refused however real it is, and saying so is not a formality: it is the whole difference between this trade and the offence it resembles."},
  {"criterion": "A stranger reaches the same result",
   "looks_like": "Steps, requests, queries or commands precise enough that a reviewer who has never seen the system gets there. \"I fuzzed it and it broke\" is a story."},
  {"criterion": "Impact is stated, not implied",
   "looks_like": "What an attacker could actually do, on this system, with this defect. Not the worst thing the vulnerability class has ever caused elsewhere."},
  {"criterion": "Nothing was taken that was not needed",
   "looks_like": "Enough data to prove the finding and no more, no persistence left behind, nothing broken to show it could be. The report says what was touched."},
  {"criterion": "Uncertainty is written down",
   "looks_like": "What was not checked, what could not be reproduced, what depends on an assumption. A report with no unknowns in it has usually stopped looking."},
  {"criterion": "AI use is declared",
   "looks_like": "A model used to draft, to explain or to generate a payload is named. It is accepted; a report whose reproduction steps nobody ran is not."}
]'::jsonb),

-- ═══════════════════════════════════════════════════════════════════
-- Red team
-- ═══════════════════════════════════════════════════════════════════

('security', 'red-team', 'Offensive work', '[
  {"criterion": "The exploit replays",
   "looks_like": "The reviewer follows the steps on the stated target and sees the same thing. This is binary and it is the first thing checked: nothing else in the report matters if it does not."},
  {"criterion": "The proof proves the claim",
   "looks_like": "The screenshot, response or capture shows the specific consequence claimed — data that should not be readable, an action that should not be permitted — rather than an error page."},
  {"criterion": "Severity is argued from a vector",
   "looks_like": "A CVSS vector with the choices defensible, or an explicit argument why the vector understates it. A bare adjective is not a severity."},
  {"criterion": "The root cause is named",
   "looks_like": "Which check is missing and where, not just which request returns the wrong thing. A report that stops at the symptom leaves the same class of defect in place."},
  {"criterion": "Scope was respected under pressure",
   "looks_like": "No pivot to what was interesting but out of scope, no denial of service to demonstrate load, no third-party account touched. Where the boundary was reached, the report says so."},
  {"criterion": "It is readable by the person who has to fix it",
   "looks_like": "A developer with no offensive background can follow it to the line. Jargon that saves the author time and costs the fixer a day fails this."}
]'::jsonb),

-- ═══════════════════════════════════════════════════════════════════
-- Blue team
-- ═══════════════════════════════════════════════════════════════════

('security', 'blue-team', 'Defensive work', '[
  {"criterion": "The conclusion follows from the artefact",
   "looks_like": "Every claim points at a line, a packet or an offset in the material provided. An analysis that needs knowledge the artefact does not contain has guessed."},
  {"criterion": "The detection fires, and stays quiet",
   "looks_like": "Run against the sample it triggers; run against ordinary traffic or logs it does not. Both halves shown. A rule tested only on the positive case is a hypothesis."},
  {"criterion": "The timeline is ordered and sourced",
   "looks_like": "Events in sequence, each with where it came from, and timezone stated once. Clock skew between sources is called out rather than smoothed over."},
  {"criterion": "Observation is kept apart from inference",
   "looks_like": "\"The account authenticated from this address\" and \"the account was compromised\" are in different sentences, and the second one says what makes it likely."},
  {"criterion": "It says what to do next",
   "looks_like": "Containment, eradication and the detection or control that would have caught it earlier. An analysis with no recommendation is a description."},
  {"criterion": "Sensitive content is handled",
   "looks_like": "Credentials, tokens and personal data in the artefact are redacted in the write-up, and anything extracted is not republished. The indicators stay; the payload contents do not have to."}
]'::jsonb),

-- ═══════════════════════════════════════════════════════════════════
-- Code audit
-- ═══════════════════════════════════════════════════════════════════

('security', 'code-audit', 'Code security review', '[
  {"criterion": "The path is traced end to end",
   "looks_like": "From the entry point to the sink, through every layer that was supposed to sanitise, with file and line at each step. A finding that names a sink without naming its reachable source is a scanner hit."},
  {"criterion": "Reachability is established",
   "looks_like": "The code is on a path a request can take, with the configuration and feature flags that make it so. A vulnerable function nothing calls is documented as such rather than counted."},
  {"criterion": "The fix is proposed and would work",
   "looks_like": "A concrete change, at the right layer, that closes the class and not only the instance. \"Sanitise the input\" is not a fix; the parameterised query is."},
  {"criterion": "False positives are stated",
   "looks_like": "What the tool flagged and why it was dismissed, with the reason. An audit that reports only hits gives no way to judge how carefully it read."},
  {"criterion": "Dependencies are judged, not listed",
   "looks_like": "An advisory in the tree is a finding only when the vulnerable code path is reachable from this project. Version tables pasted from a scanner fail this line."},
  {"criterion": "Nothing sensitive is republished",
   "looks_like": "Secrets found in code or history are reported privately and redacted in the write-up. An audit that publishes a live key has caused the incident it was looking for."}
]'::jsonb),

-- ═══════════════════════════════════════════════════════════════════
-- Governance
-- ═══════════════════════════════════════════════════════════════════

('security', 'governance', 'Governance and compliance work', '[
  {"criterion": "It maps to a named requirement",
   "looks_like": "Each control or clause points at the article, control number or criterion it answers. A policy that cites nothing cannot be audited against anything."},
  {"criterion": "It describes what is actually done",
   "looks_like": "The document matches the practice, including where the practice is worse than the aspiration. A policy nobody follows is a liability, not a control."},
  {"criterion": "Evidence exists and is reproducible",
   "looks_like": "For each claim, what an auditor would be shown, where it comes from, and how it will be produced again next year. Screenshots with no query behind them fail."},
  {"criterion": "Risk is assessed with a method",
   "looks_like": "A stated scale, applied consistently, with the reasoning attached. Two assessors using it on the same risk should land in the same place."},
  {"criterion": "Residual risk is accepted by somebody",
   "looks_like": "What is not being fixed, why, and who decided. An unowned acceptance is how a finding survives three audits."},
  {"criterion": "It is possible to comply with",
   "looks_like": "Short enough to be read, specific enough to be checked, and workable on an ordinary day. A control that requires heroism will be bypassed and then documented as met."}
]'::jsonb),

-- ═══════════════════════════════════════════════════════════════════
-- Purple team
-- ═══════════════════════════════════════════════════════════════════

('security', 'purple-team', 'Purple exercise work', '[
  {"criterion": "A detection exists that did not before",
   "looks_like": "The output of the exercise is a rule, a query or a control, committed somewhere, with the technique it covers named. An exercise whose output is a slide deck has not finished."},
  {"criterion": "Techniques are named in a shared vocabulary",
   "looks_like": "ATT&CK identifiers or an equally explicit taxonomy, so coverage can be argued about. \"We tried some lateral movement\" cannot be mapped to anything."},
  {"criterion": "The detection was validated by re-running the technique",
   "looks_like": "Execute, observe the alert, and show both. Writing the rule and asserting it would fire is the failure this line exists for."},
  {"criterion": "Both sides are represented in the record",
   "looks_like": "What the attack did and what the defence saw, on the same timeline, including the steps nothing saw. A report written by one side only is a pentest or an audit."},
  {"criterion": "The exercise was reversible",
   "looks_like": "A stop condition, cleanup that was verified, and no artefact left in the environment. Simulation tooling that leaves persistence behind has created a real incident."},
  {"criterion": "The gaps are ranked",
   "looks_like": "Which blind spots matter most given this organisation, and what closing each would cost. An unranked gap list gets read once."}
]'::jsonb);
