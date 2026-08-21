# The Skilluv Ops Charter

Ops is the domain where a mistake is not undone by rewriting a function. A
misconfigured cluster, a migration run badly, an on-call rotation nobody can
hold: what breaks is in production, at somebody's, immediately.

This charter says what Skilluv expects of a person working in this domain,
and what Skilluv owes them in return.

---

## 1. The eight trades

| Trade | What it does |
|---|---|
| DevOps Engineer | CI/CD, containers, infrastructure as code |
| Platform Engineer | The internal platform everybody else ships on |
| Kubernetes Specialist | Operators, service mesh, GitOps |
| SRE | Service objectives, error budgets, resilience |
| Incident Commander | Runs the response, writes the post-mortem |
| Cloud Architect | Design, cost, multi-region |
| Observability Engineer | Metrics, logs, traces, and what connects them |
| Database Administrator | Replication, tuning, recovery |

Five review families group them, because somebody who reads a Terraform plan
reads a Helm chart and has no useful opinion on a query plan.

## 2. What counts as proof

Not a pull request. In this domain, proof is:

- **a reusable artefact** — a module, a chart, a pipeline, a dashboard, a
  runbook. Judged on one question: can somebody else use it without its
  author in the room;
- **an objective held** — a target announced, a window, a figure reached, and
  the source of the figure;
- **an incident led** — with both durations and a published post-mortem;
- **a cost reduction** — both amounts, what was changed, and confirmation
  that the service still stands.

Each of these is recorded with what it rests on. "Reliable" is not a proof;
"99.95% over ninety days, public dashboard" is.

## 3. Post-mortems are blameless, and that is a constraint

There is no column, anywhere, to record who caused an incident. This is not
an editorial policy: the schema does not allow it.

The reason is practical rather than moral. A post-mortem that names somebody
is one nobody writes honestly the next time, and it is the next time that
matters. What is recorded is what the system allowed.

Two requirements:

- **two hundred characters minimum.** A shorter post-mortem is a title, and
  the second occurrence of the same incident is what it costs;
- **at least one follow-up action.** A post-mortem concluding nothing to do
  either found a system that cannot fall over, or did not look.

Promised actions that are overdue are visible. That is what separates a
post-mortem practice from an archive of post-mortems.

## 4. What Skilluv reads, and what it will never read

An objective closes with a figure its own author typed, and an address where
that figure can be checked. The obvious way to automate the checking would be
an API key to the client's monitoring — Datadog, Instana, a private Grafana.
**Skilluv will not do that.**

Such a key does not give "was the service up". It gives the map of their
services and how they depend on each other, their incident history, their
traffic volumes, and often user identifiers in the logs. That is word for word
what the reinforced NDA protects. Holding it for several clients at once would
make this platform worth attacking for what it knows about other people rather
than for what it knows about itself.

So the rule is simple:

- **what is already public is read automatically.** A status page anybody can
  open is queried with no credential at all, and what it published is shown
  next to the declaration;
- **anything behind a credential stays declared, sourced, and read by a
  person.**

What is read publicly does not replace the declared figure and does not claim
to: a status page shows only the outages its operator chose to publish. What
it gives a reviewer is the other half of the conversation. Somebody announcing
99.99% over a window in which their own public page shows eleven hours of
major outage has not lied to a machine — they have written something a reader
can now see does not add up.

## 5. Cost is a skill

Cutting a bill by 60% is engineering work, exactly as holding an availability
objective is. Skilluv attests it, on one condition: that somebody verified
the service still stands.

A cost reduction that broke the service is an outage with a spreadsheet. The
verification covers both halves or neither.

## 6. What Skilluv expects

**Security by default.** A module that opens a port for convenience, a secret
in a repository, a role too wide "for now": all three are refused at review,
with no discussion of context.

**Respect for what was promised.** An announced objective is a promise made
to somebody. Missing it happens and gets said; redefining it afterwards so it
was met does not.

**What runs gets documented.** A runbook is not accompanying documentation,
it is the deliverable. The test is the one in section 2: somebody else, at
three in the morning, without its author.

## 7. What Skilluv owes

**Bounded access.** An ops mission grants access to production
infrastructure. That access is temporary, logged, and removed at the end —
not when somebody remembers.

**Paid on-call.** Being reachable is work. A mission that includes on-call
says so and pays for it; one that does not say so does not include it.

**No responsibility without authority.** Nobody holds an availability
objective for a system they are not allowed to change. A mission asking for
one without the other is refused at brief review.

**Confidentiality, both ways and bounded.** A network topology learnt on a
mission is not retold. What Skilluv requires of a contributor, Skilluv holds
towards them.

---

## 8. What is left to do

- draft the reinforced NDAs for missions with production access, and the
  on-call retainer contracts (ticket L-01);
- publish the review grids of the five families;
- have both reviewed by a lawyer.

No ops mission with production access opens before those documents exist.
