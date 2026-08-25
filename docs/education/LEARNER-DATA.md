# Education — learner data, minors, and what an attestation is not

*Ticket L-01. To be reviewed by a lawyer before the first cohort runs; what
follows is the platform's position, written so the review has something to
correct rather than something to invent.*

The legal review is shared with the other domains' L-01 tickets. This is the
part specific to education, and it is the part with the most exposure: every
other domain's artefacts are about work, and this domain's are about people.

---

## 1. Why this domain is different

In every other domain, the person whose data is in an artefact is the member
who made it. They agreed to the terms, they can delete their account, they
chose to publish.

Here the artefact is about **learners**: people who are usually not members,
who signed up for a course rather than for a portfolio, who are sometimes
minors, and who have never seen this platform. Under the GDPR they are data
subjects with the full set of rights, and the platform is at minimum a
processor and arguably a controller for what it stores about them.

Nothing about a learner reaches this platform unless it is either aggregate or
consented. That is a design constraint, not a policy page.

## 2. What the schema enforces, so nobody has to remember

| Rule | Where it lives |
|---|---|
| A testimonial cannot be stored without consent | `a_testimonial_carries_its_consent`, migration 0524 |
| An outcome row is per learner, so one learner can be erased alone | `education_learner_outcomes`, migration 0524 |
| A delivery reporting on learners is not attested until its author declares it clear | `education_learner_data_cleared_at`, migration 0523 |
| The declaration has a signer and a date | `project_slices_education_declaration_has_an_author` |
| A public profile shows aggregate figures only | `education_profile::cohorts_for` |
| Outcomes are readable by the teacher and the learner, and nobody else | `routes::education::list_outcomes` |

The last one includes curators and moderators. An outcome row is a record of
somebody's difficulty learning, and the platform's interest in moderating
content does not extend to reading it.

## 3. Minors

**Nothing identifiable, at all.** No name, no image, no voice, no work sample,
no assessment result, no message.

An under-18 learner appears in a delivery as a number in a total, and in no
other way. There is no consent form that changes this, because the platform is
not equipped to verify parental consent across the jurisdictions it operates
in, and a consent it cannot verify is worse than a refusal.

Where a programme is for minors:

- the brief says so before the work starts, because it changes what may be
  collected at all;
- the educator keeps assessment data in their own institution's systems, under
  that institution's terms, and brings only aggregates here;
- the platform stores no direct communication channel between an educator and
  a minor learner. Cohort messaging is for members, and members are 16+ under
  the platform's own terms.

## 4. Consent, when it is used at all

A testimonial is the only place a learner's own words reach a public surface,
and it requires:

- the learner writing it themselves — not the educator paraphrasing;
- an explicit, timestamped consent, recorded as `testimonial_consent_at`;
- a way to withdraw it that actually removes the text.

Consent to a testimonial is not consent to anything else. It does not cover
the learner's assessment results, their attendance, or their name appearing in
a report.

## 5. Erasure

A learner who is a member deletes their outcome rows through the platform's
existing erasure path, and the cohort's completion figure recomputes without
them.

This has a consequence worth stating: **an attestation can stop being
supported.** If enough learners erase their records that a cohort falls below
the threshold, the fact the attestation claims is no longer evidenced. The
attestation is not revoked automatically — the teaching happened — but the
figure behind it changes, and a reader following it sees what is actually
there. This is the correct behaviour and the alternative is a cached number
that nobody can check.

A learner who is not a member has no account to erase from, which is exactly
why nothing identifiable about them is stored.

## 6. Retention

- Outcome rows: for as long as the cohort's attestation is live. An educator
  who deletes their attestation has no reason to keep them.
- Assessment content inside `pre_assessment` / `post_assessment`: the platform
  does not inspect it and does not want personal data in it. It is for scores
  and skill labels, not for comments about a person.
- Nothing is kept "for statistics" beyond the aggregate figures already on the
  cohort.

## 7. Assessment ethics

- Rubrics published before submission.
- An appeal process, named, with somebody other than the original assessor
  deciding.
- No exercise designed so that copying passes, and plagiarism handled as a
  teaching problem before a disciplinary one.
- An educator does not assess a learner they have a personal or commercial
  relationship with, and says so when it happens.

## 8. What an attestation is not

Printed, in clear, on every education attestation:

> This attestation records work delivered on Skilluv and reviewed by a
> practitioner of the trade. **It is not a diploma, a teaching qualification
> or an accreditation.** It does not entitle the holder to teach where a
> licence is required, and it makes no statement about any qualification a
> school, a ministry or an employer may ask for.

This matters more here than in any other domain. "Certified trainer" is a
phrase with legal weight in several of the jurisdictions the platform serves,
and an attestation that reads like one is a liability for the platform and a
false hope for the member.

## 9. Open questions for the lawyer

1. Is the platform a controller or a processor for `education_learner_outcomes`
   where the learner is a member? Where they are not?
2. Does the aggregate-only rule of §3 hold for a 16- or 17-year-old who is a
   member under the platform's own terms, or does the stricter rule apply?
3. What notice, if any, does a learner in an off-platform cohort need to be
   given before an educator records an aggregate figure derived from their
   results?
4. Does §5 — an attestation surviving while its evidence shrinks — create any
   misrepresentation exposure, and should the threshold be re-evaluated at
   read time rather than at issue time?
5. In which of Benin, Senegal, Côte d'Ivoire, France and Germany is "formateur
   certifié" or an equivalent a protected term, and is the disclaimer in §8
   sufficient?
