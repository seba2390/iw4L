# Game library

Start `iw4l` without a map argument to open the game library.
Choose Modern Warfare (2007), Modern Warfare 2, Black Ops or Black Ops 2.
Each profile opens its own menu with Multiplayer, Campaign and settings;
Black Ops profiles also include Zombies. BO2 offers experimental survival;
Campaign and Black Ops Zombies display Work in Progress.

Game Installations opens the folder picker for each profile. Choices are
validated, saved and rescanned. Empty choices use normal game discovery.
Settings apply through the runtime settings service and save automatically.
Use the mouse, arrow keys, Enter and Escape; the mouse wheel scrolls long lists.

Multiplayer provides map selection, mode and limits, lobby creation, privacy,
start and leave controls, and the community server browser.
BO2 exposes native Free for All and Team Deathmatch. Its complete multiplayer feature
set remains experimental; see [T6.md](T6.md).
In a BO2 match, Escape opens Resume, Create-a-Class, settings and leave controls.
The host can end a match and return the group to its lobby to change map/rules.
The lobby supports passwords; protected browser entries prompt before joining.
Modern Warfare (2007) has installation recognition and menus; multiplayer
requires an asset reader that is not implemented yet. Black Ops needs an
owned installation and has not been validated on this machine.

The library opens before game asset preparation. MW2 scripted menu assets
are prepared asynchronously after selecting its profile.
First-person preparation uses a larger frame budget while a map is loading.
BO2 menu artwork loads asynchronously from the selected installation: multiplayer
and Zombies backgrounds, button backings and a native bitmap font. No game assets
are redistributed. Other profiles use the generated fallback backdrop, documented
in [the asset notes](../crates/ui/assets/launcher-background.md).
