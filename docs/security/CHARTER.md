# The security domain — what it is and what it refuses to be

## Why this domain is on a learning platform at all

Because the entry to it is broken. The trade asks for a certification that
costs between three hundred and eight thousand euros, or experience nobody will
give you without one, and the result is a profession that recruits from people
who could afford to wait. Meanwhile the work itself — reading a log, tracing an
injection path, writing a policy somebody can comply with — needs a laptop, a
container, and somebody willing to read what you produced.

The second list is what this platform can supply. The first is not our business.

## What we mean by proof

A stranger can check it.

That is the whole standard, and it is why nothing here is a self-assessment.
A confirmed finding was reproduced by somebody else. A published disclosure was
read by somebody who tried to follow it. A validated policy was read against
the framework it cites. A captured flag was compared against a hash nobody
holding it could see.

The corollary is uncomfortable and load-bearing: **a certification you paste in
moves no score.** It is shown on your profile, marked *declared* until somebody
opens the issuer's page, and it counts for nothing in a rank or a craft score.
Not because it is worthless — an OSCP is a real thing — but because the platform
did not see you earn it and will not pretend otherwise.

## The one rule above the others

**You work on what you were given written permission to work on.**

Every other line in this charter is a preference. This one is the difference
between the trade and the offence it resembles, and it is enforced rather than
requested:

- a security slice cannot be created without `security_authorisation_url`;
- an offensive mission cannot leave draft without rules of engagement;
- a finding against a host outside the published scope is refused at
  submission, however real it is.

Those are three database constraints and a service check. A charter that only
said "always get authorisation" would be a charter.

## What is worth more than what

The weights are rows in `craft_score_weights` and you can read them. The
editorial position they encode, in one line:

> One confirmed critical finding on a live system outweighs twenty solved
> capture-the-flag challenges.

180 points against 160, and the flags took a month. That is deliberate.
Training grounds are where you learn and they are not where you demonstrate:
the answers were planted, and a scheme that paid for finding them would fill
profiles with evidence of practice and none of work.

For the same reason, a captured flag and a passed lab produce an attestation
and **no `deliverables` row** — which means they move no rank. A weekend on a
range must not outrank a year of merged contributions.

## What this domain will not do

**No auto-grading of judgement.** A finding is not graded by a machine, and it
never will be. The hash comparison in a capture-the-flag challenge is grading a
planted answer, which is a different thing and is labelled as such.

**No leaderboard of severity.** There is a hall of fame ordered by findings, and
there is no ranking of people by how bad the worst thing they found was. That
metric rewards luck and punishes the person who audited a well-written codebase.

**No bounty this platform cannot pay.** There is no money. When there is, it
will be said in the same place as everything else, with the amount.

**No hosting of somebody else's licensed material.** Retired machines, forensic
datasets and vulnerable applications are linked, never rehosted, with their
attribution travelling with them.

**No exploit marketplace.** Not now and not later. This platform records that
somebody found something and that it was fixed; it is not a channel for selling
the one to people who do not want the other.

**No "we take security seriously" without a document behind it.** The trust
page publishes what is self-assessed as self-assessed and what is not started
as not started, and the compliance list will say `not_started` for years,
because that is true.

## What we ask of a reviewer

Read the grid before the submission. There is one per trade, they are public
before anybody submits, and every one of them has a line the reviewer is meant
to check rather than be convinced of — a replay, a query that fires, a path a
reader can follow, evidence an auditor would take.

And write the reason. Every refusal here carries one, by constraint. A reporter
told "no" and not why files the same report again next week, and is right to.

## What we ask of a researcher

Three things, and only the third is unusual.

1. Reproduce it. If a stranger cannot get to your result, you have a story.
2. Argue the severity from a vector, not an adjective.
3. **Say what you did not do.** Where you stopped, what you did not touch, what
   you believed was on the other side. It is the most valuable habit in this
   trade and the only one nothing can measure — a scope respected leaves no
   evidence — which is why the one badge nothing awards automatically is for it.

## Where this document is wrong

It will be. When it is, the code is the authority: the scope list, the
transition table, the weights and the grids are all readable, and every one of
them can be argued with by somebody who has read them. That is the point of
putting them in rows instead of in prose.
