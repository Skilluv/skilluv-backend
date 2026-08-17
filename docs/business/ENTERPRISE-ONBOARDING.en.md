# Onboarding an enterprise — the sequence

Written for a sales team that does not exist yet. That is deliberate: a
process written afterwards describes what was done, not what was meant.

**Today there is no team and no client.** This document is therefore a
hypothesis about the right way to go about it, to be corrected on first
contact with a real one.

---

## 1. The first call

One objective: **find out what problem the person has**, not place a product.

Four questions, in this order:

1. **What made you go looking for somebody today?** A departure, growth, a
   specific project that is stuck. The answer steers everything else.
2. **How are you recruiting at the moment?** What has worked, what has failed,
   how long it takes.
3. **What made you doubt a candidate last time?** This is the question that
   counts. The answer is nearly always "I had no way of checking what they
   said they could do", which is exactly the problem Skilluv solves.
4. **What is the deadline, and what is the budget?** Asked together, because
   either alone means nothing.

**What we do not do on the first call:** the demonstration. A demonstration
before understanding the need is a catalogue, and a catalogue convinces
nobody.

---

## 2. Recommending a product

| What they say | What they need |
|---|---|
| "We need one specific profile, for a role" | Search plus credits |
| "We are hiring several people this year" | Pipeline subscription |
| "We have a well-defined one-off piece of work" | Bounty or mission |
| "We have a large, vague piece of work" | Studios |
| "We want developers to know who we are" | Sponsorship, an event |
| "We want to understand the market" | A report, a data licence |
| "We do not know where to start" | Advisory, two days |

**The honest recommendation is sometimes "nothing".** A company looking for
thirty senior Java developers in two weeks will not find them here, and saying
so costs one client today and wins three later.

---

## 3. The demonstration

One thing to show, and it is not the interface: **a real profile with its
proofs**, and the verification link clicked live.

The moment that counts is opening `/verify/{code}` and having the page show a
contribution merged into a repository they recognise. What that says, without
saying it: this is not us asserting, it is checkable.

What not to do:

- show the feature list;
- show a profile invented for the demonstration — that is precisely the
  failure mode the product corrects;
- show an empty page while promising it will fill up.

If no real profile has enough proof to be shown, **it is too early for the
demonstration.** That is a signal, not an obstacle to work around.

---

## 4. Verification and setup

**Enterprise verification (KYC).** Legal existence, a person authorised to
commit the company, a payment method. Before any access to profiles: a
platform that lets an unverified account contact its members is a platform
whose members leave.

**Second factor mandatory** on every enterprise account. Not negotiable: a
compromised recruiter account is an entire address book.

**Technical setup**, where applicable:

- delegated authentication (SSO) — for enterprises that ask;
- account provisioning (SCIM) — beyond about ten recruiters;
- API and tokens — for integration with an existing tool;
- webhooks — to receive events rather than poll.

Allow half a day with a technical contact. With no technical contact, do not
offer an integration: it will not be finished and will leave the impression of
a complicated product.

---

## 5. First value

**One objective: that something real happens within seven days.**

A first guided search, a first message sent, or a first bounty posted. It does
not matter which — what matters is that they completed an action, on their
own, and that it worked.

An enterprise that has done nothing in the first seven days will do nothing.
It is the most reliable indicator there is, and it never improves by itself.

---

## 6. Thirty, sixty, ninety days

**Thirty days.** Have they used the product without us? If not, the problem is
the setup, not the interest. Go back to §5.

**Sixty days.** Has there been an outcome — an interview, a delivery, a bounty
paid? If not, find out where it is stuck: the profiles, the approach, the
product chosen.

**Ninety days.** Would they renew? Ask directly rather than waiting for the
date. A lukewarm answer at ninety days is a no at twelve months, and there are
three months left to fix it.

---

## 7. Expanding

One rule: **we only expand after an outcome.** Offering an annual programme to
a company that has yet to get anything is selling a promise on a promise.

The paths that make sense:

- credits → pipeline subscription, when the volume justifies it;
- bounty → mission, when the work grows;
- mission → Studios, when a team is needed;
- anything that worked → annual programme, at the first renewal.

---

## 8. Email templates

Four, short. A long sales email is not read.

**After the first call** — restate their problem in one sentence, propose one
thing, give a date.

**After the demonstration** — the verification link that was shown, and
nothing else. That is the piece that works on its own.

**Seven days with no action** — a question, not a chase: "what stopped you?"
You learn more from the answer than from ten meetings.

**Before a renewal** — thirty days out, with what happened over the period. A
renewal asked for with no review is a renewal that was not earned.

---

## 9. What we do not do

- **No chasing after two nos.** A third message turns "not now" into "never".
- **No promises about profiles that do not exist.** If the pool does not have
  what they are looking for, say so.
- **No discount for signing quickly.** A price that drops under pressure was
  not a fair price.
- **No commitment beyond twelve months** before a first full cycle has been
  delivered.
