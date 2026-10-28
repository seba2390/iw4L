# Game library

Start `iw4l` without a map argument to open the game library. It needs no
game data: it lists the installed games (Modern Warfare (2007), Modern
Warfare 2, Black Ops, Black Ops 2, Black Ops III), and offers a folder scan when
none is found. Each game opens its own menu with Multiplayer and Campaign; a
game whose crate offers a zombies mode (`game_api::GameModes::zombies`) also
gets Zombies, listing those of the mode's maps the installation carries and
the mode's note (Black Ops 2: Origins runs rules written for IW4L).
Campaign displays Work in Progress.

Game Installations: Scan a Folder for Games picks one folder and finds every
game under it (a few levels deep, five seconds at most); each game's own row
picks its folder by hand. Choices are validated, saved and rescanned. Empty
choices use normal game discovery.
The library has no settings page: each game's own menus hold its settings.
A game with a startup cinematic (Black Ops: `number_lady_intro`) plays it
full screen while its front end loads behind it; any key or mouse button
skips it, and a hint says so after 3 s.
Use the mouse, arrow keys, Enter and Escape; the mouse wheel scrolls long lists.

Multiplayer provides map selection, mode and limits, lobby creation, privacy,
start and leave controls, and the community server browser.
BO2 exposes native Free for All and Team Deathmatch. Its complete multiplayer feature
set remains experimental; see [T6.md](T6.md).
In a BO2 match, Escape opens Resume, Create-a-Class, settings and leave controls.
The host can end a match and return the group to its lobby to change map/rules.
The lobby supports passwords; protected browser entries prompt before joining.
Modern Warfare (2007) has installation recognition and menus; multiplayer
requires an asset reader that is not implemented yet. Black Ops multiplayer is
refused; its Zombies runs ([BO1-ZOMBIES.md](BO1-ZOMBIES.md)).

The library opens before game asset preparation. MW2 scripted menu assets
are prepared asynchronously after selecting its profile.
First-person preparation uses a larger frame budget while a map is loading.
BO2 menu artwork (multiplayer and Zombies backgrounds, button backings and a
native bitmap font) preloads as soon as the installation is found; a page waits on
a blank screen until it is ready. BO2 Zombies matches show the game's own loading
screens and need a captured patch zone ([BO2-ZOMBIES.md](BO2-ZOMBIES.md)). The
window size is set in desktop points. No game assets are redistributed.
Other profiles use the generated fallback backdrop, documented
in [the asset notes](../crates/ui/assets/launcher-background.md).
