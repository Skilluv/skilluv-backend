# Writing a challenge somebody can pass

For whoever is adding to the security catalogue. Two families, and the rule that
decides which one you are writing.

## The rule

**Does this platform own the secret?**

If yes — a range we host, an artefact we hold — the challenge is machine
checked, and it is created through the API by somebody who has solved it.

If no — Juice Shop, a retired machine, a published forensic dataset — the
challenge is checked by a person reading a write-up, and it is seeded like every
other domain's catalogue.

Getting this backwards produces the one failure that is invisible from the
inside: a flag hash invented by an author who could not know the answer, on a
challenge that is published, claimable and permanently unanswerable. Nothing
errors. Forty people conclude they are not good enough.

## The two families

| Kind | Verified by | Who creates it |
|---|---|---|
| `ctf_flag` | SHA-256 of a flag we planted | `POST /api/admin/security/challenges` |
| `defensive_lab` | Hashed answers to questions we know | `POST /api/admin/security/challenges` |
| `training_ground` | A reviewer reading a write-up | Seeded, curated |
| `machine_walkthrough` | A reviewer reading a write-up | Seeded, curated |
| `analysis_exercise` | A reviewer reading a write-up | Seeded, curated |
| `audit_exercise` | A reviewer reading a write-up | Seeded, curated |

---

## Writing a flag challenge

### The flag

Format `SKILLUV{lower_snake_case}`. Recognisable, so somebody knows they have
found it, and specific enough that it cannot be guessed from the scheme.

**Never derive it from something public.** A flag that is `SKILLUV{` plus the
challenge slug is a flag anybody can compute without solving anything.

Send it in plaintext to the creation endpoint; it is hashed there and the
plaintext is never stored. That response is the last time it exists outside
your notes.

### The target

`security_target_url` has to be somewhere you control and are allowed to have
attacked. If that is not true, you are writing a `training_ground` challenge.

### The description

Four things, in this order:

1. **The objective**, in one sentence, behavioural. "Log in as the
   administrator without knowing the password" — not "exploit the SQL injection
   in the login form", which is the answer.
2. **A progressive hint**, two or three sentences. Where to look, not what to
   do.
3. **The flag format**, exactly.
4. **What it is worth**, and roughly how long.

### Difficulty

Two fields, and both matter. `difficulty` 1–5 drives rewards and
recommendations across the whole platform; `security_difficulty_tier` is the
word this trade uses. Calibrate the tier by time for somebody who has done the
previous one:

| Tier | Time | Shape |
|---|---|---|
| `easy` | under an hour | One defect, visible from the surface |
| `medium` | one to three hours | One defect that needs enumeration first |
| `hard` | half a day | Two steps, or one non-obvious step |
| `insane` | a weekend | A chain, where each step is only reachable from the last |

### Before publishing

**Somebody who is not you solves it from the instructions alone.** Not you
re-reading them. Every unanswerable challenge in the history of this format got
there by skipping this.

Challenges are created as `draft` for that reason, and the response says so.

## Writing a defensive lab

Same rule: you must know the answers, which means you produced or fully
analysed the artefact.

### The artefact

Uploaded to the private bucket. Its size is shown before the download starts,
because a five-hundred-megabyte memory image on a metered connection is a
decision rather than a click.

**Redact it.** A log window contains other people's requests. Credentials,
tokens, and anything identifying somebody who was not part of the incident come
out; addresses stay, because they are the object of the analysis. If the
artefact came from this platform's own logs, use
`POST /api/admin/security/findings/{id}/blue-lab`, which asks you to confirm
that explicitly.

### The questions

Between four and eight. Each one:

- **Answerable from the artefact alone.** If it needs something in your head,
  it is unanswerable for everybody else.
- **Not guessable.** "Which tool was used?" with four choices is a coin flip
  twice. "Paste the user agent of the successful request" is not.
- **With a hint**, shown only when the answer is wrong. This is what makes a
  failed attempt teach something instead of just failing.

Answers are normalised — trimmed and lowercased — before hashing, unless the
question is marked `case_sensitive`. Mark it when the answer is a payload or a
hash; leave it alone when it is an address, a count or a tool name.

Set the pass mark deliberately. Eighty per cent of five questions is four; of
eight it is seven. Three attempts, then a day's cooling off.

## Writing a human-checked challenge

Everything above about objectives and calibration applies. What replaces the
flag:

- **`security_external_url`** — where the target lives. Linked, never rehosted:
  their licence, and their maintenance.
- **`security_attribution_md`** — whose material it is and under what terms.
  Several of the forensic datasets are CC-BY, which requires the attribution to
  travel with the use. An attribution in a migration comment travels nowhere.
- **`security_writeup_required = TRUE`** — enforced by a constraint, because
  without it the challenge would be published with no verification at all.
- **A brief** saying what a good write-up contains, from the templates in
  `content_guides`.

For machines specifically: ask for the write-up **in their own words**, and ask
which existing walkthroughs they read and when. Official write-ups exist for
every retired machine; a submission that reproduces one is refused, and the
honesty about what was read is assessed as much as the machine.

## What not to write

**Trick questions.** A challenge that turns on a pun, an off-by-one in the
instructions, or a guess is not teaching a technique. If somebody who knows the
technique cannot pass it, it is not a security challenge.

**Anything requiring a paid tool.** If it cannot be done with Burp Community and
ZAP, say so in the description and expect fewer people.

**Anything requiring a specific machine.** "Needs 32 GB of memory" is a
challenge for people who already have the job. The onboarding wizard asks what
somebody can run precisely so the recommender does not do this.

**Real personal data, ever.** Not in an artefact, not in a screenshot, not in a
database dump. There is no educational value that pays for it.
