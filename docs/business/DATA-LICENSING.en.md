# Data licensing policy

What Skilluv agrees to sell from the data it holds, to whom, and what it
refuses.

**This policy outranks any commercial negotiation.** A contract that
contradicts it is not an exception, it is a contract not to sign.

---

## 1. What can be licensed

Three categories, and a fourth that does not exist.

**Aggregated statistics.** Distribution of skills by country, by language, by
trade. No row concerns an identifiable person. Minimum twenty people per cell:
below that, a statistic about three people names three people.

**Trends.** How the skills in demand, the technologies used and the delivery
times evolve. Same threshold.

**Pay ranges.** What missions pay, by trade and by region. Aggregated, never
by name, never tied to an identifiable employer.

**Individual profiles: only with the explicit consent** of the person
concerned, with a share of the revenue for them (§4). Without that consent no
profile leaves — not anonymised, not pseudonymised, not at all.

### The fourth category, the one that does not exist

Anything that would let a person be reconstructed from supposedly anonymous
data. A dataset "anonymised" to country, trade, year of first contribution and
three languages often names exactly one person. Any request whose granularity
approaches that threshold is refused, even when each field taken alone is
innocuous.

---

## 2. Consent

For anything touching an individual profile:

- **explicit** — a box unticked by default, never consent inferred from
  signing up;
- **informed** — the person sees which data, to which kind of buyer, and what
  they receive;
- **revocable at any time**, with effect on licences in force: a buyer
  receiving updates stops receiving that row, and is contractually bound to
  delete it within thirty days;
- **logged** — every licence granted is recorded and the person concerned is
  notified.

Consent to a data licence is never a condition of access to anything else. A
talent who refuses keeps exactly the same functionality.

---

## 3. Who may buy, and who may not

### Acceptable buyers

- **Academic and public research** — free, in exchange for publishing the
  results.
- **Recruitment software vendors**, to enrich a product, with no resale.
- **Financial institutions**, for aggregated market analysis.
- **Public bodies and funders**, for training policy.
- **Insurers**, for aggregated pricing only.

### Refused buyers

Without discussion, and the list is in the contract:

- **mass surveillance** — any purpose involving tracking people who are not
  suspects;
- **discrimination** — any use aimed at excluding on origin, nationality,
  gender, age, health, orientation or membership;
- **identification for a coercive measure** — immigration enforcement,
  applying criminal law from skills data;
- **resale**, in any form;
- **training models** without specific and separate consent.

A buyer who refuses to declare their purpose is refused. A purpose that
changes after signature ends the contract without refund.

---

## 4. The talents' share

**0.5% to 2% of licence revenue**, paid to the people whose data is in the set
concerned, pro rata.

The rate depends on granularity: the more individual the data, the higher the
share. A statistic where one person is one thousandth pays little; a profile
named in a recruitment dataset pays more.

The amounts are small — saying so is more honest than presenting it as an
income. What matters is not the sum, it is that **somebody's data never earns
for the platform alone.**

Paid into the talent's wallet, visible on their statement, naming the licence
that produced it.

---

## 5. GDPR and the right to erasure

Skilluv processes data about people in the European Union and in West Africa.
The legal basis for licensing is **consent**, which carries the right to
withdraw it, at any time, without justification.

In practice:

- an erasure request is handled within thirty days;
- it is **propagated to licensees**, who are contractually bound to delete;
- data already aggregated into a published report cannot be recovered, which
  is told to the person **before** they consent, not after they ask;
- the public attestation is the exception: it is an issued proof whose whole
  point is verifiability. Its holder can revoke it, which makes it
  unverifiable — but not retroactively erased from the places it was shown.

---

## 6. The log

Every licence granted writes a line: which buyer, which data, which declared
purpose, which date, which duration, which amount, which share paid.

That log is readable by any person whose data appears in it, for the part
concerning them. A data policy with no readable log is a statement of
intent.

---

## 7. What the tooling guarantees

This policy was written before its implementation, deliberately, because the
other order produces tooling that decides the policy. The implementation now
exists, and here is what it makes impossible rather than discouraged.

**Consent is per purpose.** Four distinct purposes (public score API,
academic research, commercial licence, unified profile), one row per person
per purpose. Agreeing to appear in a score API agrees to nothing else. There
is no route that grants consent on somebody's behalf.

**The wording agreed to is copied onto the consent row.** A purpose's
description will be reworded over time; consent given to the old wording was
not given to the new. What can be produced in an audit is what was actually
on screen.

**A withdrawal keeps the row.** A revoked consent proves consent existed for
the period a dataset was built in, and deleting it would make that
unprovable in exactly the audit where it matters.

**The covered population is read fresh at every settlement**, never copied
into a list. Somebody who withdrew last week is not paid and is not in the
delivered set.

**A floor of thirty people.** No report, licence or statistic may rest on a
smaller population. A "skills gap in Cotonou" chart drawn from four people
names those four whatever its title says — and the commercial pressure runs
exactly that way, which is why the floor is in the code and not in a style
guide.

**A commercial licence at 0% is refused by the database.** Zero is defensible
for a public research dataset; it is not for a sale.

**The revenue-share ceiling is 20%** in the schema, the default 1%, and the
band published in section 4 remains 0.5–2%. The ceiling exists so a
negotiation cannot write an absurd number, not to be reached.

**The public API says nothing about somebody who agreed to nothing** — and
answers "not found" rather than "private". A directory built from refusals
would be a directory of everybody who declined, which is still information
they did not agree to share.

**Official recognition requires a signed contract.** A government instance
cannot declare it recognises anything without the signed convention attached:
without it the claim is just a claim, and the people carrying the attestation
are the ones who would find out it was worthless.
