# Game library

Start `iw4l` without a map argument to open the game library.
Choose Modern Warfare (2007), Modern Warfare 2, Black Ops or Black Ops 2.
Each profile opens its own menu with Multiplayer, Campaign and settings;
Black Ops profiles also include Zombies. Campaign and Zombies display
Work in Progress.

Game Installations opens the folder picker for each profile. Choices are
validated, saved and rescanned. Empty choices use normal game discovery.
Settings apply through the runtime settings service and save automatically.
Use the mouse, arrow keys, Enter and Escape; the mouse wheel scrolls long lists.

Multiplayer provides map selection, mode and limits, lobby creation, privacy,
start and leave controls, and the community server browser.
BO2 currently exposes native Free for All. Its complete multiplayer feature
set remains experimental; see [T6.md](T6.md).
Modern Warfare (2007) has installation recognition and menus; multiplayer
requires an asset reader that is not implemented yet. Black Ops needs an
owned installation and has not been validated on this machine.

The library opens before game asset preparation. MW2 scripted menu assets
are prepared asynchronously after selecting its profile.
First-person preparation uses a larger frame budget while a map is loading.
The backdrop is original generated artwork; its provenance and prompt are
in [the asset notes](../crates/ui/assets/launcher-background.md).
