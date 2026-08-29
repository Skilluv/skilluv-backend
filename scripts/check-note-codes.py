"""Every `note` a member reads carries a code the front can translate.

## What this guards

Eight handlers return a `note` field written as a French sentence, hard-coded
in Rust, on responses read by any member. They are not decorative labels: they
are the sentences that tell somebody what they have just committed to,
returned at the moment they commit to it.

  * withdrawing consent is not retroactive — the thing people get wrong;
  * an onboarding or a placement starts on acceptance, and the person may
    refuse what their employer paid for;
  * paying does not certify;
  * a cost reduction is only attested once the service is checked.

An anglophone reader received those in French, at the moment they had to
understand them to decide. And the front cannot fix it alone: re-translating
them would betray them, rewriting them would display a commitment the backend
did not make.

So each one now carries a `note_code` beside it, and the front translates from
the code. This script is the half that keeps it true: the ninth `note` will be
written by somebody who has not read this file, and it will be refused here.

## Why a script and not a Rust test

The check is textual — "does this literal have a sibling literal" — and a Rust
test asserting it would have to re-list the sites by hand, which is the copy
that drifts. Reading the source is the only version that cannot go stale.

Exit 1 on a bare note. Run: python3 scripts/check-note-codes.py
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ROUTES = ROOT / "src" / "routes"

# Admin and security surfaces are out of scope, and deliberately so. Their
# notes are in English and address administrators and an anglophone security
# audience — a different reader, a different surface, and a code there would
# buy nothing.
#
# Every `admin_*.rs` for the same reason, found by running this: `admin_slices`
# passes a note straight through from the request body, and
# `admin_validators` carries an English operations note about dogfooding
# ratios. Neither is a sentence a member reads.
EXEMPT = {"security.rs"}
EXEMPT_PREFIX = "admin_"

# Only a note whose value is a **string literal** — a sentence written in Rust.
# `"note": body.note` passes through whatever a caller sent, so there is no
# sentence here to translate and no code that could describe it.
NOTE = re.compile(r'^\s*"note"\s*:\s*"')
NOTE_CODE = re.compile(r'^\s*"note_code"\s*:\s*"([a-z0-9_]+)"')


def main() -> int:
    bare: list[str] = []
    codes: list[str] = []

    for path in sorted(ROUTES.glob("*.rs")):
        if path.name in EXEMPT or path.name.startswith(EXEMPT_PREFIX):
            continue
        lines = path.read_text(encoding="utf-8").split("\n")
        for i, line in enumerate(lines):
            if not NOTE.match(line):
                continue
            # The code sits on the line after the note's closing. A note is
            # often a multi-line string continued with `\`, so scan forward to
            # the end of the literal rather than checking i + 1.
            j = i
            while j < len(lines) - 1 and lines[j].rstrip().endswith("\\"):
                j += 1
            following = lines[j + 1] if j + 1 < len(lines) else ""
            match = NOTE_CODE.match(following)
            if match:
                codes.append(match.group(1))
            else:
                bare.append(f"{path.name}:{i + 1}  {line.strip()[:70]}")

    if bare:
        print("NOTE-CODES: a member-facing note has no code the front can translate\n")
        for entry in bare:
            print(f"  {entry}")
        print(
            "\nAdd a `note_code` on the line after it. The sentence stays — it is\n"
            "the commitment the backend makes — and the code is what lets an\n"
            "anglophone reader receive it in their own language.\n"
            "\nSee SKI-353. Admin and security surfaces are exempt (English, and a\n"
            "different audience); add the file to EXEMPT here if you add one."
        )
        return 1

    duplicates = {c for c in codes if codes.count(c) > 1}
    if duplicates:
        # Two different sentences under one code would have the front show the
        # wrong commitment for one of them — worse than the French original,
        # because it would be confidently wrong.
        print(f"NOTE-CODES: the same code covers two different notes: {sorted(duplicates)}")
        return 1

    print(f"NOTE-CODES ok -- {len(codes)} member-facing notes, each with a code")
    return 0


if __name__ == "__main__":
    sys.exit(main())
