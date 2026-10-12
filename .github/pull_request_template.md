## What and why

<!-- One or two lines. For a game: which game, which behaviour. -->

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
- [ ] **Checks run.** `cargo xtask boundary --enforce` and `make publish-check`
      are clean; clippy clean and `rustfmt --edition 2024` on touched files.
- [ ] **Review threads resolved before merge.** Every review comment is
      answered or fixed and its thread resolved; no merge over an open review.
