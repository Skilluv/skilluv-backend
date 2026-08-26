# Incident response

What to do when something has gone wrong, written for whoever is on their own at
the time. `SECURITY.md` has the runbooks for four specific technical scenarios;
this document is the process around them — severities, who decides, the clocks,
and the post-mortem.

It is also an audit exercise in the security catalogue. A runbook nobody has
tried is a document.

---

## Severities

| | What it is | Response starts | Told |
|---|---|---|---|
| **S1** | Personal data exposed, or an attacker has access | Immediately, drop everything | Regulator within 72 h, affected people directly |
| **S2** | Service down, or a critical vulnerability confirmed and unpatched | Within an hour | Status page, and the reporter if there is one |
| **S3** | A defect affecting some users, or a confirmed high-severity finding | Same working day | The people affected |
| **S4** | Cosmetic, or an informational finding | Next working day | Nobody in particular |

Two things to notice. **A confirmed critical finding is an S2 even before it is
exploited** — an unpatched hole somebody else knows about is a service problem.
And **an S1 is defined by exposure, not by certainty**: if you are not sure
whether data was reached, it is an S1 until you are.

## Who decides

At a team of three, one person, and that has to be written down rather than
assumed: whoever is awake declares the severity, and declaring it is not a
decision to defer. Getting it wrong upward costs an hour; getting it wrong
downward costs the notification deadline.

There is no on-call rotation and no pager. That is a real limitation. Its
mitigation is that alerts go to Discord `#ops-alerts` where more than one person
sees them, and that the platform is small enough for an hour's delay not to be
catastrophic. Both of those stop being true with users.

## The five steps

### 1. Declare, and write the time down

Open a document — anything — with the timestamp, what you know, and the
severity. Everything else in this process reads it.

Do not skip this because you are busy. The reconstruction afterwards is what
the regulator, the post-mortem and everybody's memory depend on, and it cannot
be done from recollection.

### 2. Contain

Stop it spreading, before understanding it. In practice:

- **Credentials suspected:** rotate `JWT_SECRET` — this invalidates every
  session, deliberately — and any provider secret involved.
- **A specific account:**
  ```sql
  UPDATE user_sessions SET revoked_at = NOW() WHERE user_id = $1;
  ```
- **A specific address:** Cloudflare rule, and check whether it holds a research
  token — a researcher exceeding the scope looks exactly like an attacker at
  this point, and the difference is one query.
- **A deployment:** roll back to the previous signed image. `SECURITY.md` has
  the commands.

**Take a copy of the logs before rotating anything.** Rotation and restarts lose
evidence, and you will want it in step 4.

### 3. Eradicate

Now understand it. What was the path in, what did it reach, and is it closed.

If it came from a reported finding, the finding's own record — its reproduction,
its rounds, its proof files — is the best evidence available, and it is already
in the database.

Do not restore service until the way in is closed. A restored service with an
open door is a second incident.

### 4. Recover, and check what was reached

Bring it back, then answer the question the notification depends on: **whose
data was involved?**

For an S1 this is the whole task. Be specific — "the users table was readable"
is not an answer; "these 340 accounts' email addresses and display names were
readable between 14:02 and 14:31" is.

### 5. Write it up in public

Within a week, in `docs/postmortems/YYYY-MM-DD-slug.md`, and publish it. The
shape:

- **What happened**, in two sentences.
- **When**, as a timeline with times.
- **Impact**, specifically. Who and what, or "nobody, and here is how we know".
- **Root cause**, technical. Not "human error" — the reason the system allowed
  the error.
- **What we are changing**, with an owner and a date each.

No blame on a person. A person who made a mistake in a system that permitted it
is the second problem.

## Notification, when personal data is involved

**72 hours** from becoming aware, to the CNIL. That clock starts when you first
had reason to think data was exposed, not when you finished investigating.

The letter goes out before all the facts are in, and says so. It needs: what
happened, what data, roughly how many people, what the likely consequences are,
and what is being done. "We are still establishing the scope" is an acceptable
sentence in a first notification and a fatal one in a fourth.

People affected are told directly, in plain language, with what they should do —
change a password, watch for a phishing email. Not a paragraph of reassurance
with the facts in the middle.

`PRIVACY.md` is the reference for what data exists and who processes it, which
is what the letter needs.

## When the incident is a reported finding

Most incidents in this domain will arrive as a report rather than as an alert,
and the two processes have to meet:

1. The finding goes through the normal flow — triage, confirmation — because
   that is what credits the reporter and records the severity.
2. **In parallel**, if the finding is exploitable now, it is an S2 and this
   document applies. Do not wait for the disclosure process to finish before
   patching.
3. The reporter is a party to it. Tell them the fix shipped, and consider asking
   whether they can confirm it is closed. Nobody is better placed.
4. The embargo is separate from the incident. A patched finding stays embargoed
   until it is published, and the post-mortem may have to be written before the
   write-up can be.

## When the incident is caused by a researcher

It happens: somebody runs a load test, or takes a whole table to prove a read.

The safe harbour holds. Somebody acting in good faith who exceeded the scope
and told us is covered — that is what the commitment is for, and it is worth
most in exactly this case. Revoke the research token, tell them what happened,
and ask them to write up what they did, because it is now the best evidence
available.

It stops holding if they did not act in good faith, or did not tell us. That
judgement is a person's and it is recorded with its reason like every other
decision here.

## What has been tested

Honestly: the backup restore drill (`docs/BACKUP_RESTORE_DRILL.md`). Nothing
else on this page has been rehearsed against a real incident, and the first time
it is used, something in it will be wrong.

The cheapest way to fix that before it matters is a tabletop: read this
document out loud against a scenario, note every sentence that does not tell you
what to do next, and change those sentences. Twenty minutes, and worth doing
before the first user rather than after.
