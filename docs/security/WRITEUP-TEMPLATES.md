# The templates

Five things this domain writes, and where the templates live.

**They are rows, not files.** `content_guides` with `kind = 'writeup_template'`,
in English and French, readable at:

```
GET /api/domains/security/guides?kind=writeup_template
```

They are rows because the editor loads them. A guide is read once; a template is
what somebody is typing into, and **the headings it does not have are the
sections that do not get written**. The reproduction section of a finding report
is the clearest case: a report without one is refused, and the difference
between a reporter who writes one and a reporter who does not is usually that
somebody put the heading there.

## The five

| Template | Slug | For |
|---|---|---|
| Vulnerability report | `security-template-finding` | Submitting a finding. What a reviewer needs to reproduce it and rate it. |
| Public disclosure | `security-template-disclosure` | The version a stranger reads after the embargo. Written to teach. |
| Defensive analysis | `security-template-analysis` | Reading an artefact to a conclusion, observation kept apart from inference. |
| Engagement report | `security-template-engagement` | What a client is handed: an executive page, then findings they can act on. |
| Threat model | `security-template-threat-model` | A system, its threats in a shared vocabulary, and who owns each mitigation. |

Plus five **brief templates** (`kind = 'brief_template'`), one per trade, for
whoever is *setting* work rather than doing it.

## The two sections in every one of them

**What was not done.** Where you stopped, what you did not touch, what you
believed was on the other side of the boundary.

**What is not certain.** What you could not confirm, and what your conclusion
depends on.

Both are what a reviewer most needs and neither is what somebody writing about
their own work volunteers. Putting them in the template is the only reliable way
to get them — and in the finding template, the first of the two is the section
that earns the one badge nothing awards automatically.

## The two rules that are not in any template

**A severity is a vector, not a number.** `CVSS:3.1/AV:N/AC:L/…` with a sentence
per non-obvious metric. A score somebody typed cannot be argued with, and a
severity you cannot argue is one you will lose.

**A live credential is never written up.** If you find one in code, in history,
in a log or in a capture, report it privately and redact it in everything you
write. An audit that publishes a working key has caused the incident it was
looking for.

## Changing one

They are rows, so a curator can edit them
(`domain_curator:security` or `admin`). Two things to keep if you do:

- The reproduction section stays required in the finding template. Everything
  else is arrangement; that one is what makes a report a report.
- Keep both locales in step. A template that exists in one language quietly
  becomes the only one anybody uses.
