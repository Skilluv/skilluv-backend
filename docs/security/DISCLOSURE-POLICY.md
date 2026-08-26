# What happens to a report

The clocks, the states, and who may move them. This is the policy the code
enforces — every rule below is a constraint, a state-machine entry or a worker,
and where it is one the file and function are named so you can check.

## The states

```
submitted ──▶ triaged ──▶ confirmed ──▶ fixed ──▶ published
    │            │            │
    │            │            └──▶ duplicate
    │            └──▶ not_applicable
    └──▶ withdrawn
```

`allowed_transition` in `src/services/security_findings.rs` is the whole table,
and its tests walk it. Three things it refuses that are worth knowing:

- **Nothing skips the middle.** A `submitted` report cannot become `published`,
  by anybody, ever.
- **A triager may not confirm.** Triage decides whether something is worth a
  reviewer's afternoon; confirming asserts publicly that a vulnerability is
  real, and that is a different judgement.
- **Only an administrator publishes.** Every other transition can be corrected
  by making another one. Publication cannot, because the internet keeps a copy.

## The clocks

| Clock | Length | What happens at the end |
|---|---|---|
| Acknowledgement | Immediate | Automated, on submission |
| Triage | **7 days** | A person has read it and said why, either way |
| Embargo | **90 days** from confirmation | The finding becomes `partially_disclosed` and an administrator decides |
| Round | No fixed limit, **5 rounds maximum** | Somebody decides on the evidence available |

The embargo length is `disclosure_policy_days` on the row, defaulting to 90. A
mission or a project can carry its own, which is why it is a column and not a
constant.

## The embargo does not publish anything by itself

The design asked for automatic publication when the clock runs out and the
owner has gone quiet. That is refused, and the reason is in
`sweep_embargoes`: publishing a vulnerability is irreversible, and a cron job
at three in the morning is the wrong thing to be holding that decision.

What expiry produces is `disclosure_stage = 'partially_disclosed'` — the
existence and the severity become quotable, the reproduction does not — and an
item on an administrator's list. Same outcome one working day later, and it
cannot go wrong unattended.

Reminders go out at 30, 7 and 1 days before the end, to the reporter, because
the clock is a promise this platform made them.

## Extensions

The owner of the system can ask for more time
(`POST .../extension`), and an administrator grants it
(`POST .../extension/grant`). Two calls, deliberately: an extension that
applied itself would make the embargo a suggestion.

Both are on the finding's history with the reason, so a researcher can see who
asked for what and when.

## Withholding

Some findings should never be published — a defect in a system that cannot be
patched, a disclosure that would harm somebody who is not a party. An
administrator can set `disclosure_stage = 'withheld'` with at least twenty
characters of reasoning, which is stored and shown.

This is not a way to make an inconvenient finding disappear. The finding stays
confirmed, the attestation stays issued, and the reporter keeps their credit on
the hall of fame. Only the write-up does not happen.

## Rounds

A finding is rarely accepted or refused on first read. A reviewer opens a round
naming what is missing, using one of six kinds:

| Kind | When |
|---|---|
| `sec_repro_insufficient` | The reviewer cannot get to the same result |
| `sec_proof_insufficient` | The screenshot shows an error page, not the claim |
| `sec_severity_disputed` | The defect is agreed, the vector is not |
| `sec_patch_requested` | A concrete remediation is wanted before accepting |
| `sec_scope_question` | What was tested may be outside the authorisation |
| `sec_impact_unclear` | The impact is missing or is the class restated |

The researcher answers on the report; the reviewer resolves the round as
`satisfied` or `insufficient`. Five rounds is the database's limit. After the
fifth, a report that is still not reproducible is a decision rather than
another round.

## Severity

Computed from a CVSS 3.1 vector, never accepted as a number. `services::cvss`
implements the published formula, including its own `Roundup` — which is not
"round to one decimal", and getting that wrong moves scores across the
medium/high boundary.

A reviewer can override, and when they do:

- the reporter's tier is **kept** (`severity_reported_tier`), so the
  disagreement stays readable;
- a written reason of at least twenty characters is required by the database as
  well as by the service;
- the reporter is notified with both values and the reasoning.

An unexplained severity change is the thing researchers leave a platform over,
and it is not possible here.

## Duplicates

Two people find the same thing. Industry practice is first-to-file, and this
platform follows it — with the second finding recorded rather than discarded:

- the original keeps its confirmation and its fragments;
- the duplicate gets `security_finding_co_credit`, an attestation that says
  independent co-discovery, with the timestamps that show it was not copied;
- the craft score counts a co-credit at a third of an original.

**Nothing is merged by a machine.** A similarity scan runs every fifteen
minutes and flags candidates on the row; a person decides, because a merge
decides who is paid and a trigram score does not get to.

## Anonymity

A checkbox on the report. An anonymous reporter appears on the hall of fame as
`anonymous-<six hex characters>`, derived from their user id so it is stable
across their findings, and their profile is not linked. Their attestations are
still theirs and still verifiable.

Per finding, not per account: the same person may want their name on a web bug
and not on a client engagement.

## Proof files

Uploaded before the report — that is the shape of the form — to
`POST /api/security/reports/uploads`, which returns a **key** and not a URL.

The private bucket, always. A screenshot of an unfixed vulnerability *is* the
vulnerability, and in a public bucket the embargo would be decorative. Download
links are minted per request, expire in an hour, and are refused to anybody who
is not the reporter, a triager, a reviewer of this domain or an administrator.

An upload nothing references after thirty days is deleted by a daily sweep.
Uploads happen before submission, so abandoned drafts leave files behind, and a
bucket that only grows eventually holds somebody's proof of something they
never reported.

Executables are refused on the extension **and** on the first bytes. This is a
security platform, and a report attachment is the most obvious place to try to
have somebody run something. Nothing is scanned for malware, and that is said
plainly rather than implied — see `REVIEWER-ONBOARDING.md` for what a reviewer
is expected to open a proof file in.

## Notifications

Twelve kinds, all transactional: a person cannot opt out of learning what
happened to a report they filed. Not being told is the single most common way a
disclosure programme dies.

The text is in `locales/{en,fr,ar}.yml` under `notification.security`, so
adding a language does not mean editing any call site.

## What a reporter can do

| Action | When |
|---|---|
| Withdraw | While `submitted` or `triaged` |
| Answer a round | Whenever one is open |
| Ask for anonymity | At submission |
| Dispute a severity | By replying on the round the reviewer opened, or by asking for one |

## What this policy does not cover

Payment. There is none. See `SCOPE.md`.

Disputes about a paid engagement rather than a disclosure: those go through
`disputes` and `mission_arbitrations`, and the policy for them is in
`docs/security/LEGAL.md`.
