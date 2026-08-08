# Changelog

## [1.8.0-alpha] - 2026-08-xx

### Added
- profiles
- Added "Open profile folder" to the profile menu, opening the folder where the selected game's profiles are stored.
- Profiles are now stored under readable names like `Patch 1.4 [1fe9ec7a].tzst` instead of a bare ID, and renaming a profile renames its files. The bracketed code is the profile's ID, so two shared profiles that share a name stay distinct. Existing profiles are renamed on first launch, and a profile archive copied in from another install is picked up automatically.
- Tools now belong to a profile. Each profile keeps its own tool list, launch options, titlebar pins and ordering, and its own list of removed auto-detected tools; switching profiles restores them. Existing tools are copied into every profile of that game on first launch.

### Changed
- Blocked profile switching while a tool inside the profile folder is still running, instead of letting the switch fail partway.
- Deleting a profile now shows the same looping progress bar used while installing mods, rather than a percentage that sat at 0% until the delete finished.
- Removed the spinner beside the active row in the profile switching dialog; the row's own progress bar already shows the work is running.

### Fixed
- Fixed the Retry button in the Task window doing nothing for a mod install that failed or was canceled at the overwrite prompt.
- Fixed profiles stored on disk but missing from the profile list being invisible and unreachable forever, with no way to switch to them or reclaim their disk space. Startup recovery now restores them from the metadata embedded in the profile itself.
- Fixed deleting a profile leaving copies of it behind, which reappeared as profiles on the next launch. Duplicate copies of a profile are now reported, and identical ones can be deleted to reclaim their space.
- Fixed profile switching discarding tool launch options and titlebar pins, and re-adding the tools as new entries on switching back.
- Fixed duplicating a profile reporting "Extracting selected profile" while it was actually copying files, sitting frozen at 20% for the whole copy, and briefly running the progress bar backwards when the source was a compressed profile.
- Fixed removing an auto-detected tool hiding a tool at the same path in every other profile of that game.

## [1.7.1] - 2026-07-26

### Added
- Added support for installing split archives.

### Changed
- Disabled dragging mods when already inside a category folder.
- Refined further the logic in selecting files for mod auto-update.
- Changed ESC button behavior to close context menu first before closing any window.
- Enabled clearing selected mods by pressing ESC.

### Fixed
- Fixed heavy stuttering when playing game (hopefully).
- Fixed scrolling in fullview mode skipping several images at once when the system mouse scroll step is set high.
- Fixed potential issue causing downloaded mod to be installed in an Unlinked state.
- Fixed issue in console log preventing scrolling down.
- Fixed Task window jittering when scrolling to the bottom edge.
- Fixed mod detail metadata is not showing at all when "Always Show" is selected.
- Fixed left column width on Settings > General > Behavior being too narrow.
- Fixed filtering by characters not showing all mods.
- Fixed mod card's context menu accessible from header.
- Fixed scrolling through images in fullview mode skipping some images.
- Fixed mods staying selected, and the mod detail window staying open, after assigning them to a category from a context menu (dragging them into the folder already cleared both).
- Fixed mods selection not being cleared after moving them to a category.

## [1.7.0] - 2026-07-12

### Added

- Added Russian localization (ru-RU).
- Added support for proxy settings.
- Added the mod folder size to the mod details window, next to its category label.
- Added explanations of the tools available in the Tools window.
- Added an option to hide empty category folders.
- Added an option to automatically translate mods when they are opened.
- Added the F7 hotkey to translate a mod when the mod details window is open.
- Added the Esc hotkey to close the currently focused window.
- Added the Ctrl+W hotkey to close the currently focused window.
- Added the Ctrl+Shift+W hotkey to close all windows.
- Added Page Up, Page Down, Home, and End hotkeys to most windows.
- Added two new font sets: Elegant and Traditional.
- Added a button in the header to open the mods folder.

### Changed

- Upgraded Rust and dependency versions.
- Refactored much of the code to improve performance.
- Slightly adjusted the UI to accommodate Russian text, which tends to be longer.
- Adjusted the lookup order for the `hestia.toml` configuration file to prioritize any existing file.
- Revamped the guide shown when no games are enabled.
- Formatted like and download counts.
- Added a cooldown period before automatically checking for mod updates to avoid hammering GameBanana's servers.
- Enabled the translation service to translate metadata.
- Enabled font changing on non-Windows platforms.
- Adjusted the log window to allow text selection.
- Improved file tracking when checking for updates to mods with multiple files.
- Improved the handling of category folder and mod deletion.
- Tweaked the control buttons when Hestia is maximized.
- Refined the context menu to allow opening a mod's GameBanana page either within Hestia or in the system's default browser.
- Renamed "Game & Path" to "Games" in the Settings menu.

### Fixed

- Fixed the image decoder becoming stuck when receiving unexpected input, which prevented further images from being rendered until Hestia was restarted.
- Fixed the initial mod scan not being triggered after a deep scan was completed.
- Fixed the Tools window continuing to show the last selected game when all games were disabled.
- Fixed missing translations for some strings.
- Fixed mods for a specific character not being sorted by date when "Recent Updated" was selected.
- Fixed tools launched from Hestia being forcibly closed when Hestia exited.

## [1.6.0] - 2026-06-15

### Added
- App localization: Bahasa Indonesia (id-ID).
- App localization: Simplified Chinese (zh-CN).
- Auto‑switches language based on detected system locale
- Added experimental translate button on the mod detail window.
- Added support for resuming downloads.

### Changed
- Improved icon rendering in filter button's context menu.

## [1.5.0] - 2026-06-07

### Added
- New folder-style category layout.
- Filtering mods by specific character in Browse tab.
- Added hotkey CTRL+N to create new category when Settings > Category is currently open.
- Added new option to download a mod and install it in disabled state.
- Added a folder icon next to the mod's name on mod detail to open it in File Explorer.

### Changed
- Added more option to the sort button's menu.
- Disabled mods now stay disabled when updated.
- Improved illegal file name handling when installing mods from a zip or folder.
- Further improved download fails handling.
- Further improved path handling when auto-detecting paths.
- Further improved checkboxes selection logic.
- Now auto selects the whole text when creating and renaming a category or mod's name.
- Added more details into survey's privacy policy.
- Delayed survey window more when dismissed.
- Reorganized settings menu.

### Fixed
- Fixed context menu on mod cards still showing even when right clicking on header or title area.
- Fixed losing focus when renaming a category on mod detail window.
- Fixed JSON handling when searching for mods on Browse tab.
- Fixed updates redownloading non-stop, maybe.
- Fixed black screen on first time launch.

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
