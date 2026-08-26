# Legal — what is drafted, what is reviewed, and what is missing

**Nothing on this page has been reviewed by a lawyer.** That sentence is the
most important one here, and it is repeated on every document it applies to and
returned by the endpoint that serves them. The failure mode of self-drafted
legal text is not that it is bad; it is that everybody downstream assumes
somebody checked.

Read this before signing anything, and before publishing a mission that asks
somebody else to.

---

## 1. Safe harbour

**Status: written, unreviewed, and relied on.**

`SCOPE.md` contains a commitment not to pursue anybody who follows it in good
faith, including somebody who crosses the boundary by accident and says so. It
follows the disclose.io baseline.

What it is not: a defence under any particular jurisdiction's computer misuse
law. It is this platform's undertaking, which is worth what this platform's word
is worth, and it does not bind a third party whose system somebody reaches by
pivoting. That is why the scope forbids pivoting.

**What a review needs to settle:** whether the wording is enforceable as an
undertaking under French law; whether it can be given at all before the
operating entity exists (see §5); what happens to it if the entity changes.

## 2. Confidentiality agreements

**Status: two drafts, unreviewed, in the database.**

`mission_nda_templates` holds two, in English and French:

- `mutual_standard` — mutual, two years, with an explicit carve-out letting the
  contributor describe the engagement in general terms.
- `mutual_extended` — five years, no public description beyond type and
  duration, a six-month non-solicitation clause and a reference restriction.

Both carry `is_reviewed = FALSE` on the row.

### What the signature is, legally

A **simple electronic signature** under eIDAS: the signer is authenticated, is
shown a document, accepts it, and what is recorded is the SHA-256 of the exact
bytes shown, the time, the address where there is a trustworthy one, and the
name they typed. Admissible and rebuttable — the lowest of the three tiers.

It is **not** an advanced or qualified signature. Self-hosting DocuSeal would
produce the same tier through more moving parts; a commercial provider
(Yousign, DocuSign) would produce the tier above at roughly 1–5 € per
signature. Neither is refused for later. What is refused is pretending the
difference does not exist.

The document hash is the load-bearing part. Without it a signature proves that
somebody clicked yes, and the document can be substituted afterwards.

### The clause most likely to be wrong

Non-solicitation, in the extended template. It restricts *engagement* rather
than people, for six months, and the reason is written into the clause itself:
the platform's costs are paid by commission on missions it arranged. Six months
is a guess. Whether it is enforceable at all against an individual in France is
exactly the sort of question a review exists to answer.

## 3. Intellectual property on a paid engagement

**Status: implemented, unreviewed.**

`missions.ip_terms` has five values, and the security-relevant reading is:

| Value | Who owns what |
|---|---|
| `full_ownership_client` | Client owns the report and the findings outright |
| `retain_reusable_components` | Client owns report and findings; the contributor keeps methodology and tooling |
| `licence_to_client` | Contributor keeps the report; client gets a right to use it. Rare, usually priced lower |
| `open_source_output` | The deliverable is published |
| `dual_license` | Both |

The default for security work is `retain_reusable_components`, which matches how
the trade actually works: the report and the findings are facts about the
client's system; the scanner configuration and the custom script are not.

**Disclosure is separate from ownership**, and deliberately: `missions`
carries `allows_public_disclosure` and `credits_researcher_in_disclosure` as
their own booleans, because a client can own a report outright and still permit
a redacted write-up once the fix has shipped. Folding that into the ownership
enum would have made a common pair inexpressible.

**What a review needs to settle:** whether "methodology and tooling remain the
contributor's" survives contact with a client's own contract; whether the
default should be the default.

## 4. Disputes

**Status: implemented on the generic mission machinery.**

A disagreement about a paid engagement goes through `disputes` and
`mission_arbitrations`: either party raises it, the other concedes or contests,
and an administrator or a `mission_arbiter` decides with a written reason that
goes into the mission's history.

The decision is applicable and final. This platform is not a court and does not
offer an appeal — and it says so rather than implying a process it cannot run.
Nothing in it removes either party's right to go to an actual court.

Five dispute shapes are expected: deliverable insufficient, late, scope creep,
non-payment, confidentiality breach. The first three are decided on the mission's
own documents, which is why the acceptance criteria and the rules of engagement
are required fields.

## 5. The operating entity

**Status: missing, and the largest gap on this page.**

There is no company yet. That has three consequences somebody should be told
before they rely on any of the above:

1. **Contracts have no counterparty.** An agreement signed "with Skilluv" is
   currently an agreement with an individual.
2. **No professional liability insurance.** See §6.
3. **Invoicing and VAT are not set up** for a marketplace. See §7.

Until it exists, the honest position — and the one taken — is: no money moves
through the platform, missions are not being solicited from clients, and the
disclosure programme offers recognition rather than payment. Every one of those
is stated where a user reads it and not only here.

## 6. Insurance

**Status: researched, not bought.**

A platform that arranges security engagements is exposed if a contributor
destroys data, discloses something, or causes an outage during a test. Without
insurance that exposure is personal and unbounded.

The market, as at 2026:

| Option | Rough annual cost | Note |
|---|---|---|
| Specialist cyber liability (Hiscox and similar) | 5–10 k€ | Designed for this; needs a company |
| General professional indemnity (AXA Pro and similar) | 2–5 k€ | Cheaper, and the cyber cover is usually thin |
| Insurtech (Wakam and similar) | Varies | More flexible; read the exclusions twice |
| Individual professional indemnity | ~500 €/year | What is available without a company. Very limited cover |

Minimum cover worth having: third-party damage, professional error, an incident
on staging caused by a researcher, and data-breach costs — notification,
forensics, communication.

**Decision: not now, because the entity does not exist.** Until then, the
mitigation is structural rather than financial: no paid missions are being
brokered, engagements would be capped in value, and clients would be told in
writing that the platform is in beta and its legal structure is in progress.

## 7. Invoicing, VAT and reporting

**Status: partially implemented, unreviewed.**

`mission_invoices` and `invoice_counters` exist, with sequential numbering per
year and no gaps — which is a legal requirement in France and the reason the
counter is a table and not a `count(*)`.

What is implemented: invoice rows, sequential numbers, a monthly CSV export for
an accountant, and the Stripe Connect payout machinery from P13.

What is not, and needs the entity first:

- **VAT.** 20 % for France and the EU without a valid VAT number; reverse
  charge with one; outside the EU, out of scope. Validation against VIES is not
  wired up.
- **Invoices issued on behalf of a contributor** who is a French
  auto-entrepreneur, with the "TVA non applicable, art. 293 B du CGI" note.
  This is the Uber and Malt pattern and it needs the contributor's SIRET.
- **DAS-2** — the annual declaration of fees paid to each contributor above
  1 200 €.
- **DEB/DES** for intra-EU transactions.

## 8. Personal data

`PRIVACY.md` at the repository root is the document, and it is also an audit
exercise in this domain's catalogue — a platform that publishes a privacy notice
and does not want it read is publishing decoration.

The two things specific to this domain:

- **A proof file may contain personal data.** A screenshot proving an IDOR
  contains somebody else's record. Proofs live in the private bucket, are served
  through signed one-hour links to four roles only, and are deleted thirty days
  after nothing references them.
- **A defensive lab artefact contains addresses and may contain more.** The
  redaction is an operator's judgement and is confirmed explicitly before a
  generated lab can be created. Addresses are kept because they are the object
  of the analysis; credentials and anything identifying somebody who was not
  part of the attack are not.

## 9. What to ask a lawyer, in priority order

If there is a budget for two hours, spend it here:

1. **The safe harbour wording** — is the undertaking in `SCOPE.md` enforceable,
   and can it be given before the entity exists?
2. **The two confidentiality templates**, particularly the non-solicitation
   clause and its duration.
3. **The `retain_reusable_components` default** and whether it holds.
4. **The dispute process** — is "applicable and final" a statement this
   platform may make?

Priority two, once there is money involved: terms of service for the
marketplace, the invoicing-on-behalf-of arrangement, and escrow. Holding third-party
funds in France attracts ACPR attention above a certain volume, and that is a
question to ask before the volume arrives rather than after.

Roughly: an independent lawyer reviewing the templates is 200–500 €; a firm
specialising in technology law is 1 500–3 000 € and would answer §5 as well.

## 10. When this page is out of date

Every "unreviewed" above becomes "reviewed on {date} by {who}, changes in
{commit}" when it happens, and the corresponding `is_reviewed` flag on the
template row is set in the same change. A reviewed document with a stale flag is
worse than an unreviewed one, because the flag is what the endpoint tells the
signer.
