# Modern Warfare 3 — fidelity ledger

Rule: Modern Warfare 3 matches run on Modern Warfare 3's own rules and data;
no other game's behaviour stands in. A rule that is not recovered is an
`unknown!` in `game_iw5`, listed here with its id; `cargo xtask boundary`
refuses an id that is missing here.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.

## Open

- [ ] **G** `iw5.scripts.gametypes` (`GameScripts::program`): no Modern
      Warfare 3 map has a script program, so its matches are refused at load.
      Until then they ran Modern Warfare 2's gametype scripts and natives.
      *Needs:* Modern Warfare 3's builtin list and the natives behind it, and
      its match flow.

## Closed
