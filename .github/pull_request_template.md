## What and why

<!-- One or two lines. For a game: which game, which behaviour. -->

Closes #<!-- issue number; see AGENTS.md "Work from an issue". Small fixes without an issue: write "No issue — <why>". -->

## Checklist

- [ ] **Source of each behaviour.** For every rule this PR adds or changes, say
      where it comes from: the game's handler (how it was read), its zone data
      (which asset), or the snapshot/state IW4L already carries. Nothing from
      another game stands in.
- [ ] **Ledger.** Anything IW4L decides itself under a game is in
      `docs/fidelity/<game>.md` (Open section, severity, id) and fails loudly
      where unimplemented (`unknown!("<id>")` or an error starting `<id>:`).
      Otherwise a line here: `Ledger: none — <reason>`.
- [ ] **Progress.** A milestone in `docs/PROGRESS.md` this PR reaches (or
      breaks) is ticked (or unticked) here, and `cargo xtask progress --write`
      has regenerated the README table. Otherwise: no milestone changed.
- [ ] **Checks run.** What CI runs is clean: `make publish-check`,
      `cargo check --workspace --all-targets` (no warnings),
      `cargo xtask lints --enforce`, `cargo fmt --all -- --check`,
      `ruff check scripts && ruff format --check scripts`,
      `cargo xtask boundary --enforce`, `cargo xtask progress`,
      `cargo xtask ledger-guard --base origin/main` and
      `cargo xtask port-graph check --ci`.
- [ ] **Review threads resolved before merge.** Every review comment is
      answered or fixed and its thread resolved; no merge over an open review.
