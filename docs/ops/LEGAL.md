# Ops — production access, confidentiality, and on-call

What Skilluv requires before a mission touches somebody's production
infrastructure, what it puts in writing, and where it stops and a lawyer
starts.

**This is not legal advice.** It is the platform's position, written so
somebody can act on it and so a lawyer reviewing it has something specific to
correct. Where the platform is unsure, this document says so rather than
choosing the comfortable answer.

---

## 1. Why ops is different

Every other domain on this platform produces work that can be reviewed before
it does anything. A pull request sits in a branch; a design sits in a file; a
model sits in a repository. An ops mission is different in three ways, and
each one has a legal consequence:

1. **the work runs.** A Terraform apply changes something that exists.
   Reviewing it afterwards is reviewing the aftermath;
2. **it requires access.** Not a fork, not read access — credentials on
   somebody's production estate, usually with more permission than the task
   strictly needs, because scoping it precisely is work nobody wants to do at
   the start;
3. **the contributor learns things.** A network topology, an incident
   history, a list of what is held together with tape. None of that is
   deliverable, all of it is valuable to a competitor, and none of it can be
   given back.

Everything below follows from those three.

---

## 2. The reinforced NDA

A standard mutual NDA is not enough here, and the difference is worth
spelling out for the lawyer who drafts it.

**Duration.** Twenty-four months after the mission ends, not twelve. A
network topology and a list of unpatched systems stay useful to an attacker
for years, unlike a product roadmap that is stale in six months.

**What it covers, named explicitly.** A clause protecting "confidential
information" leaves an argument about whether infrastructure counts. The list
should name: network topology and addressing, credentials and their rotation
schedule, incident history, dependency inventory, cost figures, and the
identity of subcontractors. All of these are learnt incidentally and none is
a deliverable.

**What it must not cover.** The contributor's own general skill and
experience. A clause broad enough to stop somebody using Terraform for their
next client is a clause that stops them working, and it is the clause an
enterprise's standard template most often contains. Skilluv refuses missions
whose NDA has no such carve-out, and says why.

**Symmetry.** Skilluv holds the same obligation towards the contributor. What
is learnt about somebody's rate, their availability or their difficulties on
a mission does not leave the platform.

**Publication.** The default is that a contributor may say a mission
happened, name the type of work, and publish anonymised figures — "a cloud
bill reduced by 60%" — without naming the client. Anything narrower is a
negotiated exception with a stated end date, because a contributor who cannot
say what they did cannot build the portfolio this platform exists for.

---

## 3. Access: delegated, temporary, audited

Three properties, none optional.

**Delegated.** Access is granted by the client, on the client's own identity
system, in the contributor's own name. Never a shared account, never
credentials passed through Skilluv. Skilluv does not want to hold anybody's
production credentials and should never be asked to.

**Temporary.** Access ends when the mission ends, and the mission brief says
who removes it. "When someone remembers" is the state most engagements are
actually in, and it is how a former contractor's key stays live for a year.

**Audited.** The client's own audit log is the record. The contributor should
ask, at the start, whether their actions are logged — not because they intend
anything, but because an unlogged estate makes it impossible to prove they
did not do the thing that broke.

Skilluv's own position: **the platform never holds production credentials on
behalf of a client, and never brokers them.** If a mission asks for that, it
is refused. This is the clearest line in the domain and it should stay
clear.

### The same line, applied to monitoring

A read-only key to a client's observability stack looks like a lesser thing
than a production credential, and it is not. It carries the service map, the
dependency graph, the incident history, the traffic volumes, and — because
logs and traces carry user identifiers — personal data. That is the same list
this section exists to protect, obtained more quietly.

So the platform reads **only what the operator already published to the whole
internet**: a public status page, queried with no credential. Anything behind
an authentication prompt stays declared, sourced and reviewed by a person, and
what is read publicly is shown beside the claim rather than in place of it.

If an enterprise ever asks for the authenticated integration, the answer is
not automatically no — but it is a conversation with a real case in front of
it, a data-processing agreement, and someone who can say who rotates the key
and who is accountable if it leaks. Until then the feature does not exist, and
its absence is a position rather than a gap.

---

## 4. Compliance frameworks

An enterprise under SOC 2, ISO 27001 or HIPAA does not merely prefer certain
practices; it has told an auditor it enforces them. A contributor who works
outside them creates a finding, and the finding lands on the client.

The mission brief must state which frameworks apply. Where one does, the
practical consequences are usually: background checks or identity
verification before access, access reviews on a schedule, change management
with approval recorded, and data residency limits.

Skilluv's position, said plainly: **the platform does not certify compliance
and does not audit it.** What it does is make the requirement visible in the
brief, so a contributor knows before applying rather than after being
refused. Claiming more than that would be selling an assurance nobody here
can stand behind.

Where personal data is involved — and incident data usually is, because logs
carry user identifiers — the mission is a data-processing arrangement and
needs the corresponding agreement. That is a lawyer's document, not this one.

---

## 5. On-call retainers

Being reachable is work, whether or not anything happens. A contract that
pays only for interventions pays somebody nothing for the constraint of not
leaving town, and it is the arrangement contributors most often accept and
most often regret.

What an on-call clause has to state:

- **the window.** Hours and days, in a named timezone. "Evenings" means
  different things three time zones apart, and this domain is remote by
  default;
- **the response time.** How long after a page the contributor must
  acknowledge. Acknowledge, not resolve — a clause promising resolution in
  thirty minutes is a clause nobody can honour;
- **what counts as a page.** Which alerts justify waking somebody. Without
  this, an alert added later quietly widens the obligation;
- **the retainer.** Paid for the availability itself, per period, whether or
  not anything fires;
- **the intervention rate.** Paid on top, per call, usually higher at night;
- **the backup.** Who is called when the contributor does not answer. A
  rotation of one is not a rotation, and a contract that pretends otherwise
  makes one person permanently unavailable to their own life;
- **the end.** Notice period on both sides. An on-call arrangement with no
  exit is indefinite servitude with a monthly payment.

Skilluv refuses a mission that includes on-call without naming a retainer.
This is not negotiable and it is not a preference: unpaid availability is the
single most common way this trade is exploited.

---

## 6. Responsibility without authority

A mission that asks somebody to hold an availability objective for a system
they are not allowed to change is refused at brief review.

This is a legal position as much as a professional one. Accepting
accountability for an outcome one cannot influence is accepting liability for
somebody else's decisions, and when the system falls over the conversation
about whose fault it was starts from a contract that already said it was the
contributor's.

---

## 7. What is still missing

Written down rather than left implied, because the platform has said
elsewhere that no ops mission with production access opens before these
exist:

- the reinforced NDA template itself, drafted against section 2;
- the on-call retainer template, drafted against section 5;
- both reviewed by a lawyer, and by one who has seen an infrastructure
  engagement before;
- a data-processing agreement template for missions touching incident data.

Until then, ops missions run on artefacts a contributor builds in their own
environment — modules, charts, dashboards, runbooks — which is most of the
domain and all of the portfolio-building half of it.
