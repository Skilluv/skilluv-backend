# Ownership, licences and data — AI domain

*To be published at `skill-uv.com/ai/ip`.*

**This is not legal advice.** It states Skilluv's position and the rules the
platform enforces. Open questions are marked as open; they are waiting on a
lawyer with AI experience, and this document will be revised then.

It is written now because the absence of a rule is itself a rule, and the
worst one: with no text, everybody improvises and finds out once the model is
published.

---

## 1. Who owns a trained model

**By default, whoever trained it.** A model produced in a Skilluv challenge
belongs to its author; the platform claims nothing and hosts no weights.

Two exceptions:

- **Commissioned work.** What is produced for a client follows what the
  contract says, and the contract must say it. See §5.
- **Derived models.** Fine-tuning a pretrained model does not reset the
  counter: see §2.

## 2. The licence chain

A fine-tuned model inherits the obligations of its base. They differ, and the
gap matters:

| Upstream licence | What it means in practice |
|---|---|
| Apache 2.0 / MIT | Commercial use free, attribution required |
| Vendor community licence | Often usage thresholds, use restrictions, sometimes an attribution requirement |
| Non-commercial | The derivative cannot be sold, nor serve a paid product |
| Closed weights behind an API | You do not fine-tune; you do not have the weights |

**Skilluv rule**: the base model's licence is named in the card, and the
derivative's licence is compatible with it. A deliverable whose licence chain
is incoherent is refused — not out of formalism, but because it is unusable
by anybody who picks it up.

Terms change. **Check the licence as at the training date**, and record that
date in the card.

## 3. Data provenance

**Admissible**: a dataset published under an open licence; data you produced;
data obtained with consent; a public corpus whose terms permit the intended
use.

**Not admissible**: a site scraped against its terms; personal data with no
legal basis; a dataset whose origin you cannot state.

A default position on the open web would be imprudent and is not taken here:
the legality of scraping for training purposes varies by jurisdiction and is
moving. **Open question, to be settled with a lawyer.**

What is settled: on Skilluv, a deliverable **says where its data came from**. A
dataset with no stated provenance is refused whatever the legal answer turns
out to be.

## 4. Personal data

The GDPR assumes data can be erased. A trained model does not allow that
simply: weights do not forget on request.

The practical consequence is upstream, not downstream:

- do not put personal data in a training set without an explicit legal basis;
- anonymise before, not after;
- for a dataset containing people — faces, voices, identifying text —
  document consent in the card.

A model you could not correct if somebody withdrew is a model whose weights
you do not publish.

## 5. Commissioned work

When work is paid for by a third party, four splits are possible and the
contract names one:

- **full transfer** — weights and code pass to the client;
- **open model** — the client keeps their usage rights, the model is published;
- **commercial licence** — the client may exploit it, nobody else;
- **weights to the client, code to the author** — the most common, and the
  worst written.

**No mission starts before the split is written down.** The ambiguity only
surfaces once the work has value, which is the worst possible moment.

## 6. European regulation

The EU AI Act classifies uses by risk level and imposes increasing
transparency obligations. Skilluv is not subject to most of them today — we do
not place an AI system on the European market — but work published here may be
placed there by whoever picks it up.

The position: **write down what a downstream user needs in order to comply.**
That is exactly what the model card already asks for — intended use, data,
limits, evaluation. Work documented to the charter's standard is work on which
compliance is possible.

**Open question**: the detail of the obligations per classification, to be
settled with a lawyer before a European company commissions a mission.

## 7. Generative output

The status of a generative model's output varies by jurisdiction and is not
settled. Saying so is more honest than deciding.

What Skilluv applies:

- the models and LoRAs used are cited, with their licences;
- a style imitating an identifiable living person is not published without
  their agreement — regardless of what the law permits;
- the generative nature of the artefact is declared.

## 8. What causes a revocation

- a licence chain violated, discovered after validation;
- a dataset withdrawn over a rights problem;
- personal data found in a published dataset;
- a stated provenance that turns out to be false.

Revocation removes the artefact from every count and leaves the history
visible.

---

*See also: the [domain charter](./CHARTER.en.md) and the [disclosure
policy](./SAFETY-DISCLOSURE.en.md).*
