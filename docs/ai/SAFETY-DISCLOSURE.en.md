# Disclosure and responsible AI research

*To be published at `skill-uv.com/ai/disclosure`.*

This policy says what to do with a finding before publishing it. It applies to
any work done on Skilluv that defeats a model or a system: a jailbreak, an
injection, data extraction, tool misuse, a measured bias.

It is binding. A finding published outside this process is not attested, and
publishing it that way can lead to the work being revoked.

---

## 1. The principle

Tell whoever can fix it first, publish second.

The order is not a courtesy. Between the moment an attack is written publicly
and the moment it is fixed, anybody can use it. Notifying first shortens that
window; publishing first lengthens it.

## 2. The states

The platform tracks every finding through a state, and the transitions are
enforced:

| State | What it means |
|---|---|
| `private` | Known to the author and the reviewers only |
| `vendor_notified` | Sent to whoever can fix it, with the date |
| `embargoed` | Notified, with a publication date agreed |
| `published` | Out |
| `withheld` | Deliberately not published, with a written reason |

Nothing goes backwards. A disclosure whose history can be rewritten is not a
disclosure.

Going straight from `private` to `published` is refused by the platform, not
merely discouraged.

## 3. The window

**Ninety days** from notification, by default.

That is a default, not a law. A vendor who fixes in a week should not wait
twelve: the agreed date replaces the default. A vendor asking for longer is
sometimes right, and the agreement is recorded.

If the vendor does not answer, the clock runs anyway. Silence is not a veto.

## 4. What counts as a finding

- A named model **with its version**. "GPT-4" is not a target; without the
  version or snapshot date nobody can replay it six months later.
- A **reproduction procedure** a stranger can follow.
- A **success rate** over a stated number of attempts. Seven out of ten and
  seven out of a thousand read the same when only successes are recorded.
- A **proposed mitigation**. Reporting without proposing leaves the whole
  problem with somebody else.

Zero successes out of N is not a finding: it is a model behaving. That is
useful, and it belongs elsewhere.

## 5. Bias

A measured bias is disclosed **even when it is awkward**, including for a
Skilluv partner. The conditions are the same as for everything else: a written
protocol, named subgroups, a measured gap, and a third party able to replay
it.

A bias result that cannot be reproduced is not disclosed — not out of
political caution, but because it is not established.

## 6. Dual use

Some findings teach an attacker more than they help a defender. The platform
has `withheld` for that case, and **requires a written reason**: withholding
with no stated ground is indistinguishable from burying.

Sensitive cases are decided by more than one person. In phase 1 that means: at
least one holder of `ai_reviewer:safety` other than the author, and the
decision is recorded.

Publishing a mitigation without publishing the full exploit is almost always
the option still open when both extremes are bad.

## 7. Releasing weights

Making trained weights public is a decision, not an end-of-project formality.
Before release: what the model can do that you do not want it to, what its
training data contains, and under which licence it goes out.

A model that has not been evaluated on that point is not ready to be
published, even if it works.

---

*See also: the [domain charter](./CHARTER.en.md) and the red-team report
template, served by `GET /api/guides/template-red-team-report`.*
