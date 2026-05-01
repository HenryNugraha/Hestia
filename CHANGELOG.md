# Changelog

## [1.1.0] - 2026-05-01

### Added

- Added separate launch behavior for tools in Settings > General > Interface.
- Added support for copying GameBanana IDs from mod details in both MY MODS and BROWSE.
- Added actions for category assignment and update preferences into mod card context menu.
- Added options for how Hestia handles updates for locally modified mods.
- Added drag reordering for the game switcher.

### Changed

- Reworked update preferences under mod SOURCE to use "Ignore update once" and "Ignore update always".
- Reworked modified mod update behavior so they can show update availability without losing their modified status.
- Reworked exact file-set update handling so it is used internally for split-folder mod installs for simplicity.
- Improved update checking to reduce unnecessary GameBanana JSON requests.
- Renamed titlebar launch actions to "Play with mods" and "Play without mods".
- Renamed "Extracted Metadata" to "Metadata" in mod detail window.
- Reworked metadata extraction to allow selecting alternative source files if available.
- Reworked path auto-detection for XXMI and all games.
- Adjusted Settings > Game & Path grouping width.

### Fixed

- Fixed missing categories from mod metadata when the category doesn't exist by recreating it.
- Fixed category grouping behavior so an all-uncategorized library does not show a redundant category section.
- Fixed category drag reordering in mod details.
- Fixed missing drag insertion lines at the top and bottom of category lists.
- Fixed duplicated mod folders sharing the same stored UID.
- Fixed right-pane child windows distorting while the app is minimized.
- Fixed modified-state detection for disabled mods.

## [1.0.0] - 2026-04-26

### Added

- Initial public release of Hestia.
