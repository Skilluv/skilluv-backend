# Privacy

What Skilluv holds about you, why, for how long, and how to get it out or have
it deleted.

**Status: written by the people who built the platform. No lawyer has reviewed
it.** See `docs/security/LEGAL.md` for what that means and what a review would
settle first. This document is also an audit exercise in the security
catalogue — a platform that publishes a privacy notice and does not want it read
is publishing decoration.

Last substantive change: with the security domain, 2026.

---

## Who is responsible

Skilluv, currently operated by an individual rather than a company. There is no
legal entity yet and no separately appointed data protection officer; the
contact for anything on this page is **security@skill-uv.com**, which is read by
the person who operates the platform.

That is unusual and it is stated rather than dressed up. When the entity exists,
this section changes and the change is in the git history.

## The lawful bases

| What | Basis | Article |
|---|---|---|
| Your account, your work, your attestations | Performance of a contract — you asked us to run this for you | 6(1)(b) |
| Security, fraud prevention, rate limiting, audit logs | Legitimate interest — a platform that cannot tell an attack from a user cannot protect either | 6(1)(f) |
| Recruiter access to your public profile | Legitimate interest, and you can turn the profile off | 6(1)(f) |
| Analytics, marketing email, dataset licensing | Consent, per purpose, revocable | 6(1)(a) |
| Invoices and accounting records | Legal obligation | 6(1)(c) |

The consent purposes are rows in `data_purposes` and each is opt-in
separately — public score API, academic research, commercial licensing, unified
profile. `GET /api/legal/consent-version` says which version you agreed to;
`POST /api/legal/consent` changes it. Withdrawing consent stops the processing
and does not delete the account.

## What is collected

**Because you created an account:** email address, username, display name,
password hash (Argon2id — never the password), and whatever you chose to put on
your profile.

**Because you did work here:** your submissions, deliverables, reviews you gave
and received, attestations, badges, rank history, orientations, and the
challenges you attempted.

**Because the platform has to run:** session records with the address and user
agent they were created from, an audit log of administrative actions, rate-limit
counters in Redis (which expire on their own), and error reports.

**If you are paid through the platform:** the identity documents required for
payouts, held in a private object store, and payment records. Card details are
never seen by this platform — Stripe holds those.

**If you file a security report:** the report, the proof files you upload, and
the address the requests came from if you declared a research token. Proof files
are covered separately below.

**If you connect something:** a GitHub token, a portfolio handle, a Discord id —
each only after you link it, and each removable.

**What is never collected:** your location beyond a country you typed, anything
from a tracker on another site, and biometrics.

## Who else sees it

| Processor | What they get | Why |
|---|---|---|
| **Hetzner / Coolify** | Everything, as the hosting | The servers |
| **Cloudflare** | Request metadata, addresses | DNS, TLS, denial-of-service protection |
| **Brevo** | Email address and the message | Transactional email |
| **Stripe** | Name, email, payout details | Payments and payouts |
| **Sentry** | Error reports, which can include a user id | Knowing when something broke |
| **GitHub** | Only what you make public by contributing | Where the work happens |

No processor is used for advertising, and none receives your data for their own
purposes.

## How long

| Data | Kept |
|---|---|
| Account and work | While the account exists |
| Sessions | 30 days after last use, then deleted |
| Rate-limit counters | Minutes to an hour. Redis expires them |
| Audit log of administrative actions | 3 years — it exists to answer "who did that" |
| Invoices and accounting records | 10 years, as French law requires |
| Identity documents for payouts | 5 years after the last payout, as anti-money-laundering rules require |
| Security proof files | 30 days after nothing references them, then deleted by a daily sweep |
| Published attestations and disclosures | Indefinitely, because their point is to be checkable later |

That last row is the one to read twice. **A published attestation and a
published disclosure survive account deletion**, with your name on them if you
did not ask for anonymity. They are public records of verified work, other
people rely on them, and a platform whose proofs can be retracted has no proofs.
Everything else about you goes.

## Your rights

| Right | How |
|---|---|
| **Access and portability** | `POST /api/auth/me/data-export` — a machine-readable archive of everything, emailed to you |
| **Rectification** | Edit your profile, or ask |
| **Erasure** | `DELETE /api/auth/me` — requires your password again. Deletes every row naming you, except the published records above |
| **Restriction and objection** | Withdraw a consent purpose, or turn off the public profile |
| **Complaint** | The CNIL, in France, and you do not have to talk to us first |

Erasure is real deletion, not a flag. What survives it is listed above and
nowhere else.

## Security-specific processing

Two things in this domain that no other part of the platform does, and both are
worth saying plainly.

### Proof files can contain other people's data

A screenshot proving an IDOR contains somebody else's record. That is
unavoidable — it is the proof — so:

- proofs go to the **private** object store, never the public one;
- download links are minted per request and expire in an hour;
- only four roles can read one: the reporter, a triager, a security reviewer,
  and an administrator. Not the owner of the system under test, who is told
  about the finding through the disclosure rather than handed the reporter's raw
  evidence;
- the report asks you to take **no more than proves the finding**, and that is
  a rule of the programme rather than advice;
- a proof nothing references after 30 days is deleted automatically.

### Defensive lab artefacts contain addresses

A generated exercise built from a real attack on this platform contains request
logs. Addresses are kept, because they are the object of the analysis;
credentials, tokens and anything identifying somebody who was not part of the
attack are removed before the artefact is published, and an operator has to
confirm they have done that before the exercise can be created.

If your address appears in one because you attacked us under the disclosure
programme, that is you, and you can ask for the exercise to be withdrawn.

## Breach notification

If personal data is exposed, the CNIL is told within 72 hours and anybody
affected is told directly, with what happened and what to do. `INCIDENT_RESPONSE.md`
is the runbook, including who decides and what goes in the letter before all the
facts are in.

## Transfers outside the EU

Stripe and Cloudflare process data outside the European Union under standard
contractual clauses. Everything else stays on European infrastructure.

## How to challenge any of this

The security domain has an audit exercise for exactly that: read this document
against the GDPR, article by article, and report what is missing, wrong or
unevidenced. A finding against a published policy is as real as a finding
against the code, and there is an attestation for it.

Or just write to **security@skill-uv.com**.
