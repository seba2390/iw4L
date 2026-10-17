# Agents

Read this file first, then [`docs/INDEX.md`](docs/INDEX.md) for where things
live. [`CONTEXT.md`](CONTEXT.md) is the maintainer's working-memory contract
(`context/`); [`CONTRIBUTING.md`](CONTRIBUTING.md) is the same workflow for
people.

## Work from an issue

All open work is a GitHub issue on
[seba2390/iw4L](https://github.com/seba2390/iw4L/issues). Each carries a
`game: …` label (Modern Warfare 2, Black Ops 1, Black Ops 2, or `all`), a
`mode: …` label (Multiplayer, Zombies, or `all`), `area: …` labels and exactly
one status: `status: help wanted` (free), `status: in progress` (claimed) or
`status: blocked` (waits on the issues under "Depends on").

1. **Pick.** An open `status: help wanted` issue whose "Depends on" issues are
   closed. Work you were asked to do that no issue covers gets an issue first,
   in the same shape and with the same labels — unless it is a small fix (a
   typo, a wrong constant, a doc correction).
2. **Claim before the first edit.**
   ```sh
   gh issue edit <N> --add-assignee @me \
     --remove-label "status: help wanted" --add-label "status: in progress"
   gh issue comment <N> --body "Claimed on branch <branch>."
   ```
   Agents may share one GitHub account, so the comment, not the assignee,
   says who holds the claim. Read the issue back (`gh issue view <N>
   --comments`): an earlier `Claimed` comment with no `Released` after it
   means the issue is taken — remove your comment, pick another.
3. **Branch and draft pull request.** Branch from a freshly fetched
   `origin/main`, named for the game and topic (`t5/…`, `t6/…`, `iw4/…`,
   `shared/…`). After the first commit, push and open a draft pull request
   whose body says `Closes #<N>`.
4. **Finish inside the issue.** The pull request does what the issue's
   acceptance list says, including the milestone it ticks in
   `docs/PROGRESS.md` and the ledger ids it removes. Merging closes the issue;
   an issue that lists it under "Depends on" and has no other open dependency
   then moves from `status: blocked` to `status: help wanted`.
   New work found on the way becomes a new issue, not a wider pull request.
5. **Stopping early.** Comment `Released: <what is done, what is left,
   branch>`, unassign, and put `status: help wanted` back. A claim with no
   commit or comment for seven days may be released by the maintainer.

## Rules

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

## Checks before a pull request

CI runs `make publish-check`, `cargo check --workspace --all-targets` (fails
on any rustc warning), `cargo xtask lints --enforce` (clippy and rustc warnings
may only go down: `xtask/lints/allow.txt`), `cargo fmt --all -- --check`, ruff
on `scripts/`, `cargo xtask boundary --enforce`, `cargo xtask progress` and
`cargo xtask ledger-guard` (a game's code change also changes its ledger, or
the pull request body says `Ledger: none — <reason>`). Run them before
pushing — CONTRIBUTING.md lists the exact commands — and fill in the pull
request template.
