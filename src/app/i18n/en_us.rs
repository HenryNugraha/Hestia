const EN_US: [&str; TEXT_KEY_COUNT] = [
    // Window: What's New
    "What's New", // WhatsNewWindowTitle
    "Click to show feedback survey.", // WhatsNewFeedbackSurveyTooltip

    // Window: Feedback Survey
    "Optional", // FeedbackSurveyOptional
    "Submitting...", // FeedbackSurveySubmitting
    "Submit Feedback", // FeedbackSurveySubmitFeedback
    "Dismiss", // FeedbackSurveyDismiss
    "Remind me later", // FeedbackSurveyRemindLater
    "Skip this version", // FeedbackSurveySkipVersion
    "Never ask again", // FeedbackSurveyNeverAskAgain
    "Privacy details", // FeedbackSurveyPrivacyDetails
    "Feedback is submitted anonymously.\nThere is no way to identify or even contact submitters.\nVotes may be published publicly, but messages are private.\nOnly the following data payload will be sent to the survey server:", // FeedbackSurveyPrivacyCopy
    "• Client: Sha256 hash of randomly generated UUID in hestia.toml file\n• Server & Database URL: {server_url}\n• Server Geolocation: Asia Pacific", // FeedbackSurveyPrivacyPayload
    "See survey results here:", // FeedbackSurveyResultsHeader
    "• Ongoing: ", // FeedbackSurveyResultsOngoing
    "• Previous: ", // FeedbackSurveyResultsPrevious

    // Window: Log
    "Log", // LogWindowTitle
    "Log copied", // LogCopied

    // Window: Tasks
    "Tasks", // TasksWindowTitle
    "Ongoing", // TasksOngoing
    "Ongoing ({count})", // TasksOngoingCount
    "No active tasks", // TasksNoActiveTasks
    "Completed", // TasksCompleted
    "Completed ({count})", // TasksCompletedCount
    "No completed tasks", // TasksNoCompletedTasks
    "Downloads", // TasksDownloads
    "Downloads ({count})", // TasksDownloadsCount
    "Installs", // TasksInstalls
    "Installs ({count})", // TasksInstallsCount
    "Failed", // TasksFailed
    "Failed ({count})", // TasksFailedCount
    "No tasks", // TasksNoTasks
    "Queued", // TasksStatusQueued
    "Installing", // TasksStatusInstalling
    "Downloading", // TasksStatusDownloading
    "Canceling", // TasksStatusCanceling
    "Completed", // TasksStatusCompleted
    "Failed", // TasksStatusFailed
    "Canceled", // TasksStatusCanceled
    "Canceling…", // TasksCanceling
    "Cancel", // TasksCancel
    "Retry", // TasksRetry
    "Resume", // TasksResume
    "Starting download…", // TasksStartingDownload
    "Queued…", // TasksQueuedProgress
    "Installing mod files…", // TasksInstallingModFiles
    "Canceling task…", // TasksCancelingTask

    // Window: Tools
    "Tools", // ToolsWindowTitle
    "No game selected", // ToolsNoGameSelected
    "Launch", // ToolsLaunch
    "Set launch options", // ToolsSetLaunchOptions
    "Open Folder", // ToolsOpenFolder
    "Unpin from Titlebar", // ToolsUnpinFromTitlebar
    "Pin to Titlebar", // ToolsPinToTitlebar
    "Remove", // ToolsRemove
    "Add Tool", // ToolsAddTool
    "Tool", // ToolsFallbackLabel
    "No game selected for tool add", // ToolsNoGameSelectedForAdd
    "Tool already added", // ToolsAlreadyAdded
    "Tool added", // ToolsToolAdded
    "Tool removed", // ToolsToolRemoved
    "Only up to 4 tools can be shown in the titlebar for one game", // ToolsTitlebarLimit
    "Titlebar tool limit reached", // ToolsTitlebarLimitReached
    "Tool executable is missing", // ToolsExecutableMissing
    "Tool not found: {path}", // ToolsNotFound
    "Launched tool: {tool}", // ToolsLaunched
    "Could not launch tool", // ToolsCouldNotLaunch
    "Could not open location", // ToolsCouldNotOpenLocation
    "Tool launch options saved", // ToolsLaunchOptionsSaved
    "Tool Added", // ToolsActionAdded
    "Tool Removed", // ToolsActionRemoved
    "Tool Launched", // ToolsActionLaunched

    // Window: Tool Launch Options
    "Set Launch Options", // ToolLaunchOptionsWindowTitle
    "Launch options (ie, -option value -flag)", // ToolLaunchOptionsHint
    "Save", // ToolLaunchOptionsSave
    "Cancel", // ToolLaunchOptionsCancel

    // Window: Dialogs
    "Scanning paths...", // DialogScanningPaths
    "Finding your XXMI and game paths", // DialogFindingPaths
    "Hestia is now deep scanning accessible drives for XXMI and supported games.", // DialogDeepScanningPaths
    "Scan Results", // DialogScanResults
    "Continue", // DialogContinue
    "Stop Scan", // DialogStopScan
    "Stopped", // DialogStopped
    "Not found", // DialogNotFound
    "Searching...", // DialogSearching
    "Found", // DialogFound
    "Choose...", // DialogChoose
    "Multiple found", // DialogMultipleFound
    "Imported Mod", // DialogImportedMod
    "Missing .ini", // DialogMissingIniTitle
    "No recognizable .ini file found in the archive's parent path, archive may contain multiple mods.\nSelect which folder(s) to install:", // DialogMissingIniPrompt
    "Install", // DialogInstall
    "Install Merged", // DialogInstallMerged
    "Install selected folders into the same mod folder and treat them as a single mod", // DialogInstallMergedTooltip
    "Install Separately", // DialogInstallSeparately
    "Install selected folders into their own mod folder", // DialogInstallSeparatelyTooltip
    "Install failed", // DialogInstallFailed
    "Install unavailable", // DialogInstallUnavailable
    "Install failed for {name}: {error}", // DialogInstallFailedFor
    "Install inspection failed for {name}: {error}", // DialogInstallInspectionFailed
    "Install dispatch failed for {name}", // DialogInstallDispatchFailed
    "Failed to start install for {name}: {error}", // DialogInstallStartFailed
    "Select a game first.", // DialogSelectGameFirst
    "No folders selected", // DialogNoFoldersSelected
    "Install canceled", // DialogInstallCanceled
    "Install canceled: {name}", // DialogInstallCanceledMessage
    "Installation Conflict", // DialogInstallationConflict
    "this folder", // DialogThisFolder
    "Already exists in:", // DialogAlreadyExistsIn
    "Replace", // DialogReplace
    "Merge", // DialogMerge
    "Keep Both", // DialogKeepBoth
    "Conflict (Replace)", // DialogConflictReplace
    "Conflict (Merge)", // DialogConflictMerge
    "Conflict (Keep Both)", // DialogConflictKeepBoth
    "Conflict (Cancel)", // DialogConflictCancel
    "Drop mods to install them\n\nor\n\ndrop images to add into:\n{name}", // DialogDropModsImages
    "Drop to install", // DialogDropToInstall
    "Unsupported", // DialogUnsupported
    "Unsupported: {file}", // DialogUnsupportedFile
    "file", // DialogFile
    "Archives", // DialogFileFilterArchives
    "Executable", // DialogFileFilterExecutable
    "Open an unlinked mod detail first", // DialogOpenUnlinkedModDetailFirst
    "Installing: {count} mod(s)", // DialogInstallingCount
    "Could not create mods folder", // DialogCouldNotCreateModsFolder
    "Could not disable installed mod", // DialogCouldNotDisableInstalledMod
    "Could not keep mod disabled", // DialogCouldNotKeepModDisabled
    "Installed", // DialogInstalledAction
    "Installed {count} mods", // DialogInstalledCount
    "Installed: {name}", // DialogInstalledName
    "Synced", // DialogSyncedAction
    "Update unavailable", // DialogUpdateUnavailable
    "Updating: {title}", // DialogUpdatingTask

    // Main GUI: App Messages
    "Could not save settings", // AppCouldNotSaveSettings
    "Could not save data", // AppCouldNotSaveData
    "Warn: {detail}", // AppLogWarn
    "Error: {detail}", // AppLogError
    "Launch failed", // AppLaunchFailed
    "Launch path not set", // AppLaunchPathNotSet
    "Game not selected", // AppGameNotSelected
    "Play (Modded)", // AppPlayModded
    "Play (Vanilla)", // AppPlayVanilla
    "Modded", // AppModded
    "Vanilla", // AppVanilla
    "{label} path not set for {game}", // AppLaunchPathNotSetForGame
    "Launched {game} ({mode})", // AppLaunchedGameMode
    "No feedback survey is configured for this version.", // AppNoFeedbackSurveyConfigured
    "Adding clipboard image...", // AppAddingClipboardImage
    "Could not paste image", // AppCouldNotPasteImage
    "Could not attach images", // AppCouldNotAttachImages
    "Could not save images", // AppCouldNotSaveImages
    "Images Added", // AppImagesAddedAction
    "Added {count} image(s)", // AppImagesAdded
    "Could not add images", // AppCouldNotAddImages
    "Watch Preview", // AppWatchPreview
    "Could not open browser", // AppCouldNotOpenBrowser
    "Could not refresh mods", // AppCouldNotRefreshMods
    "{count} mods scanned, no changes", // AppModsScannedNoChanges
    "Reloaded: {count} mods, no changes", // AppReloadedNoChanges
    "{count} mods scanned", // AppModsScanned
    "Reloaded: {count} mods", // AppReloaded
    "{count} added", // AppReloadAdded
    "{count} removed", // AppReloadRemoved
    "{count} changed", // AppReloadChanged
    "Reload: {line}", // AppReloadAction
    "Category", // AppCategoryAction
    "Created \"{category}\"", // AppCategoryCreated
    "{mod} has no valid GameBanana category; skipped category creation", // AppCategorySkippedNoValidGameBananaCategory
    "Survey", // AppSurveyAction
    "Discarded unreadable pending feedback payload: {error}", // AppSurveyDiscardedUnreadablePendingFeedbackPayload
    "Retrying pending feedback payload", // AppSurveyRetryingPendingFeedbackPayload
    "Submitted feedback for {version}", // AppSurveySubmittedFeedback
    "Feedback submit failed for {version}: {error}", // AppSurveyFeedbackSubmitFailed
    "Discarded pending feedback payload for {version}", // AppSurveyDiscardedPendingFeedbackPayload
    "Could not submit feedback", // AppCouldNotSubmitFeedback
    "Feedback submitted", // AppFeedbackSubmitted
    "Download canceled: {title}", // AppDownloadCanceled

    // Main GUI: Chrome
    "\nMod Manager", // ChromeAppSubtitle
    "Play", // ChromePlay
    "Install\nZip/Rar", // ChromeInstallArchive
    "Install\nFolder", // ChromeInstallFolder
    "Reload", // ChromeReload
    "Game is not installed or configured.", // ChromeGameNotInstalled
    "Launch the game with mods via XXMI", // ChromeLaunchWithModsTooltip
    "Launch the game without mods", // ChromeLaunchWithoutModsTooltip
    "Play with mods", // ChromePlayWithMods
    "Play without mods", // ChromePlayWithoutMods
    "Install a mod from a zip/rar/7z archive", // ChromeInstallArchiveTooltip
    "Install a mod from an already extracted folder", // ChromeInstallFolderTooltip
    "Install", // ChromeInstall
    "Install & Disable", // ChromeInstallDisabled
    "Rescan installed mods and check for updates on GameBanana (Ctrl+R)", // ChromeReloadLibraryTooltip
    "Reload the current list (Ctrl+R)", // ChromeReloadBrowseTooltip
    "Close", // ChromeClose
    "Restore", // ChromeRestore
    "Maximize", // ChromeMaximize
    "Minimize", // ChromeMinimize
    "My Mods", // ChromeMyMods
    "Browse", // ChromeBrowse
    "Tools (Ctrl+T)", // ChromeToolsTooltip
    "Tasks (Ctrl+J)", // ChromeTasksTooltip
    "Log (Ctrl+L)", // ChromeLogTooltip
    "Settings (F10)", // ChromeSettingsTooltip
    "No games detected or enabled", // ChromeNoGamesDetected
    "See Settings → Game & Path", // ChromeSeeSettingsGamePath

    // Main GUI: Browse
    "Discover mods on GameBanana...", // BrowseSearchHint
    "GameBanana Mods", // BrowseModsTitle
    "Characters", // BrowseCharacters
    "Popular", // BrowsePopular
    "Recent Updated", // BrowseRecentUpdated
    "Best Match", // BrowseBestMatch
    "{count} mods", // BrowseModsCount
    "Loading…", // BrowseLoading
    "{count} hidden for NSFW", // BrowseHiddenNsfwCount
    "{count} mods", // BrowseSelectedCharacterModsCount
    "Show all mods", // BrowseShowAllMods
    "Fetching mods from GameBanana…", // BrowseFetchingMods
    "Installed", // BrowseInstalled
    "Open in Browser", // BrowseOpenInBrowser
    "Could not open browser", // BrowseCouldNotOpenBrowser
    "Loading more…", // BrowseLoadingMore
    "No character list is configured for this game.", // BrowseNoCharacterList
    "Refresh characters", // BrowseRefreshCharacters
    "Clear this filter", // BrowseClearFilter
    "Selected: {name}", // BrowseSelectedCharacter
    "{count} characters", // BrowseCharacterCount
    "Waiting", // BrowseWaiting
    "No characters returned by GameBanana.", // BrowseNoCharactersReturned
    "Mod Detail", // BrowseModDetail
    "Copy GameBanana ID", // BrowseCopyGameBananaId
    "GameBanana ID copied", // BrowseGameBananaIdCopied
    "Unknown", // BrowseUnknown
    "Updates", // BrowseUpdates
    "This mod is private.", // BrowsePrivateMod
    "Automatic installation is disabled. You may be able to view or download it directly on GameBanana if you are authorized.", // BrowseAutomaticInstallDisabledAuthorized
    "This mod has been withheld", // BrowseWithheldMod
    "Withheld by", // BrowseWithheldBy
    "Automatic installation is disabled until the withhold is resolved.", // BrowseAutomaticInstallDisabledWithheld
    "Rule violation", // BrowseRuleViolation
    "This mod no longer exists.", // BrowseDeletedModNoLongerExists
    "This mod has been deleted by", // BrowseDeletedBy
    "This mod has been deleted", // BrowseDeleted
    "Files", // BrowseFiles
    "Archived Files", // BrowseArchivedFiles
    "Loading mod details…", // BrowseLoadingDetails
    "{size} • {date} • {downloads} downloads", // BrowseFileMetadata
    "Choose Files", // BrowseChooseFiles
    "This mod has multiple files available.\nSelect file(s) to download and install:", // BrowseMultipleFilesPrompt
    "This game has no configured GameBanana character category list.", // BrowseNoConfiguredCharacterCategoryList
    "Characters unavailable", // BrowseCharactersUnavailable
    "Connection failed", // BrowseConnectionFailed
    "Browse failed", // BrowseFailed
    "Characters failed", // BrowseCharactersFailed
    "Browse detail failed", // BrowseDetailFailed
    "Could not load updates", // BrowseCouldNotLoadUpdates
    "Downloaded: {title}", // BrowseDownloaded
    "Could not prepare install", // BrowseCouldNotPrepareInstall
    "Download failed", // BrowseDownloadFailed
    "Resolving download: {title}", // BrowseResolvingDownload
    "No downloadable files found", // BrowseNoDownloadableFilesFound
    "No files selected", // BrowseNoFilesSelected
    "Download queued", // BrowseDownloadQueued
    "Browse page refresh failed; using cached results: {warning}", // BrowsePageWarning
    "Browse page failed: {error}", // BrowsePageFailed
    "Character category refresh failed; using cached results: {warning}", // BrowseCharacterCategoriesWarning
    "Character categories failed: {error}", // BrowseCharacterCategoriesFailed
    "Browse detail refresh failed for mod {mod_id}; using cached details: {warning}", // BrowseDetailWarning
    "Browse detail failed for mod {mod_id}: {error}", // BrowseDetailFailedMessage
    "Browse updates refresh failed for mod {mod_id}; using cached updates: {warning}", // BrowseUpdatesWarning
    "Browse updates failed for mod {mod_id}: {error}", // BrowseUpdatesFailedMessage
    "Download failed for {title}: {error}", // BrowseDownloadFailedMessage

    // Main GUI: My Mods
    "Scanning installed mods", // LibraryScanningInstalledMods
    "Ensure you have XXMI installed correctly.", // LibraryEnsureXxmiInstalled
    "- Download XXMI: ", // LibraryDownloadXxmi
    "Then go to the settings window to enable a game and fix the game path if needed.\n- Click on the game icon to enable/disable it.\n- Manually select a path by clicking the […] button.", // LibraryBlankInstructions
    "Open Settings", // LibraryOpenSettings
    "Filter mod's name...", // LibrarySearchHint
    "Installed Mods", // LibraryInstalledMods
    "{count} selected", // LibrarySelectedCount
    "Select all visible mods", // LibrarySelectAllVisibleMods
    "{count} mods", // LibraryModsCount
    "1 mod", // LibraryOneMod
    "Back", // LibraryBack
    "Back to category folders", // LibraryBackToCategoryFolders
    "{active} active • {disabled} disabled • {archived} archived", // LibraryCategorySummary
    "Name A-Z", // LibrarySortNameAsc
    "Name Z-A", // LibrarySortNameDesc
    "Newest → Oldest", // LibrarySortDateDesc
    "Oldest → Newest", // LibrarySortDateAsc
    "Sort, group, and layout installed mods", // LibrarySortMenuTooltip
    "Sort Mods", // LibrarySortModsHeading
    "Sorts by mod title, falling back to folder name.", // LibrarySortNameTooltip
    "Uses the newest known install, content, or refresh timestamp.", // LibrarySortNewestTooltip
    "Uses the oldest known install, content, or refresh timestamp first.", // LibrarySortOldestTooltip
    "Group Mods", // LibraryGroupModsHeading
    "Groups mods by your per-game categories.", // LibraryGroupCategoryTooltip
    "Groups mods into Active, Disabled, and Archived sections.", // LibraryGroupStatusTooltip
    "Shows one continuous sorted mod list.", // LibraryGroupNoneTooltip
    "Category Layout", // LibraryCategoryLayoutHeading
    "Available when grouped by category.", // LibraryAvailableWhenGroupedByCategory
    "Shows category tiles first, then opens one category at a time.", // LibraryCategoryFoldersTooltip
    "Shows every category as a section in the mod list.", // LibraryCategoryListTooltip
    "Sort Categories", // LibrarySortCategoriesHeading
    "Manual", // LibraryCategorySortManual
    "By Name (A-Z)", // LibraryCategorySortByNameAsc
    "By Least Mods", // LibraryCategorySortByLeastMods
    "By Most Mods", // LibraryCategorySortByMostMods
    "Uses your manual category order.", // LibraryCategorySortManualTooltip
    "Sorts category folders and sections by category name.", // LibraryCategorySortByNameTooltip
    "Shows categories with the most mods first.", // LibraryCategorySortByMostModsTooltip
    "Shows categories with the fewest mods first.", // LibraryCategorySortByLeastModsTooltip
    "Miscellaneous", // LibraryMiscellaneousHeading
    "Within status groups, follows category order before the selected sort.", // LibrarySortCategoryFirstTooltip
    "Places Active mods first, then Disabled, then Archived before the selected sort.", // LibrarySortStatusFirstTooltip
    "Available when grouped by category in list layout.", // LibraryUncategorizedFirstListOnlyTooltip
    "Toggle Visibility", // LibraryToggleVisibility
    "Mod State", // LibraryModStateHeading
    "Show all mod states", // LibraryShowAllModStates
    "Hide all mod states", // LibraryHideAllModStates
    "Enabled mods", // LibraryEnabledMods
    "Disabled mods", // LibraryDisabledMods
    "Archived mods", // LibraryArchivedMods
    "Update State", // LibraryUpdateStateHeading
    "Show all update states", // LibraryShowAllUpdateStates
    "Hide all update states", // LibraryHideAllUpdateStates
    "Unlinked", // LibraryUnlinked
    "Up to Date", // LibraryUpToDate
    "Update Available", // LibraryUpdateAvailable
    "Check Skipped", // LibraryCheckSkipped
    "Missing Source", // LibraryMissingSource
    "Modified Locally", // LibraryModifiedLocally
    "Ignoring Update", // LibraryIgnoringUpdate
    "Shows mods that are ignoring the current update or ignoring updates until turned off.", // LibraryIgnoringUpdateTooltip
    "Update", // LibraryUpdate
    "Enable", // LibraryEnable
    "Disable", // LibraryDisable
    "Archive", // LibraryArchive
    "More", // LibraryMore
    "(none)", // LibraryNone
    "There is no category yet.\n\n1. Click a mod card to open its detail.\n2. Click \"Uncategorized\" below the mod's name.\n3. Click \"+ New Category\" and name it.", // LibraryNoCategoryHelp
    "There is no category yet.", // LibraryNoCategoryYet
    "New Category", // LibraryNewCategory
    "Open", // LibraryOpen
    "File Explorer", // LibraryFileExplorer
    "No GameBanana source is linked for this mod.", // LibraryNoGameBananaSource
    "Ignore update once", // LibraryIgnoreUpdateOnce
    "Ignores the current update if one is available. If no update is available yet, remembers the current remote version and ignores the next update detected.", // LibraryIgnoreUpdateOnceTooltip
    "Sync this mod with GameBanana before using ignore once.", // LibraryIgnoreUpdateOnceDisabledTooltip
    "Sync at least one selected mod with GameBanana before using ignore once.", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "Ignore update always", // LibraryIgnoreUpdateAlways
    "Indefinitely sets this mod's update status to \"Ignoring Update Always\" until unchecked.", // LibraryIgnoreUpdateAlwaysTooltip
    "Modified", // LibraryModified
    "\n(Modified)", // LibraryModifiedSuffix
    "…and {count} more", // LibraryAndMore
    "Modified & Ignoring Once", // LibraryModifiedIgnoringOnce
    "Modified & Ignoring Always", // LibraryModifiedIgnoringAlways
    "Modified & Update Available", // LibraryModifiedUpdateAvailable
    "Ignoring Once", // LibraryIgnoringOnce
    "Ignoring Always", // LibraryIgnoringAlways
    "Missing", // LibraryMissing
    "Skipped", // LibrarySkipped
    "Empty", // LibraryEmpty
    "Moving", // LibraryMoving
    "Move here", // LibraryMoveHere
    "Open {item}", // LibraryOpenItem
    "Drop on a category", // LibraryDropOnCategory
    "Reorder folder", // LibraryReorderFolder
    "Categories", // LibraryCategoriesHeading
    "{folders} folders / {uncategorized} uncategorized mods", // LibraryFoldersUncategorizedSummary
    "Drop switches to Manual order", // LibraryDropSwitchesToManualOrder
    "Rename", // LibraryRename
    "Rename (F2)", // LibraryRenameShortcut
    "Folder only, move mods outside", // LibraryFolderOnlyMoveModsOutside
    "Folder and mods inside", // LibraryFolderAndModsInside
    "Deleted folder: {category}", // LibraryDeletedFolder
    "Active", // LibraryStatusActive
    "Disabled", // LibraryStatusDisabled
    "Archived", // LibraryStatusArchived
    "Recycled", // LibraryRecycledAction
    "Deleted", // LibraryDeletedAction
    "Delete failed", // LibraryDeleteFailed
    "Disable failed", // LibraryDisableFailed
    "Archive failed", // LibraryArchiveFailed
    "Enable failed", // LibraryEnableFailed
    "Restore failed", // LibraryRestoreFailed
    "Disabled", // LibraryActionDisabled
    "Archived", // LibraryActionArchived
    "Enabled", // LibraryActionEnabled
    "Unarchived", // LibraryActionUnarchived
    "{action}: {name}", // LibraryActionMessage
    "{action} {count} mod(s)", // LibraryActionCountMessage
    "{action} {category} and {count} mod(s)", // LibraryCategoryActionCountMessage
    "Queued updates for {count} mod(s)", // LibraryQueuedUpdates
    "Rename failed", // LibraryRenameFailed
    "Renamed", // LibraryActionRenamed
    "Renamed to: {name}", // LibraryRenamedTo
    "Personal Note", // LibraryPersonalNote
    "Saved personal note", // LibrarySavedPersonalNote
    "Personal note removed", // LibraryPersonalNoteRemoved
    "Could not save personal note", // LibraryCouldNotSavePersonalNote
    "Remove image", // LibraryRemoveImage
    "Click here to", // LibraryClickHereTo
    "manually add images.", // LibraryManuallyAddImages
    "You can drop the images here too,", // LibraryDropImagesHere
    "or paste from clipboard (CTRL + V).", // LibraryPasteFromClipboard
    "Adding images...", // LibraryAddingImages
    "Add images", // LibraryAddImages
    "Images", // LibraryImagesFileDialog
    "Adding {count} image(s)", // LibraryAddingImagesCount
    "Could not add images", // LibraryCouldNotAddImages
    "Image removed", // LibraryImageRemoved
    "Could not remove image", // LibraryCouldNotRemoveImage
    "Description", // LibraryDescription
    "Metadata", // LibraryMetadata
    "Requires RabbitFX", // LibraryRequiresRabbitFx
    "Add a personal note", // LibraryAddPersonalNote
    "Save personal note", // LibrarySavePersonalNote
    "Editable user note", // LibraryEditableUserNote
    "Edit personal note", // LibraryEditPersonalNote
    "+ Add Note", // LibraryAddNote
    "Local", // LibraryLocal
    "Open in File Explorer", // LibraryOpenInFileExplorer
    "Source", // LibrarySource
    "• Last synced: {age}", // LibraryLastSynced
    "Resync", // LibraryResync
    "Unlink", // LibraryUnlink
    "GameBanana Page", // LibraryGameBananaPage
    "Link to GameBanana to enable update tracking and metadata sync.", // LibraryLinkGameBananaPrompt
    "URL or ID", // LibraryUrlOrId
    "Sync Mod", // LibrarySyncMod
    "Update Preferences:", // LibraryUpdatePreferences
    "Syncing with GameBanana…", // LibrarySyncingGameBanana

    // Window: Settings
    "Settings", // SettingsWindowTitle
    "General", // SettingsTabGeneral
    "Category", // SettingsTabCategory
    "Advanced", // SettingsTabAdvanced
    "Game & Path", // SettingsTabGamePath
    "About", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "Behavior", // SettingsGeneralBehaviorSection
    "When launching a game:", // SettingsGeneralBehaviorWhenLaunchingGame
    "After installing a mod:", // SettingsGeneralBehaviorAfterInstallingMod
    "When launching a tool:", // SettingsGeneralBehaviorWhenLaunchingTool
    "Mod detail metadata:", // SettingsGeneralBehaviorModDetailMetadata
    "Do Nothing", // SettingsGeneralBehaviorDoNothing
    "Minimize Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "Exit Hestia", // SettingsGeneralBehaviorExitHestia
    "Add to Selection", // SettingsGeneralBehaviorAddToSelection
    "Open Mod Detail", // SettingsGeneralBehaviorOpenModDetail
    "Never show", // SettingsGeneralBehaviorNeverShow
    "Show if no description", // SettingsGeneralBehaviorShowIfNoDescription
    "Always show", // SettingsGeneralBehaviorAlwaysShow

    // Window: Settings > General > Installed Mods List
    "Installed Mods List", // SettingsGeneralInstalledModsListSection
    "Group list by:", // SettingsGeneralInstalledModsGroupListBy
    "Category Layout:", // SettingsGeneralInstalledModsCategoryLayout
    "Category", // SettingsGeneralInstalledModsGroupCategory
    "Status", // SettingsGeneralInstalledModsGroupStatus
    "None", // SettingsGeneralInstalledModsGroupNone
    "List", // SettingsGeneralInstalledModsLayoutList
    "Folders", // SettingsGeneralInstalledModsLayoutFolders
    "Sort by category first", // SettingsGeneralInstalledModsSortByCategoryFirst
    "Sorts by category order (not necessarily alphabetical).", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
    "Sort by status first", // SettingsGeneralInstalledModsSortByStatusFirst
    "Sorts Active mods first, then Disabled, then Archived.", // SettingsGeneralInstalledModsSortByStatusFirstTooltip
    "Show mod status on card", // SettingsGeneralInstalledModsShowModStatusOnCard
    "Show category on card", // SettingsGeneralInstalledModsShowCategoryOnCard
    "Mod state is still shown by the colored status dot.", // SettingsGeneralInstalledModsShowCategoryOnCardTooltip
    "Show disabled mods", // SettingsGeneralInstalledModsShowDisabledMods
    "Show archived mods", // SettingsGeneralInstalledModsShowArchivedMods
    "Show uncategorized mods first", // SettingsGeneralInstalledModsShowUncategorizedModsFirst

    // Window: Settings > General > Operational
    "Operational", // SettingsGeneralOperationalSection
    "Mods to check for updates:", // SettingsGeneralOperationalModsToCheckForUpdates
    "Automatically update mods:", // SettingsGeneralOperationalAutomaticallyUpdateMods
    "Active", // SettingsGeneralOperationalStatusActive
    "Disabled", // SettingsGeneralOperationalStatusDisabled
    "Archived", // SettingsGeneralOperationalStatusArchived
    "Also update mods that have been modified:", // SettingsGeneralOperationalAlsoUpdateModifiedMods
    "Yes", // SettingsGeneralOperationalYes
    "No, but show Update button", // SettingsGeneralOperationalNoButShowUpdateButton
    "No, and hide Update button", // SettingsGeneralOperationalNoAndHideUpdateButton
    "When installing an already exist mod:", // SettingsGeneralOperationalWhenInstallingExistingMod
    "Always Ask", // SettingsGeneralOperationalAlwaysAsk
    "Always Replace", // SettingsGeneralOperationalAlwaysReplace
    "Always Merge", // SettingsGeneralOperationalAlwaysMerge
    "Always Keep Both", // SettingsGeneralOperationalAlwaysKeepBoth
    "Always replace on updating mods", // SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods
    "Always translate mod details", // SettingsGeneralOperationalAlwaysTranslateModDetails
    "Automatically translates GameBanana details when opening a mod detail window in Browse or MY MODS. Original GameBanana metadata stays unchanged.", // SettingsGeneralOperationalAlwaysTranslateModDetailsTooltip
    "When deleting a mod:", // SettingsGeneralOperationalWhenDeletingMod
    "Move to Recycle Bin", // SettingsGeneralOperationalMoveToRecycleBin
    "Delete Permanently", // SettingsGeneralOperationalDeletePermanently

    // Window: Settings > General > Tasks
    "Tasks", // SettingsGeneralTasksSection
    "Tasks layout:", // SettingsGeneralTasksLayout
    "Sections", // SettingsGeneralTasksLayoutSections
    "Tabbed", // SettingsGeneralTasksLayoutTabbed
    "Single List", // SettingsGeneralTasksLayoutSingleList
    "Clear completed tasks:", // SettingsGeneralTasksClearCompletedTasks
    "Clear Tasks", // SettingsGeneralTasksClearTasks
    "Task order:", // SettingsGeneralTasksOrder
    "Oldest → Newest", // SettingsGeneralTasksOldestToNewest
    "Newest → Oldest", // SettingsGeneralTasksNewestToOldest

    // Window: Settings > Category
    "Select a game to configure categories.", // SettingsCategorySelectGame
    "Browse", // SettingsCategoryBrowseSection
    "Auto-create GameBanana categories for downloaded mods", // SettingsCategoryAutoCreateGameBananaCategories
    "Applies to {game}.", // SettingsCategoryAppliesToGame
    "Categories", // SettingsCategoryCategoriesSection
    "Select all categories", // SettingsCategorySelectAllCategories
    "Unselect all categories", // SettingsCategoryUnselectAllCategories
    "New", // SettingsCategoryNew
    "New category (Ctrl+N)", // SettingsCategoryNewTooltip
    "Delete", // SettingsCategoryDelete
    "Uncategorized", // SettingsCategoryUncategorized

    // Window: Settings > Game & Path
    "Having trouble with paths?", // SettingsPathScanTitle
    "Hestia can perform a deep scan to detect paths for XXMI and supported game", // SettingsPathScanDescription
    "Scan Paths", // SettingsPathScanButtonScan
    "Scanning...", // SettingsPathScanButtonScanning
    "Scan accessible drives for XXMI and game executables.", // SettingsPathScanButtonTooltip
    "XXMI", // SettingsPathXxmiSection
    "XXMI Launcher:", // SettingsPathXxmiLauncher
    "Path not found", // SettingsPathPathNotFound
    "Use default XXMI mod path for games", // SettingsPathUseDefaultXxmiModPath
    "Game", // SettingsPathGameSection
    "Game EXE file:", // SettingsPathGameExeFile
    "{code} Mods Folder:", // SettingsPathGameModsFolder

    // Window: Settings > Advanced > Appearance
    "Appearance", // SettingsAdvancedAppearanceSection
    "Language:", // SettingsAdvancedAppearanceLanguage
    "Font Style:", // SettingsAdvancedAppearanceFontStyle
    "Classic", // SettingsAdvancedAppearanceFontClassic
    "Modern", // SettingsAdvancedAppearanceFontModern
    "Uses 'Segoe UI' typeface", // SettingsAdvancedAppearanceFontClassicTooltip
    "Uses 'Selawik' typeface", // SettingsAdvancedAppearanceFontModernTooltip

    // Window: Settings > Advanced > Content Restriction
    "Content Restriction", // SettingsAdvancedContentRestrictionSection
    "Hide Unsafe Contents:", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "Hide NSFW mods, hide counter", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "Hide NSFW mods, show counter", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "Show with images censored", // SettingsAdvancedContentRestrictionShowImagesCensored
    "Show unrestricted", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Cache and Archive
    "Cache and Archive", // SettingsAdvancedCacheArchiveSection
    "Cache size:", // SettingsAdvancedCacheArchiveCacheSize
    "Current Usage: {gb} GB", // SettingsAdvancedCacheArchiveCurrentUsage
    "Clear Cache", // SettingsAdvancedCacheArchiveClearCache
    "Cache cleared", // SettingsAdvancedCacheArchiveCacheCleared
    "Could not clear cache", // SettingsAdvancedCacheArchiveClearCacheFailed
    "Archive Usage: {gb} GB", // SettingsAdvancedCacheArchiveArchiveUsage
    "Delete Archived Mods", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "Recycled", // SettingsAdvancedCacheArchiveRecycled
    "Deleted", // SettingsAdvancedCacheArchiveDeleted
    "{count} archived mods", // SettingsAdvancedCacheArchiveArchivedMods
    "Archives cleared: {count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "No archives to clear", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "Could not clear archives", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "by {authors}", // SettingsAboutBy
    "Version:", // SettingsAboutVersion
    "Click to show What's New.", // SettingsAboutVersionTooltip
    "Automatically check for update", // SettingsAboutAutomaticallyCheckForUpdate
    "Checking...", // SettingsAboutUpdateChecking
    "Restart to Update", // SettingsAboutUpdateRestartToUpdate
    "Check for Update", // SettingsAboutUpdateCheckForUpdate
    "Up to Date", // SettingsAboutUpdateUpToDate
    "Failed to Check", // SettingsAboutUpdateFailedToCheck
    "Manual Update Required", // SettingsAboutUpdateManualRequired
    "Update Available", // SettingsAboutUpdateAvailable
    "Update ready", // SettingsAboutUpdateReady
    "Update failed", // SettingsAboutUpdateFailed
    "Update download canceled", // SettingsAboutUpdateDownloadCanceled
    "Wait for active tasks before updating", // SettingsAboutUpdateWaitForActiveTasks
    "Could not apply update", // SettingsAboutUpdateCouldNotApply
    "Hestia is installed in a folder this process cannot update:\n{path}\nMove Hestia to another folder and try again, or update this install from an elevated process.", // SettingsAboutUpdateManualInstallFolder
    "Attribution", // SettingsAboutAttributionSection
    "Data source: GameBanana, API used with permission. GameBanana mod metadata, media, and browse data are sourced from GameBanana.", // SettingsAboutAttributionGameBanana

    // Translation strings
    "Translation failed", // TranslationFailed
    "Translation in progress", // TranslationInProgress
];
