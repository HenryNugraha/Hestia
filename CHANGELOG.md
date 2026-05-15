# Changelog

## [1.4.1] - 2026-05-15

### Changed
- Clarified Hestia's independent project status in README wording.
- Renamed app subtitle and installer shortcut name from "XXMI Mod Manager" to "Mod Manager" to avoid implying affiliation with XXMI projects.
- Added installer cleanup for the old "Hestia - XXMI Mod Manager" shortcut name.

## [1.4.0] - 2026-05-15

### Added
- Categories can now be sorted automatically.
- Enabled managing categories via new Settings > Category tab.
- Added deep-scan mode to resolve path detection issues.
- Added unintrusive quick feedback form.

### Changed
- Various minor visual interface improvements: dim screen when game switcher is open, added icons on child windows, separated Tools and Log buttons from using the same icon.
- Unchecking "Use default XXMI mod path for games" now prefil the mods folder input bars with current value.

## [1.3.0] - 2026-05-09

### Added
- Added auto-creating categories based on GameBanana category when downloading mods in Hestia.
- Added "+ Add Note" button on a mod without description and metadata.

### Changed
- Improved async threading that handles caption when opening images in fullview mode.
- Improved Linux compatibility, including refactoring fonts (now uses Selawik by default instead of Segoe UI).

### Fixed
- Fixed renaming a mod that has an update will sometime cancel the renaming and download the update instead.
- Fixed bulk selecting mods in MY MODS tab when grouping mods by mod state.

## [1.2.0] - 2026-05-03

### Added
- Added a new mod state "Check Skipped" for linked mods that are not checked for update.
- Added descriptive tooltips when hovering over mod states.
- Added support to manually add images for unlinked mods.
- Added a "What's New" window that shows after an update to show highlighted changelogs.

### Changed
- Improved accuracy in detecting whether the mods in BROWSE is installed.
- Reworked download process to better handle disconnections and allow resuming downloads.
- Mods' metadata will now be shown as Description if they don't have it.

### Fixed
- Fixed the checkbox "Ignore update once" reverting back to unchecked immediately when enabled.
- Fixed the caret (blinking cursor) mistakenly aligned to the right on some input fields.

## [1.1.1] - 2026-05-01

### Added

- Added app-update checks so Hestia detects protected install folders before attempting self-update.

### Changed

- Changed the installer to install per-user under `%LOCALAPPDATA%\Programs\Hestia` by default.
- Improved app state loader so when existing data detected, Hestia will attempt to load them first instead of creating new app state files.

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
