# Communication — rights, disclosure and generated content

*Ticket L-01. To be reviewed by a lawyer before the first paid commission is
published; what follows is the platform's position, written so the review has
something to correct rather than something to invent.*

The full legal review is shared with the other domains' L-01 tickets — the
questions overlap enough that four separate engagements would be four times
the cost for one set of answers. This document is what this domain adds.

---

## 1. Who owns what

**Default: the author keeps everything.** Work delivered against a challenge,
a terrain or an open contribution belongs to whoever wrote it, under whatever
licence the upstream project requires.

**A commission changes that only if it says so, in writing, before the work
starts.** `missions.ip_terms` carries the four possibilities the platform
supports, and a mission with none is unpublishable.

The one term that never transfers is the right to show the work existed. A
commission may take the copyright; it may not take the author's ability to say
they wrote it. Where a client genuinely needs that — ghostwriting is a real
arrangement — it is `buyout` on the licensing scope, `permits_portfolio_use`
is false, and the platform records that the work exists without saying what it
is.

## 2. Ownership and licence are two questions

`ip_terms` says who owns it. `licensing_scope` says what the client may do
with it. A writer who keeps the copyright still has to know whether the client
may:

- syndicate the piece to three other publications;
- translate it into five languages;
- run it under an employee's byline;
- keep it up for two years or for ever.

Every communication mission states a scope. It is the most common way a
commission goes wrong in this domain: the client assumed worldwide and
perpetual, the writer assumed one publication and one year, and nobody wrote
it down.

## 3. Sponsorship disclosure

**Mandatory, at the top, in the same medium as the content.** A sponsored
article says so before the first paragraph. A sponsored video says so in the
first fifteen seconds *and* in the description. A sponsored talk says so on
the first slide.

This is not only the platform's rule. In the European Union the Unfair
Commercial Practices Directive and, in France, the *loi du 9 juin 2023* on
commercial influence make undisclosed paid promotion an offence with real
penalties. Members publishing from other jurisdictions are subject to their
own, and several are stricter.

An undisclosed partnership discovered after the fact is grounds for revoking
the attestation that rests on the piece. The attestation says a competent
reader found the work sound; it was not sound.

## 4. Citation and fair use

Quote, attribute, link. Short excerpts for the purpose of commentary,
criticism or teaching are permitted in most jurisdictions the platform serves,
and the permission is narrower than people assume: it does not cover
reproducing a whole article, a full figure from a paywalled paper, or a
screenshot of somebody's paid course.

Ask. Most authors say yes, and the ones who say no would have said no
afterwards too.

## 5. Generated content

**Disclosed, and accepted.** The platform's position across every domain, and
this one needs it stated most precisely because the tool can produce the
artefact rather than assist with it.

What is accepted:

- a draft written with a model and then checked, line by line, by the author;
- translation with machine assistance, reviewed by somebody who reads the
  target language;
- generated illustration, declared, where the piece is not about the
  illustration.

What is not:

- a claim no human verified;
- a citation nobody opened;
- a piece delivered to a paying client with generated content in it, where the
  client was not told and did not consent. This is contractual, not
  stylistic — a client who commissioned a person and received a model's output
  did not get what they paid for;
- a generated likeness or voice of a real person, ever, without their written
  consent. The audio domain's rule (see `docs/audio/VOICE-RIGHTS.md`) applies
  here whenever a video or a podcast is involved.

## 6. Non-compete on DevRel commissions

A developer advocate commissioned by one company and speaking for a competitor
the following month is a real problem for both, and a permanent non-compete is
not the answer: it would make this trade unliveable for anybody who is not
employed full time.

The platform's position: at most **six months**, at most **the named direct
competitors listed in the commission**, and paid for. A non-compete that is
not compensated and not bounded is one the platform will not host.

## 7. What a member should keep

- the commission, with its IP terms and its licensing scope, in writing;
- the source files of anything delivered;
- for a translation, the source version translated and the glossary used;
- for research writing, the data and the protocol;
- consent, in writing, for any likeness, voice or quotation of a person.

## 8. Open questions for the lawyer

1. Does an attestation, as the platform issues it, create any warranty
   obligation towards a third party who relies on it?
2. Where a commission's `licensing_scope` and the upstream project's licence
   conflict — a client commissioning a documentation contribution to an
   AGPL project — which governs, and what should the platform refuse to
   publish?
3. Cross-border: a Benin-registered platform, a member in Senegal, a client in
   Germany. Which law governs the commission by default, and what should the
   platform's template say?
4. Is the six-month bounded non-compete of §6 enforceable in the jurisdictions
   the platform operates in, and does making it paid change the answer?
5. What is the platform's exposure when a member publishes an undisclosed
   sponsored piece under an attestation the platform issued?
