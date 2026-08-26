# Getting started

If you have never done any of this: `CURRICULUM.md` is the twelve-week path and
this page is the ten minutes before it.

## The five minutes that matter

1. **Read `SCOPE.md`.** Four minutes. It is what separates this trade from an
   offence, and everything else here assumes you have.
2. **Answer the wizard.** `GET /api/users/me/domain-profile/security/questions`
   asks five things, and one of them is unusual: *what can you actually run?*
   A browser, local tools, virtual machines, a home lab, or cloud. Nothing will
   then recommend you a week you cannot do.
3. **Pick a trade, or say you have not decided.** Up to three, and one is a
   stronger claim than three.

## The first month, per trade

Written out, in the platform, in French and English:

```
GET /api/domains/security/guides?kind=onboarding
```

or under the trade you chose in the interface. Five guides, and they share
almost nothing — a red teamer's first week is a proxy and a range; a governance
specialist's is a framework read properly, once.

They are also readable in the repository as rows of migration 0562, if you would
rather read SQL than open a browser, which in this domain is a reasonable
preference.

## What you need installed

Depends on the answer you gave about your machine. The honest minimum per
trade:

| Trade | Minimum | Comfortable |
|---|---|---|
| Red team | A browser, and Burp Community or ZAP | Plus Docker for local ranges |
| Blue team | Wireshark | Plus Python, plus Volatility |
| Code security | An editor and Semgrep | Plus the language toolchain of what you audit |
| Governance | A text editor | Nothing else. This trade needs no tooling and that surprises people |
| Purple team | Two virtual machines you can throw away | A separate machine |

`TOOLKIT.md` has the install notes and what each free tier actually does.

## What not to do first

**Do not buy a certification.** Not because they are worthless — an OSCP is a
real thing — but because it is three hundred to eight thousand euros spent
before you know which of the five trades you want. Twelve weeks of the
curriculum costs nothing and answers that.

**Do not test anything that is not in a scope.** Not your employer's site, not
your old university's, not the shop with the obviously broken checkout. This is
the mistake that ends careers before they start, and the range exists so that
you never need to make it.

**Do not start with a hard machine.** Two weeks on something calibrated for
somebody with two years of experience teaches nothing except that you cannot do
it, which is false.

## Where to ask

`#security-help` on Discord. Bring the request, the response, and what you
expected — the same three things a report needs, which is not a coincidence.

For a mentor: `POST /api/users/me/mentorship/request-mentor`. Matching is on
the trade you chose and the tools you named, capped at three mentees per mentor
because a security session is somebody reading your work for an hour and it does
not compress.

## The first thing worth submitting

Not a finding. A **write-up of a range challenge** — two hundred words on one
thing you solved on Juice Shop or WebGoat, with the request, the response, and
the sentence saying which check was missing.

A reviewer will read it and tell you what is missing from it. That feedback,
early, is worth more than the first finding — because the first finding written
without it usually gets refused for a reason nobody explained beforehand.
