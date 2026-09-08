# The stack of a trade

A trade card on the signup flow draws the logos of the tools that trade works
with. This is where those identifiers come from and what a client may assume
about them.

## Where it lives

`orientations.stack` is a `TEXT[]` of identifiers, in reading order. It comes
back on every row of `GET /api/orientations`, in the same response as the rest
- there is no second call per trade, deliberately, because that would undo the
work `/api/orientation-counts` exists to do.

```json
{
  "slug": "web-frontend-developer",
  "name": "Développeur Web Frontend",
  "tags": ["web"],
  "stack": ["react", "vue", "svelte", "typescript", "css"]
}
```

`stack` is not `tags`. Tags are broad categories - `web`, `api`, `mobile` -
meant for filtering. They cannot draw a stack, which is what made this column
necessary.

## What a client may assume

**Identifiers are stable.** A rename would silently break every consumer that
maps them to a logo, so a tool that changes its name keeps its id and changes
its `display_name` in the registry. `tests/suite/test_ski_367_orientation_stack.rs`
pins the ones the frontend draws, so a rename fails in CI rather than on
somebody's signup screen.

**An unknown identifier should render nothing.** Not a placeholder, not a
guess. The registry grows; a client that has not caught up must degrade to
silence.

**An empty stack means "not recorded yet".** It never means "this trade uses
no tools". Most trades are empty today - see below.

## Why most are empty

The tools were, and still are, written into `description` as prose:

> React, Vue ou Svelte, TypeScript, CSS moderne. Construit ce que la personne
> voit et manipule.

That sentence has alternatives ("ou"), qualifiers ("moderne") and punctuation.
Deriving identifiers from it would produce a card showing three right logos
and a fourth invented, with nothing to signal the error - and on the screen
that asks somebody to choose their trade, a wrong logo is worse than no logo.

So the backfill in migration `0619` was written by hand and covers only the
thirteen trades whose own text names their tools outright. Filling the rest is
content work, one trade at a time, and an empty stack in the meantime is the
honest answer rather than a gap to paper over.

## Adding a tool

1. Insert it into `tools` (`id`, `display_name`, `category`). The id must match
   `^[a-z0-9][a-z0-9.+-]*$`.
2. Add it to the orientation's `stack`.

Step 1 is not optional: `orientation_stack_is_known()` refuses any identifier
with no row in `tools`, because a foreign key cannot reach inside an array and
without the trigger this column would be free text with a nicer name. A typo
would then arrive as a missing logo nobody could explain.

A stack may not repeat a tool. The card would draw the same logo twice and
look broken.

## The registry today

`SELECT id, display_name, category FROM tools ORDER BY category, id;`

The table is the source of truth - this document explains the contract, it
does not duplicate the list. Categories are `language`, `framework`, `runtime`,
`database`, `platform`, `protocol`, `os`, `tool`.
