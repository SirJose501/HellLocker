# HellLocker

**HellLocker** is a Helldivers 2 loadout tool by **Sir Jose**, built with Claude Opus
(Anthropic's AI model). Save your whole loadout (Stratagems, Booster, weapons,
grenade and armor) once, then apply it with one click or a hotkey, with presets
sorted by **faction**, **sub-faction** and **mission type**. It is a standalone
Windows app: no mod manager or game files involved.

HellLocker is a modified version of [HD2 Preset Helper](https://github.com/xmg228/hd2-preset-helper)
by xmg228 (GPL v3), extended with HellLocker's faction, sub-faction and mission
system, picture library, search, and a new interface. See
[Based on HD2 Preset Helper](#based-on-hd2-preset-helper) for details.

The app recognizes the game interface through screen capture and makes
selections with ordinary mouse and keyboard input. It does not modify game files,
inject code into the game, read game memory or alter network traffic. It is an
unofficial tool, not affiliated with or endorsed by Arrowhead Game Studios or Sony
Interactive Entertainment.

**No internet access.** HellLocker never connects to the internet: it contains no
network code and no update checker, and sends nothing anywhere. You can block it in
Windows Firewall and it works exactly the same. The exe is not packed or obfuscated,
so it can be inspected, and it can be built yourself from this source.

## Build

1. Install Rust from <https://rustup.rs> and Visual Studio Build Tools with
   **Desktop development with C++**.
2. Double-click **`Build HellLocker.bat`** in this folder. (Or open PowerShell here
   and run `powershell -ExecutionPolicy Bypass -File .\Build-Windows.ps1`.)

3. It runs the tests, then builds. The app is written to the `HellLocker` folder
   (`HellLocker\HellLocker.exe`), and `HellLocker-<version>.zip` next to it holds
   that folder, ready to upload. File Explorer opens with the zip selected.

Or push this folder to a GitHub repository and run **Actions → Build HellLocker
(Windows)**, which produces a downloadable zip.

## Use

1. Run `HellLocker.exe`. The panel opens; afterwards it lives in the system tray.
2. Press `Ctrl+Alt+Enter` (changeable in Settings) to show or hide the panel.
   It stays up when you click into the game. Drag it by its title bar (for
   example onto a second monitor) and it opens there from then on.
3. Pick a **faction** and **mission** at the top left, or cycle them with
   **A / D** (faction) and **Q / E** (mission) while the panel has focus.
4. **+ New preset** makes a preset for the faction and mission shown. Rename it
   with the pencil, and change what it is for with the **For** boxes.
5. On a loadout page in game, with the loadout you want equipped, select the
   preset and press **Save preset** (`Ctrl+S`). Release all keys and leave the
   mouse alone while it works; it clicks through the equipment pages to read them.
6. To apply, select the preset and press **Apply all** (`Enter`), or use the
   preset's own shortcut (Settings → Shortcuts).

A preset set to **Any faction / Any mission** is "General": it shows under every
filter. Choosing **All factions / All missions** shows every preset.

**Settings → Panel** has **Keep the panel open** (on by default) and **Panel
size** (100 % to 200 % on top of the Windows display scale; 150 % by default,
applied when HellLocker restarts). If the panel is over the game when you save or
apply, it hides while the clicks happen and comes back afterwards; on another
monitor it just stays put.

If saving or applying stops partway on a slower PC (menus open slowly, or the game
hitches), turn on **Settings → Gameplay → Patient mode**: HellLocker then waits up
to three times longer for the game. It never clicks faster or more; it only waits
longer before giving up.

Use **Borderless Window** in game, at the same resolution as your desktop.

### Coming from DiverKit

HellLocker is DiverKit renamed. Quit DiverKit (tray icon > Exit), then copy its
whole `data` folder next to `HellLocker.exe` before starting HellLocker the first
time; HellLocker moves it into `%APPDATA%\HellLocker`. Presets, tags, shortcuts,
panel position and size all carry over.

### Bringing presets over from HD2 Preset Helper

Copy `config.toml`, `presets.json` and the `local_templates` folder from that
app's `data` folder into `%APPDATA%\HellLocker` (start HellLocker once first so
the folder exists, then close it from the tray). Old presets show up as General;
set their faction and mission with the For boxes.

## Linux and Steam Deck (experimental)

HellLocker is a Windows program, so on Linux it runs under Wine / Proton like the game.
Two things matter:

1. **It has to run inside the game's own Proton prefix**, while the game is running.
   A program started in any other Wine prefix can't see the game at all, which is why
   HellLocker says "Helldivers 2 is not running". With
   [protontricks](https://github.com/Matoking/protontricks) installed, start the game,
   then run:

   ```
   protontricks-launch --appid 553850 /path/to/HellLocker.exe
   ```

   With the Flatpak version of protontricks:

   ```
   flatpak run --command=protontricks-launch com.github.Matoking.protontricks --appid 553850 /path/to/HellLocker.exe
   ```

2. **Use an X11 desktop session** (Linux Mint Cinnamon uses X11 by default). HellLocker
   copies the game's picture from the screen; under Wayland the copy comes out black.
   On the Steam Deck, Desktop Mode may work; Game Mode is untested.

Settings > Help > **Check my setup** says when HellLocker is running under Wine and
whether it can see the game. HDR isn't supported on Linux. If something doesn't work,
attach `app.log` from the data folder (inside the prefix:
`drive_c/users/steamuser/AppData/Roaming/HellLocker`) to a bug report.

## Files

Everything is kept in `%APPDATA%\HellLocker` (type that into the Explorer
address bar to open it), so updating means replacing the app folder; your presets
stay. Builds before 1.9.0 kept a `data` folder next to `HellLocker.exe`; the first
start of 1.9.0 copies it over, tells you, and renames the old one to
`data (moved to AppData)`, which you can delete.

For a portable install (everything in `data` next to the exe, e.g. on a USB
stick), put an empty file named `portable.txt` next to `HellLocker.exe`.

| File | What it holds |
|---|---|
| `config.toml` | Presets' names, shortcuts, faction and mission; settings |
| `presets.json`, `local_templates\` | The saved loadouts |
| `icons\` | Your own preset icons (PNG pictures) |
| `app.log` | The log; include it with a bug report |
| `last_failure.zip` | Details of the last save/apply that failed |
| `backups\before-restore.zip` | Your presets as they were before the last restore |

**Settings → Help** opens this folder, backs up your presets (and pictures and
settings) to one zip, and restores from one. To report a bug, use **Report a bug**
there (it opens the Bugs tab of the HellLocker Nexus page) and attach `app.log`
and `last_failure.zip`; **System info → Copy** gives details worth pasting in.

## Changes in 1.9.7

### Linux and Steam Deck (experimental)

- HellLocker now runs under Wine / Proton: it copies the game's picture from the screen
  there (Windows' own capture doesn't exist under Wine). See "Linux and Steam Deck"
  above for how to start it inside the game's Proton prefix.
- When it can't find the game under Wine, it says to start it inside the game's prefix
  (with the protontricks command) instead of "Helldivers 2 is not running".
- Check my setup shows that it is running under Wine, and the Wine version is in
  `app.log`.
- A black screen copy (Wayland) is noted in `app.log`.

## Changes in 1.9.6

### Folder logos

- Each folder can show a logo instead of the folder symbol: hover the folder and click
  the picture button next to rename and delete. It uses the same picker (and your own
  icons); **Default** goes back to the folder symbol.

### Two columns of presets

- The preset list shows presets two to a line, so about twice as many fit (around 20
  with a folder or two open). Rows are a little shorter.
- The Stratagems no longer show on each row; they show on the right for the selected
  (or hovered) preset. A preset with nothing saved shows "Not saved" under its name.
- Up / Down (and W / S) move to the preset above or below; Left / Right move across.
  A / D still change the faction filter.
- Dragging drops a preset before or after the one under the mouse (left or right
  half), or into a folder by dropping it on the folder's header, as before.
- The panel is a little wider to make room (1320 x 820, or 702 wide collapsed).

### Icon picker

- The icon picker now has the Settings window's look: striped black with a yellow frame
  and corner brackets.

### Equipment cards

- Item pictures are bigger: their plain margins are trimmed and the picture fills the
  card (the cards are the same size), with the name over its foot.
- Item names show in the panel's own font. HellLocker reads the name from the saved
  picture of it with Windows' built-in text recognition, once per name (kept in
  `library\names.json`). If Windows can't read it (for example no text recognition
  for your Windows language), the card shows the picture of the name as before.

### Fixes

- Folders could get stuck open and Settings would not open. Double-clicking a folder
  header (for example clicking it twice to close and reopen it) started renaming the
  folder, which blocked both. Double-click no longer renames (use the rename button
  that shows when you hover the folder), and Settings ends a rename in progress.
- With a faction or mission filter set, folders (and Favourites / Recently used) can
  be closed again; only a search opens them all.
- A new folder made while a filter is set no longer disappears while you name it.

## Changes in 1.9.5

### Preset icons

- **Two icons per preset**, shown beside its name in the list. Pick them with the two
  **Icons** buttons next to Faction, Sub-faction and Mission on the right.
- The first icon is the faction's emblem (or the sub-faction's) until you pick one;
  the second is empty until you pick one. **Default** and **None** in the picker go
  back to that.
- The picker has every faction and sub-faction emblem, the four mission types, 51
  mission objective and operation icons, 111 Stratagems, 20 Boosters and 12 planet
  conditions (storms, fog, heat and so on), with a search box and groups.
- **Your own icons**: put PNG pictures in the `icons` folder of the data folder
  (the picker's **Open folder** button opens it). They show under **Your icons**
  and are included in backups.
- Copies made with Duplicate keep the icons.

Stratagem icons: the SVG set by nvigneux,
[Helldivers-2-Stratagems-icons-svg](https://github.com/nvigneux/Helldivers-2-Stratagems-icons-svg),
shared for use in other projects. Mission, operation, Booster and planet condition
icons: from the [Helldivers Wiki](https://helldivers.wiki.gg/wiki/Category:Helldivers_2_-_Icons)
(many traced by Dogo314).

## Changes in 1.9.4

### Preset list

- **Favourites**: click the star on a preset to pin it in a Favourites section at the
  top of the list (it stays in its folder too).
- **Recently used**: the last five presets you applied, in a section under Favourites.
  Both sections open and close like folders and follow the filters and search.
- **Duplicate preset**: the copy button next to rename and delete (or **Ctrl+D**) makes
  a copy with the same loadout, pictures, tags and notes, right under the original,
  and starts renaming it. The copy gets no shortcut.
- **Notes**: every preset has a notes box under Stratagems & Booster (what it's for,
  what to bring, who runs what). Saved when you click away.

### Panel

- **Hide the details**: the tab on the divider folds the right side away, leaving just
  the presets and filters in a narrow panel (the action buttons move to their own row).
  Click it again to bring the details back. HellLocker remembers which you used.
- **Stop an action**: press **Backspace** (or the panel's **Stop** button) to stop a
  save, apply or scan. Nothing more is clicked, and it shows as stopped, not failed.
- The progress pop-up now has a progress bar and shows the stop key.

### Equipment scan

- **Scan resume**: if a scan stops partway (you stopped it, the game lost focus, or
  something went wrong), pressing **Scan equipment** again carries on where it left off.
  Finished equipment slots are skipped, and items already pictured in the slot it
  stopped in are passed without waiting for their pictures again. An unfinished scan
  is kept for three days; after that, or at another game resolution, a scan starts over.

### Help

- **Check my setup** (Settings → Help) checks:
  - the game's window mode and resolution, against your display;
  - HDR;
  - whether the equipment has been scanned;
  - overlays that draw over the game (FPS counters, Steam overlay and others);
  - the mouse wheel setting;
  - whether the game runs as administrator.
  It says what to change. Nothing is clicked.

### Languages

- German, French, Spanish (Spain and Latin America), Italian, Japanese, Korean, Polish,
  Portuguese (Brazil and Portugal) and Russian now cover all of HellLocker's own text:
  folders, the new Settings window and everything above. Faction, sub-faction and
  mission names stay in English in those languages until the in-game names are added.

## Changes in 1.9.3

### New Settings window

- Redesigned to match the panel: striped black with a yellow frame, and four tabs
  (**Gameplay**, **Panel**, **Shortcuts**, **Help**). Each setting is a card with
  an icon and a line saying what it does; switches show on and off at a glance,
  Panel size is a row of buttons, and the Background slider shows its percentage.
- Renamed: **Auto ready up** is now **Automatic Readying**, and **Save fallback
  Booster when taken** is now **Booster Fallbacking**.
- Every tab has **Reset this tab** to go back to the defaults (saved when you press
  Save).
- Shortcuts show as keycaps. A shortcut used twice is marked in red on both rows.

### New settings

- **Patient mode** (Gameplay): waits up to three times longer for the game, for PCs
  where menus open slowly or the game hitches. Off by default.
- **Sound when saved** (Gameplay): turn off the chime after saving a preset.
- **Start with Windows** (Panel): starts HellLocker in the tray when you sign in
  (for your Windows account only; nothing is installed).
- **Status pop-up** (Panel): show the progress notice at the top right (as before),
  the top left, or not at all.

### Building

- **`Build HellLocker.bat`**: double-click it in the source folder to test and
  build. It also makes `HellLocker-<version>.zip`, ready to upload.

### Help tab

- **Report a bug** opens the Bugs tab of the HellLocker Nexus page.
- **Open folder** opens the data folder (presets, settings, `app.log`,
  `last_failure.zip`).
- **System info → Copy** copies the HellLocker and Windows versions, display sizes
  and scale, the game's HDR and brightness settings and HellLocker's settings, for
  a bug report. It holds no names or file paths.
- **Back up presets** saves every preset, its pictures and your settings into one
  zip. **Restore from a backup** checks the zip, keeps what you have now in
  `backups\before-restore.zip`, swaps the backup in and restarts HellLocker.

## Changes in 1.9.2

- Fixed: opening the panel muted Discord (or set off other programs' Alt keybinds).
  The window library HellLocker uses tapped a fake Left Alt to bring windows to the
  front; HellLocker now brings the panel forward without any key presses.
- Fixed: applying Stratagems could fail with "neither the page turn nor the page after
  a recovery scroll could be placed in the temporary list map". The Stratagem list
  fades in when opened, and HellLocker could take a frame from the very start of the
  fade, when the list was barely visible, for the open list. It then mapped the
  loadout page behind it and could never line up the real list. A list now only
  counts as open once its slots are clearly visible.
- Logs Windows' "lines to scroll at a time" mouse setting when it differs from the
  default 3, to help with list scrolling reports.

## Changes in 1.9.1

**Folders**
- **+ New folder** beside + New preset. Name it as it appears, rename it with the
  pencil (or a double-click), delete it with the bin: its presets are kept and move
  back to the list. Click a folder to open or close it.
- Drag a preset by its handle (⠿) onto a folder to move it in, between two presets
  to put it there, or below the last row to take it out of its folder. The list
  scrolls while you hold a preset near its top or bottom.
- With a faction, sub-faction or mission filter (or a search), a folder only shows
  when it holds a matching preset, and then shows those presets, with a count like
  "2 / 3". Presets in no folder show below the folders as before.
- Kept in `config.toml` as `folders` and `order` under `[presets]`.

**New look**
- Right side redesigned: a header card with the preset's emblem, name and tags;
  labelled Faction / Sub-faction / Mission boxes; section bars with framed bodies;
  six gear cards (weapons, then attire) with the Helmet / Armor / Cape boxes on
  their cards; and numbered Stratagem slots with the Booster set apart. Equipment
  comes first, with Stratagems & Booster below it.
- **Save preset** and **Apply all** moved to the top beside **Scan equipment**;
  **+ New preset** sits under the filters; **Settings** is at the bottom right.
- The show / hide shortcut is shown at the top beside the version.

**Changes**
- New installs use **Ctrl + Alt + Enter** to show or hide the panel (was Ctrl +
  Shift + Space). An existing config keeps its shortcut; change it in Settings >
  Shortcuts.
- Simplified and Traditional Chinese for everything HellLocker added: filters,
  faction, sub-faction and mission names, search, the new layout and the new
  settings. Faction and sub-faction names follow the Chinese Helldivers wiki. The
  small status pop-up during save / apply stays in English, as before. Pick the
  language in Settings > General.

**Fixes**
- Slower PCs: saving, applying and Scan equipment wait up to 4 seconds (was 1.5) for
  the game to open an equipment list or respond to a click, fixing "equipment list
  and its equipped focus were not confirmed" and "hover was not confirmed". Fast
  PCs carry on as soon as the screen is confirmed, so they are not slowed down.
- PCs where each screen check is slow (copying the game's picture can take seconds
  when the graphics card is fully busy): the equipment waits stretch to match, and
  an unanswered input is only repeated after the screen has been checked again. The
  log notes it once ("screen checks are slow on this PC"). Capping the game's frame
  rate usually makes the checks fast again.
- The Faction / Sub-faction / Mission boxes follow the preset under the mouse, like
  the rest of the preview, instead of always showing the selected one.

## Changes in 1.9.0

- Sub-faction filter beside the faction filter, and a sub-faction box in each
  preset's **For** row (`subfaction` in each `[presets.slots.preset_N]` table and
  `view_subfaction` in `[ui]`; "any" means the whole faction). Terminids: Predator,
  Spore Burst and Rupture Strain; Automatons: Jet Brigade, Incineration Corps and
  Cyborg Legion; Illuminate: Mindless Masses, Appropriators and Vote Snatchers.
- A preset for a whole faction (or general) still shows under any of its
  sub-factions. Changing a preset's faction clears its sub-faction.
- Presets for a sub-faction show its emblem in the list.
- The mission filter sits beside the search box.
- Presets, settings and pictures are kept in `%APPDATA%\HellLocker` instead of
  next to the exe, so updates can't lose them. An existing `data` folder is moved
  there on the first start. `portable.txt` next to the exe keeps the old behaviour.
- Builds made with `Build-Windows.ps1` no longer contain the folder paths of the
  computer they were built on (your user name and source folder are replaced by
  neutral names).
- `HellLocker.exe` runs on its own: the C runtime is linked in, so players don't
  need the Visual C++ Redistributable.
- Fixed: after the list was rebuilt (a filter change, for example) the right side
  could keep showing the preset that had been under the mouse, not the selected one.

## Changes in 1.8.0

- The preset list is one slim horizontal row per preset: larger faction and
  mission emblems, the name and shortcut, then the Stratagems and Booster at a
  larger size. Equipment pictures and names are only shown on the right, so ten
  or more presets fit at once.
- The faction and mission filters sit side by side; the A/D and Q/E key hint is
  beside **+ New preset**.
- The panel is a little wider (1200 by 820 before scaling).

## Changes in 1.7.1

- Every button and dropdown now has a black face with faint light-gray diagonal
  lines and a yellow outline; text and icons are yellow (about 10:1 contrast, so
  they stay easy to read). Apply all is striped too, with a thicker outline and
  bolder text so it still stands out.

## Changes in 1.7.0

- Faction emblems for Terminids, Automatons and Illuminate (transparent PNGs in
  `assets\faction-*.png`, made from SVGs traced by Dogo314 and supplied by the
  project owner) in the filters, the For box and on each preset. Faction text uses
  the emblems' colors.
- **Search presets** box above the list: type part of a name (or "preset 3" for an
  unnamed one). It works together with the faction and mission filters; Up/Down
  still move through the list, Esc clears it. Preset shortcuts pause while typing.
- Buttons have a navy face with diagonal olive-yellow stripes, a bright yellow
  outline and yellow text and icons. The yellow Apply all stays solid.
- New installs start at 150 % panel size (Settings → Panel size changes it).

## Changes in 1.6.0

- **Scan equipment** button (top right). It opens each of the six equipment lists
  in turn, walks every item (the same route the app uses to learn a list), waits
  for each tile's picture to finish loading, and keeps the picture with the item's
  name in `data\local_templates\library\`. Start it from a loadout page and leave
  the game alone until the status says the scan is done; it takes a few minutes.
  Each list is saved as soon as it is finished, so an interrupted scan keeps the
  lists it completed.
- With a scan done, saving a preset takes each item's picture from the library
  instead of waiting for the tile to load, and presets saved without pictures show
  them from the library. Items the scan did not see (new gear after an update)
  are still pictured the old way; run the scan again to add them.
- Stratagems are not scanned: they are read in one capture of the loadout page,
  with nothing to load, so a library would not save any time.

## Changes in 1.5.0

- Separate **Helmet**, **Armor** and **Cape** boxes in the Equipment section
  (`helmet`, `armor`, `cape` in `[presets]`), in place of the single 1.4.0 switch,
  which is converted automatically. An unticked slot is left as it is when saving
  (the preset keeps what it had) and when applying (you keep what you wear).

## Changes in 1.4.0

- Panel font changed to Liberation Sans Bold; the Settings dialog uses the plain
  Windows font (Segoe UI) so it stays easy to read.
- **Helmet, Armor & Cape** checkbox in the Equipment section (`attire` in
  `[presets]`, on by default). Off, saving and applying cover only the primary,
  secondary and grenade: saving keeps the preset's saved attire, and applying
  leaves what you are wearing alone. This also applies to preset shortcuts. A
  preset that has never been saved with its attire is saved in full the first time.

## Changes in 1.3.1

- Font changed to Victory Striker Sans Demo.
- Saving waits for each item's picture to finish loading in the game (the tile
  stops changing; at most 3 seconds per item) before keeping it, instead of
  catching the loading ring. Save presets again to replace half-loaded pictures.

## Changes in 1.3.0

- New HL logo (header and app icon).
- The panel uses a bundled font for Latin languages (Liberation Sans Bold since
  1.4.0, `assets\LiberationSans-Bold.ttf`); Chinese, Japanese and Korean keep their
  Windows fonts.
- Bright yellow outlines on every button.
- Smaller equipment names, to save space.
- **Settings → General → Background**: a 10–100 % slider for how solid the
  background art is (`background_opacity` in `[ui]`, 85 by default). It previews
  as you drag; Save keeps it.
- Everything sits further in from the frame art.

## Changes in 1.2.1

- Saving a preset now also keeps a picture of each equipped item (from the
  item list's highlighted tile, `N-picture.png` beside the name templates). The
  pictures show next to the names in the Equipment section and as a strip on
  each preset in the list. Presets saved before 1.2.1 show names only until
  they are saved again.

## Changes in 1.2.0

- Renamed to HellLocker (`HellLocker.exe`), with its own logo, name, framed
  background and app icon, and the version shown next to the name.
- Each preset in the list shows its weapons and grenade, and its helmet, armor
  and cape, under the Stratagems.
- Faction icons in faction colors: Terminids yellow, Automatons red, Illuminate
  purple.

## Changes in 1.1.0

- The panel stays open when you click into the game (`keep_open` in `[ui]`), and
  remembers where you drag it (`panel_position`).
- Panel size setting (`ui_scale`, 1.0 to 2.0), 2.0 by default.
- More solid panel background.
- Icons for each faction and mission type, in the filters, the For boxes and on
  each preset.

## Based on HD2 Preset Helper

HellLocker started from [HD2 Preset Helper](https://github.com/xmg228/hd2-preset-helper)
0.3.3 by xmg228. Its screen reading and gear selection engine is kept from that
app; the changes listed in this README are HellLocker's own.

### Changes from HD2 Preset Helper 0.3.3 (October 2026)

- Faction and mission tags on every preset (`faction` and `mission` in each
  `[presets.slots.preset_N]` table; old configs load as General).
- Faction and mission filters in the panel, remembered between runs
  (`view_faction`, `view_mission` in `[ui]`), with A/D and Q/E to cycle them.
- New presets take the filter's faction and mission; the For boxes retag one.
- Renamed to HellLocker (`HellLocker.exe`), with its own single-instance lock and
  data folder, so it can be installed beside HD2 Preset Helper. Don't run both at
  once: they use the same default shortcuts.

## Credits

- **Sir Jose**: creator, design, artwork and testing.
- **Claude Opus** (Anthropic's AI model): wrote the code with Sir Jose.
- **xmg228**: [HD2 Preset Helper](https://github.com/xmg228/hd2-preset-helper), which HellLocker is built on.
- **Dogo314**: traced the faction and sub-faction emblems.

## Artwork

The HellLocker logo, name and background in `assets\` are the project owner's own
artwork. The faction and sub-faction emblems are the game's faction symbols, traced by Dogo314 and
supplied by the project owner; they belong to Arrowhead Game Studios, so keep that
in mind before sharing builds. The panel font is Liberation Sans Bold (© Ascender Corporation, under the
Liberation font license, which allows redistribution). The Settings dialog uses the
Windows system font, Segoe UI. The faction and mission icons are original line drawings (`src\ui\icons.slint`).

## License

GNU General Public License version 3 or later, as the original. See
[LICENSE](LICENSE).
