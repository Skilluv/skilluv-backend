# Quality — brief templates

Backlog: quality/F-05. What a client or a challenge author fills in when they
want quality work done. The **write-up** templates — what the contributor hands
back — are rows in `content_guides` (`kind = 'writeup_template'`, migration
0457); these are the other end of the same conversation.

Five briefs, one per trade. The shared skeleton first, then what each trade
needs that the others do not.

---

## The shared skeleton

Every quality brief answers these, in this order:

1. **What system, and what version.** A brief with no build identifier
   produces a report against something that no longer exists.
2. **What question is being answered.** One. A brief with three questions gets
   three shallow answers.
3. **What is in scope, and what is explicitly out.** The second list is the one
   that prevents the argument at the end.
4. **What is handed back**, in which format. Pick from
   `mission_deliverable_formats` for the quality domain.
5. **When.** And what happens to the fee if the system is not ready on the
   start date, which is the single most common way these engagements go wrong.
6. **Who receives it**, and who can answer a question during the work.
7. **Confidentiality.** NDA yes or no, and whether the contributor may state
   publicly that the engagement happened.

If a brief cannot answer 1, 2 and 3, it is not ready and the contributor should
say so rather than start.

---

## `brief-qa-code` — test plan or suite

Beyond the skeleton:

- **What already exists.** A suite, a partial one, or nothing. And whether it
  currently passes — taking over a red suite is a different job and should be
  priced as one.
- **Where it has to run.** The client's pipeline, with its constraints: run
  time budget, parallelism, whether a database is available.
- **What the team will maintain afterwards.** A suite handed to a team that has
  not agreed to own it is a suite that is deleted in six months, and the brief
  should say who is taking it.
- **Coverage target, if any** — and the warning that goes with it: a
  percentage target produces tests that satisfy the percentage. Prefer naming
  the paths that matter.

**Not in this brief:** which framework. That is the contributor's call unless
the client's pipeline forces one, and forcing one for preference costs the
engagement its best option.

## `brief-qa-cyber` — scoped security testing

Beyond the skeleton:

- **Signed rules of engagement**, or the statement that they will be signed
  before anything starts. Nothing else in this brief matters without it.
- **Authorisation source.** Who is signing, and their authority over the
  system. A signature from somebody who does not own the system is not
  authorisation.
- **Techniques not permitted.** Denial of service, social engineering,
  physical access, testing against production data — each answered yes or no
  rather than left to judgement.
- **Credentials provided**, and at which privilege levels. An unauthenticated
  test and a test with an admin account answer different questions.
- **Disclosure timeline.** Remediation window, and the date after which the
  contributor may publish an anonymised account.
- **Escalation contact**, reachable during the window.

## `brief-qa-design` — usability study or accessibility audit

Beyond the skeleton, and these two are different enough that the brief picks
one.

**For a study:**

- **Who the participants should be**, and who is recruiting them. If the client
  is, the brief says by when; if the contributor is, the brief says the budget
  for compensating them.
- **The tasks**, or the intent behind them. Realistic tasks, and the brief
  should resist writing the instructions — a task worded by somebody who built
  the product gives away the answer.
- **Number of sessions**, and what the client expects to conclude from that
  number.
- **Consent and data handling**: who holds the recordings, for how long, and
  what the contributor deletes at the end.

**For an audit:**

- **Standard and level.** WCAG 2.2 AA, or say which. "Accessible" is not a
  target.
- **Assistive technologies to cover**, and on which platforms.
- **Whether remediation is in scope** or only the findings.

## `brief-qa-game` — playtest facilitation

Beyond the skeleton:

- **Build, and what is knowingly broken in it.** Playtesters wasting a session
  on a known bug is the client's money spent on nothing.
- **Player profile wanted**: familiar with the genre or not, and why.
- **How many sessions**, and whether they run before or after a planned change.
- **What the team wants to decide.** A playtest with no pending decision behind
  it produces a report nobody reads. The brief should name the argument the
  team is currently having.
- **Embargo.** Unreleased games mean participants see something confidential;
  the brief says what participants are told and what they sign.

## `brief-qa-lead` — strategy or quality initiative

Beyond the skeleton:

- **The three numbers**, if the client has them: suite duration, share of last
  month's failures that were real, time from ready to merged. If they do not,
  measuring them is the first phase and should be priced separately.
- **Team size and shape.** Ten people in one room and ten across four time
  zones need different strategies.
- **What has already been tried**, and why it stopped. Most quality
  initiatives are the third attempt, and the brief that hides that gets the
  second attempt again.
- **Who can decide.** A strategy needs somebody with authority to accept the
  omissions. If nobody in the engagement can, the deliverable is a proposal
  rather than a strategy, and the brief should say so.
- **Retainer or fixed price.** Strategy work that ends at delivery is strategy
  work that does not land; the retainer option exists for that reason.

---

## The clause every brief should carry

> The contributor will state what was **not** covered. A report that lets a
> reader assume full coverage is refused in review, so a brief that pushes for
> one is a brief that will not be delivered against.

Putting it in the brief rather than only in the review grid saves the
conversation happening at the end, when it is expensive.
