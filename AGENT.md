**Never commit or push to `main`.** Every task goes: define the task, create a
branch for it from an up-to-date `main`, do the work there, push the branch
and open a pull request into `main`. `main` is protected on GitHub: it only
changes through a merged pull request whose checks pass.

**Several agents work in this repository at the same time.** Work and push
only on your own branch; never push to, reset or force-push a branch another
agent made. Start each branch from a freshly fetched `origin/main`. When your
pull request conflicts with `main`, merge `origin/main` into your branch and
resolve every conflict by keeping both sides' work (never take "ours" or
"theirs" wholesale over a file someone else changed), then rebuild and push.
Never force-push over history someone else has fetched, and never discard
uncommitted changes you did not make.

Workflow, artifacts and the glossary: `CONTEXT.md`. Do not touch other agents'
`context/mrs/*` without being asked.

What is published is the runtime and the documentation needed to use and
develop it. The local research base, dumps, third-party checkouts and the work
journal are not. That is a rule about *what* ships, not about what may be said:
the use of reverse engineering, the names of the games studied and the credit
owed to the projects that were read are not hidden — they are in `README.md`
and `NOTICE`, and belong there.

Retail offsets stay out of the code for their own reason: a standalone runtime
resolves nothing against a retail image, so a hardcoded address or a decompiler
placeholder name (`FUN_…`, `DAT_…`) is dead weight wherever it appears.

`make publish-check` walks the tracked tree for both. It is a grep over a fixed
set of shapes — it proves no claim about origin or licensing, and passing it is
not an argument for anything beyond the absence of those shapes.

A push of unpushed work is a pass of its own — history, tests, comments, names —
in `CONTEXT.md` under Making a public push.

**Keep the progress table true.** [`docs/PROGRESS.md`](docs/PROGRESS.md) lists
each game's milestones; the table at the top of `README.md` is generated from
it. A pull request that reaches a milestone ticks it (one that breaks it
unticks it) in the same pull request and runs `cargo xtask progress --write`.
CI fails when the table and the milestones disagree.
