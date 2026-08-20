"""Print every capability the code asks for, one per line.

A route that guards itself with a capability the catalogue does not hold is a
route nobody can ever reach: the grant fails, so the guard refuses everybody.
That has happened seven times, each time because a migration restated a CHECK
from a branch that could not see another branch's addition. Reading the names
out of the code and comparing them to the catalogue is what catches it.

The argument text is read with balanced parentheses rather than a fixed window
of lines, so a literal that merely sits near a call is not mistaken for one
passed to it.
"""

import os
import re
import sys

CALL = re.compile(r"require_(?:any_)?capability\s*\(")
LITERAL = re.compile(r'"([a-z_]+(?::[a-z0-9_-]+)?)"')


def arguments_at(text, open_paren):
    depth = 0
    for i in range(open_paren, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[open_paren + 1 : i]
    return ""


def main():
    names = set()
    for root, _, files in os.walk("src"):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(root, name)
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
            # Doc-comments show example calls; they are not call sites.
            text = re.sub(r"^[ \t]*(///|//!|//).*$", "", text, flags=re.M)
            for match in CALL.finditer(text):
                names.update(LITERAL.findall(arguments_at(text, match.end() - 1)))
    for name in sorted(names):
        sys.stdout.write(name + "\n")


main()
