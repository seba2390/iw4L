# Game boundary: whose rules run a match

Every game is played by its own rules, from its own data: Modern Warfare 2
(`iw4`) by MW2's, Black Ops (`t5`) by Black Ops', Black Ops 2 (`t6`) by Black
Ops 2's, Black Ops 3 (`t7`) by Black Ops 3's, MW3 (`iw5`) by MW3's. No game
borrows another game's behaviour; a rule a game does not have yet is said out
loud. CI enforces it on code alone — no game file is ever in the repository.
How a game's data and scripts are read: [`DECODING.md`](DECODING.md).

**Layers.** Each crate declares one in its `Cargo.toml`
(`[package.metadata.iw4l] layer`, and `game` for format/game crates):

| layer | may depend on | holds |
|---|---|---|
| `neutral` | neutral, format | no game rule or constant: transport, render backend, net transport, GSC pieces (`gsc`), `game_api`, math, diagnostics |
| `format` | neutral, its game's format | how one game's data is laid out (`fastfile_t5`, …) |
| `game` | neutral, its game's format/game | one game's rules (`game_iw4` + the `*_iw4` rule crates, `game_t5`, `game_iw5`, `game_t6`, `game_t7`) |
| `session` | anything | picks one game per match; names games only in `crates/session/src/games.rs` |
| `mixed` | anything | crates still holding several games' rules; the list may only shrink |

**One game per match.** The match's game is the map's family. `games.rs` maps
it to that game's `game_api` answers: `GameScripts` (builtins, startup, dvars),
`GameModes` (`ModeRules`: flow, limits, movement, weapons, `HudRules`; zombies
maps), `GameMenus` (fonts, `MenuLayout`), `GameVision` (shellshock), its native
registry and menu expression parser. Assets come from the match's or the
asset's own game, never another game's same-named asset. A rule `games.rs`
lends across games is one visible, ledgered line (today: Black Ops 2 moves and
fires by MW2's rules, by owner decision).

**Unknown, not borrowed.** A game crate answers a rule it does not have with
`Rule::Unknown(unknown!("t5.area.rule", what, needs))`. The caller reports it
(`gsc: refused … unknown=<id>`, a gap line) and applies nothing in its place.
Every id is listed in that game's ledger, [`fidelity/<game>.md`](fidelity/).
Black Ops multiplayer and MW3 maps are refused today for that reason.

**Enforcement.** `make boundary` (`cargo xtask boundary`) checks:

* graph — the dependency rules above;
* patterns — outside game/format crates no code names a game (`ZoneGame::T5`,
  `FamilyId::…`, a game crate path, a `"t5"` literal, `is_t5`); `games.rs` and
  the identity type are exempt;
* ledger — every `unknown!` id is in its game's ledger;
* ownership — every game has its own format crate and its own `game_<game>`
  answering all four `game_api` traits, and `games.rs` answers a game only with
  that game's crates; a lent rule is a named item whose type the borrowing
  game's ledger names (`T6OnIw4Movement`).

Ownership and ledger failures are errors. Graph and pattern debt is
`xtask/boundary/allow.txt`, a ratchet: anything not in it is new, an entry that
occurs less often is stale, `--update` only lowers it. `--enforce` fails on
either; the pre-commit hook (`cargo xtask mr`) and CI
(`.github/workflows/boundary.yml`, every push and pull request) run it.
