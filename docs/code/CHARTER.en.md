# Code Charter

*Intended for publication at `skill-uv.com/code/charter`.*

This charter says what is required, what is refused, and what a validation
rests on. It binds: a deliverable that departs from it is refused, however
good the code.

It exists because until now it was implicit, scattered through general
documents. A rule nobody can quote is not a rule.

---

## 1. What a deliverable is

A deliverable is **evidence somebody can check**: something a stranger can
open, run and judge without taking your word for it.

Accepted:

- a contribution merged into a repository you do not control;
- a published project, with an address where it runs;
- a library published to a public registry;
- a tool other people use.

Not accepted:

- a tutorial exercise, however complete;
- a private repository described but not shown;
- a screenshot with no code behind it;
- a project where you are the only possible reviewer.

The difference is not difficulty. It is verifiability.

## 2. Three non-negotiable requirements

**Tests.** They describe the expected behaviour, not the implementation. A
test that breaks on a refactor with no behaviour change is a bad test and does
not count.

**Documentation.** A reader arriving at the repository must know what to run
and why the choices were made. **Code with no documentation is refused** — the
most underestimated rule here, and the one that turns away the most
submissions.

**A licence.** Work with no explicit licence is unusable by anybody. Pick one,
and respect the licences of your dependencies: MIT, Apache and GPL do not
impose the same obligations, and ignoring them exposes the host project as
much as you.

## 3. Contribution ethics

**Attribution.** Code you reused is credited. Reusing without crediting is
plagiarism, and plagiarism revokes the artefact and every attestation resting
on it.

**Respect for maintainers' time.** An upstream contribution is prepared: read
the project's rules, check whether the subject has already been handled, open
a discussion before writing a thousand lines. An unsolicited, unprepared pull
request costs time to somebody who asked for none of it.

**Commit hygiene.** A commit does one thing and explains it. A readable
history is a form of documentation.

## 4. AI assistance

Using an assistant is **accepted and declared**.

Accepted: these tools are part of the trade, and pretending otherwise would
produce false declarations rather than different practices.

Declared: the submission states the level of assistance — none, autocomplete,
pair programming, generated then reworked, generated as is. Hiding it is a
separate offence from using it, and it is the one that is sanctioned.

What is judged remains the result and your ability to answer for it. Defending
the work in real time — explaining a choice, changing the code in front of a
reviewer — is what settles it, not the declaration itself.

## 5. Validation

A validation rests on the **review grid for the relevant family of trades**,
public and readable before you submit. It covers checkable criteria, each with
a statement of what counts as met.

A rejection says which criterion is not met and what is missing. A rejection
with no actionable reason is not a valid rejection.

## 6. Revocation

A validated artefact can be revoked: plagiarism discovered, an upstream
contribution retracted, fraud established.

Revocation removes the artefact from the count — rank, badges, attestations
that rested on it. It does not erase the history: what was revoked stays
visible as revoked.

---

*See also: the [domain manifesto](./MANIFESTO.en.md), which says why these
rules are the ones they are.*
