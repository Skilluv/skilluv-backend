# Communication — brief templates

Backlog: communication/F-05. What a client or a challenge author fills in when
they want communication work done. The **write-up** templates — what the
contributor hands back — are the other end of the same conversation.

Both halves are rows in `content_guides`, seeded by migration
`0514_communication_content_guides.sql` and translated in
`0535_communication_guides_french.sql`: `kind = 'brief_template'` for these,
`kind = 'writeup_template'` for the others. They are rows rather than files
because they are edited by people who do not deploy, and this document says
what they are for.

Five briefs, one per trade. Not four, even though there are four review
families: a brief is written by whoever commissions the work, and somebody
commissioning a conference talk and somebody commissioning a YouTube series
have nothing in common to say, however similar the two look to a reviewer.

---

## The shared skeleton

Every communication brief answers these:

1. **Who the audience is**, and what they can already do. Not "developers".
2. **What they must be able to do afterwards**, or understand, in one
   sentence.
3. **Format and length**, and where it will be published.
4. **What is in scope and what is explicitly out.** The second list prevents
   the argument at the end.
5. **Who answers a technical question** during the work, and how fast.
6. **Rights**: byline, licence, whether the author may show it in their
   portfolio.
7. **Deadline**, and how many revision rounds the fee covers.

A brief that cannot answer 1 and 2 is not ready, and the contributor should
say so rather than start.

---

## `brief-communication-docs` — documentation

Adds the page type — tutorial, how-to, reference or explanation — as a
checkbox, because a page that is two of them loses both readers and the
decision belongs to whoever commissions it.

Also asks which version is being documented and where the example code lives.
A documentation brief with no version produces a page that is wrong the day
it ships.

## `brief-communication-talk` — talk, workshop, demonstration

Adds the event: audience size, level, what they came for, and exact duration
questions included. Then the demonstration — whether there is one, whether
the network is reliable, who provides the environment.

The terms section is longer than the others on purpose. Whether the fee covers
preparation, whether travel is reimbursed, and whether exclusivity is being
asked for are the three things that go unsaid and then go wrong.

## `brief-communication-video` — video content

Adds cadence, who writes the script and whether it is approved before
shooting, and whose channel it is published on.

The paid-partnership disclosure field is not optional and does not have a "no"
option: it asks *where* the disclosure goes. Sponsorship is legitimate;
deciding after the fact where to hide it is not.

## `brief-communication-translation` — translation

Adds the regional variant, the source version as a commit or a tag, and the
existing glossary if there is one.

It asks whether review is part of this commission or a separate one, and
whether the reviewer reads both languages. A translation validated by somebody
who reads only one of them has not been validated.

## `brief-communication-research` — whitepaper, report, specification

The one with an independence section. It asks, in writing and before the work:
is the commissioner an actor in the field being studied, where will that be
stated in the document, and may they ask for an unfavourable result to be
removed.

The only good answer to the last question is no, and the point of asking it in
the brief is that it is answered before anybody knows what the result will be.

---

## The clause every communication brief should carry

**What happens to the work if the commission ends early.** Half a documentation
set, a script with no video, a translation of the first three chapters: each
is worth something to somebody, and who may use it is a question with a clean
answer before the work starts and no clean answer afterwards.
