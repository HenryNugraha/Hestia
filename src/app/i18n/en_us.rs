const EN_US: [&str; TEXT_KEY_COUNT] = [
    // Window: What's New
    "What's New", // WhatsNewWindowTitle
    "Click to show feedback survey", // WhatsNewFeedbackSurveyTooltip

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
    "Add shortcuts to external tools for this game, then launch them from Hestia. Set custom launch options when needed, and pin frequently used tools to the title bar.", // ToolsDescription
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
    "New", // ToolsNewBadge
    "{mod} includes a tool: {tool}", // ToolsInstalledModIncludesTool
    "{mod} includes {count} tools", // ToolsInstalledModIncludesTools
    "Open Tools", // ToolsOpenToolsAction

    // Window: Tool Launch Options
    "Set Launch Options", // ToolLaunchOptionsWindowTitle
    "Launch options (ie, -option value -flag)", // ToolLaunchOptionsHint
    "Save", // ToolLaunchOptionsSave
    "Cancel", // ToolLaunchOptionsCancel

    // Window: Dialogs
    "Scanning paths...", // DialogScanningPaths
    "Scan Games", // DialogFindingPaths
    "Hestia is scanning through accessible drives for supported games installations.", // DialogDeepScanningPaths
    "Scan Results", // DialogScanResults
    "Continue", // DialogContinue
    "Stop Scan", // DialogStopScan
    "Stopping scan…", // DialogStoppingScan
    "Scan stopped.", // DialogScanStopped
    "Scan completed.", // DialogScanCompleted
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
    "All files", // DialogFileFilterAllFiles
    "Open an unlinked mod detail first", // DialogOpenUnlinkedModDetailFirst
    "Installing: {count} mod(s)", // DialogInstallingCount
    "Could not create mods folder", // DialogCouldNotCreateModsFolder
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
    "Launched {game}", // AppLaunchedGame
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
    "Mods\nFolder", // ChromeOpenModsFolder
    "Reload", // ChromeReload
    "Game is not installed or configured.", // ChromeGameNotInstalled
    "Launch the game with mods via XXMI", // ChromeLaunchWithModsTooltip
    "Launch the game without mods", // ChromeLaunchWithoutModsTooltip
    "Play with mods", // ChromePlayWithMods
    "Play without mods", // ChromePlayWithoutMods
    "Install a mod from a zip/rar/7z archive", // ChromeInstallArchiveTooltip
    "Install a mod from an already extracted folder", // ChromeInstallFolderTooltip
    "Open the selected game's mods folder", // ChromeOpenModsFolderTooltip
    "Install", // ChromeInstall
    "Install & Enable", // ChromeInstallEnabled
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
    "Settings (Ctrl+P)", // ChromeSettingsTooltip
    "No Games Enabled", // ChromeNoGamesDetected
    "See Settings → Games", // ChromeSeeSettingsGames

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
    "Connection timed out", // BrowseConnectionTimedOut
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
    "XXMI Launcher required", // LibraryEnsureXxmiInstalled
    "XXMI Launcher is required before Hestia can install or download mods for this game.", // LibraryInstallXxmiDescription
    "Scan for installed games or enable one manually in settings.", // LibrarySetupDescription
    "Download XXMI", // LibraryDownloadXxmi
    "Scan Games", // LibraryFindGamesAndFixPaths
    "Find supported games automatically.", // LibraryPathScanDescription
    "Games Settings", // LibraryGamesSettings
    "Enable games and set paths manually.", // LibraryGamesSettingsDescription
    "Signature bypasser is missing", // LibraryNteBypasserMissingTitle
    "NTE mods will not load until AyakaNTEModLoader.asi or UniversalSigBypasser.asi is installed.", // LibraryNteBypasserMissingDescription
    "AyakaNTEBypasser", // LibraryNteBypasserAyaka
    "UniversalSigBypasser", // LibraryNteBypasserUniversal
    "Hestia can't make changes in this game's folder", // LibraryProtectedPathTitle
    "The game is installed in a protected location ({path}), so Windows blocks Hestia from installing, disabling, or switching mods here. Click Grant access to let your Windows account modify the game folder, this needs a one-time administrator approval. Restarting as admin also works, but must be repeated every launch and drag-and-drop from Explorer won't work while elevated.", // LibraryProtectedPathDescription
    "Grant access", // LibraryGrantAccess
    "Restart as admin", // LibraryRestartAsAdmin
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
    "Smallest → Largest Size", // LibrarySortSizeAsc
    "Largest → Smallest Size", // LibrarySortSizeDesc
    "Change library view and sort installed mods", // LibrarySortMenuTooltip
    "Mod Order", // LibrarySortModsHeading
    "Sorts by mod title, falling back to folder name", // LibrarySortNameTooltip
    "Uses the newest known install, content, or refresh timestamp", // LibrarySortNewestTooltip
    "Uses the oldest known install, content, or refresh timestamp first", // LibrarySortOldestTooltip
    "Sorts by total mod content size", // LibrarySortSizeTooltip
    "Group Mods", // LibraryGroupModsHeading
    "Groups mods by your per-game categories", // LibraryGroupCategoryTooltip
    "Groups mods into Active, Disabled, and Archived sections", // LibraryGroupStatusTooltip
    "Shows one continuous sorted mod list", // LibraryGroupNoneTooltip
    "Library View", // LibraryCategoryLayoutHeading
    "Available when grouped by category.", // LibraryAvailableWhenGroupedByCategory
    "Shows category tiles first, then opens one category at a time", // LibraryCategoryFoldersTooltip
    "Shows every category as a section in the mod list", // LibraryCategoryListTooltip
    "Category Order", // LibrarySortCategoriesHeading
    "Manual", // LibraryCategorySortManual
    "Name A-Z", // LibraryCategorySortByNameAsc
    "By Least Mods", // LibraryCategorySortByLeastMods
    "By Most Mods", // LibraryCategorySortByMostMods
    "Uses your manual category order", // LibraryCategorySortManualTooltip
    "Sorts categories by name, including category-first ordering within status groups", // LibraryCategorySortByNameTooltip
    "Shows categories with the most mods first", // LibraryCategorySortByMostModsTooltip
    "Shows categories with the fewest mods first", // LibraryCategorySortByLeastModsTooltip
    "Miscellaneous", // LibraryMiscellaneousHeading
    "Within status groups, follows category order before the selected sort", // LibrarySortCategoryFirstTooltip
    "Places Active mods first, then Disabled, then Archived before the selected sort", // LibrarySortStatusFirstTooltip
    "Places Uncategorized first in the category list and the category manager", // LibraryUncategorizedFirstListOnlyTooltip
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
    "Shows mods that are ignoring the current update or ignoring updates until turned off", // LibraryIgnoringUpdateTooltip
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
    "Sync this mod with GameBanana before using ignore once", // LibraryIgnoreUpdateOnceDisabledTooltip
    "Sync at least one selected mod with GameBanana before using ignore once", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "Ignore update always", // LibraryIgnoreUpdateAlways
    "Indefinitely sets this mod's update status to \"Ignoring Update Always\" until unchecked", // LibraryIgnoreUpdateAlwaysTooltip
    "Mark as not modified", // LibraryMarkAsNotModified
    "Treats this mod's current files as unmodified. The original install baseline is kept, and editing files after this point will show as modified again. Updates will install over these changes.", // LibraryMarkAsNotModifiedTooltip
    "Restore modification status", // LibraryRestoreModificationStatus
    "Brings back the Modified status by comparing this mod's files against the original install baseline again", // LibraryRestoreModificationStatusTooltip
    "Mark as NSFW", // LibraryMarkAsNsfw
    "Auto", // LibraryNsfwAuto
    "Yes", // LibraryNsfwYes
    "No", // LibraryNsfwNo
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
    "Mods inside only, keep folder", // LibraryFolderModsInsideKeepFolder
    "Deletes only shown mods. {count} not-shown mod(s) stay in this folder.", // LibraryFolderModsInsideKeepFolderHiddenTooltip
    "Folder and mods inside", // LibraryFolderAndModsInside
    "{count} mod(s) in this folder are not shown. Clear search/filters to delete everything inside.", // LibraryFolderAndModsInsideHiddenTooltip
    "Deleted folder: {category}", // LibraryDeletedFolder
    "Create Category", // LibraryContextCreateCategory
    "Select All", // LibraryContextSelectAll
    "Clear Selection", // LibraryContextClearSelection
    "Sort by", // LibraryContextSortBy
    "Group by", // LibraryContextGroupBy
    "Open Mods Folder", // LibraryContextOpenModsFolder
    "From Archive .zip/.rar", // LibraryContextInstallArchive
    "From Folder", // LibraryContextInstallFolder
    "Created category: {category}", // LibraryCreatedFolder
    "Active", // LibraryStatusActive
    "Disabled", // LibraryStatusDisabled
    "Archived", // LibraryStatusArchived
    "Now", // RelativeTimeNow
    "Today", // RelativeTimeToday
    "{count}m", // RelativeTimeMinutes
    "{count}h", // RelativeTimeHours
    "{count}d", // RelativeTimeDays
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
    "Mods are currently locked, probably by the game.", // LibraryModsLockedProbablyByGame
    "Skipped mods that are currently locked, probably by the game.", // LibrarySkippedLockedModsProbablyByGame
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
    "Requires RabbitFX", // LibraryRequiresRabbitFx
    "Description", // LibraryMetaSourceDescription
    "Description from GameBanana page", // LibraryMetaSourceDescriptionGbTooltip
    "Mod description", // LibraryMetaSourceDescriptionTooltip
    "Hotkeys", // LibraryMetaSourceHotkeys
    "Sourced from the mod's .ini files", // LibraryMetaSourceHotkeysTooltip
    "This mod does not contain any hotkey", // LibraryMetaSourceHotkeysUnavailable
    "List", // LibraryHotkeysViewList
    "Raw", // LibraryHotkeysViewRaw
    "Switch to raw view", // LibraryHotkeysSwitchToRaw
    "Switch to list view", // LibraryHotkeysSwitchToList
    "No toggle keys in this mod.", // LibraryHotkeysNoToggleKeys
    "Read-only", // LibraryHotkeysWriteBlockedLabel
    "Read-only while the game is running.\nTo edit these live, turn on \"Let Hestia modify d3dx.ini configuration\" in\nSettings > General > Operational > Experimental, then restart the game.", // LibraryHotkeysWriteBlockedHint
    "Game is currently running!", // LibraryHotkeysRunningToast
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
    "View on GameBanana", // LibraryGameBananaPage
    "Link to GameBanana to enable update tracking and metadata sync.", // LibraryLinkGameBananaPrompt
    "GameBanana URL or ID", // LibraryUrlOrId
    "Sync Mod", // LibrarySyncMod
    "Update Preferences:", // LibraryUpdatePreferences
    "Syncing with GameBanana…", // LibrarySyncingGameBanana

    // Window: Profiles
    "Profiles", // ProfilesTitle
    "Profile", // ProfilesLabel
    "Default", // ProfilesDefault
    "New profile", // ProfilesNew
    "New profile", // ProfilesCreateEmpty
    "Create a separate set of mods and categories.", // ProfilesNewDescription
    "Duplicate profile", // ProfilesDuplicateCurrent
    "Create a copy of the current profile's mods and categories.", // ProfilesDuplicateDescription
    "Open profile folder", // ProfilesOpenFolder
    "Open the folder where this game's profiles are stored", // ProfilesOpenFolderTooltip
    "Rename profile", // ProfilesRename
    "Choose a new name for this profile.", // ProfilesRenameDescription
    "Delete profile", // ProfilesDelete
    "Profile name", // ProfilesName
    "Enter a profile name.", // ProfilesNameEmpty
    "A profile named \"{name}\" already exists.", // ProfilesNameTaken
    "Switch profile", // ProfilesSwitch
    "Switching profile…", // ProfilesSwitching
    "Creating profile…", // ProfilesCreating
    "Duplicating profile…", // ProfilesDuplicating
    "Deleting profile…", // ProfilesDeleting
    "Archiving current profile…", // ProfilesArchivingCurrent
    "Extracting selected profile…", // ProfilesExtractingSelected
    "Copying selected profile…", // ProfilesCopyingSelected
    "Activating selected profile…", // ProfilesActivatingSelected
    "Inactive profiles are compressed to save disk space.", // ProfilesInactiveCompressedNote
    "Status", // ProfilesStatusLabel
    "Active profile", // ProfilesStatusActive
    "Waiting to compress", // ProfilesStatusQueued
    "Compressing", // ProfilesStatusRunning
    "Compressed", // ProfilesStatusComplete
    "Compression failed", // ProfilesStatusFailed
    "Not compressed", // ProfilesStatusUnavailable
    "Archive size", // ProfilesArchiveSize
    "Previous archive size", // ProfilesPreviousArchiveSize
    "No archive yet", // ProfilesNoArchiveYet
    "Size", // ProfilesUncompressedSize
    "{size} (uncompressed)", // ProfilesUncompressedSizeValue
    "Profile operation failed", // ProfilesOperationFailed
    "Files in {folder} are open in {apps}, close them and try again.", // ProfilesFilesInUse
    "Files in {folder} are open in another program, close it and try again.", // ProfilesFilesInUseUnknown
    " and {count} more", // ProfilesFilesInUseMore
    "Actions paused:", // ProfilesActionsPausedLabel
    "another task is running", // ProfilesActionsPausedFallback
    "profile operation running", // ProfilesActionsPausedProfileOperation
    "refreshing library", // ProfilesActionsPausedRefreshingLibrary
    "installing mods", // ProfilesActionsPausedInstallingMods
    "checking updates", // ProfilesActionsPausedCheckingUpdates
    "updating Hestia", // ProfilesActionsPausedUpdatingHestia
    "game is running", // ProfilesActionsPausedGameRunning
    "mods are locked", // ProfilesActionsPausedModsLocked
    "profile tool is running", // ProfilesActionsPausedProfileToolRunning
    "Select a game to manage profiles.", // ProfilesSelectGame
    "At least one profile is required.", // ProfilesAtLeastOneRequired
    "Switch to another profile before deleting this one.", // ProfilesSwitchBeforeDelete
    "Delete profile \"{name}\"?", // ProfilesDeleteConfirmation
    "This profile's mods will be PERMANENTLY deleted. There is no way to undo this.", // ProfilesDeleteConfirmationDetails
    "Profile created: {name}", // ProfilesCreated
    "Profile duplicated: {name}", // ProfilesDuplicated
    "Profile renamed: {name}", // ProfilesRenamed
    "Profile deleted: {name}", // ProfilesDeleted
    "Profile activated: {name}", // ProfilesActivated
    "Profile operation canceled", // ProfilesCanceled
    "Profile recovered", // ProfilesActionRecovered
    "Recovered {count} profile(s) found in storage but missing from the profile list.", // ProfilesRecovered

    // Window: Settings
    "Settings", // SettingsWindowTitle
    "General", // SettingsTabGeneral
    "Category", // SettingsTabCategory
    "Advanced", // SettingsTabAdvanced
    "Games", // SettingsTabGames
    "About", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "Behavior", // SettingsGeneralBehaviorSection
    "When launching a game:", // SettingsGeneralBehaviorWhenLaunchingGame
    "After installing a mod:", // SettingsGeneralBehaviorAfterInstallingMod
    "New mod install state:", // SettingsGeneralBehaviorNewModInstallState
    "When launching a tool:", // SettingsGeneralBehaviorWhenLaunchingTool
    "Do Nothing", // SettingsGeneralBehaviorDoNothing
    "Minimize Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "Exit Hestia", // SettingsGeneralBehaviorExitHestia
    "Add to Selection", // SettingsGeneralBehaviorAddToSelection
    "Open Mod Detail", // SettingsGeneralBehaviorOpenModDetail
    "Enabled", // SettingsGeneralBehaviorModInstallStateEnabled
    "Disabled", // SettingsGeneralBehaviorModInstallStateDisabled
    "Auto", // SettingsGeneralBehaviorModInstallStateAuto

    // Window: Settings > General > Installed Mods List
    "Installed Mods List", // SettingsGeneralInstalledModsListSection
    "Group list by:", // SettingsGeneralInstalledModsGroupListBy
    "Library View:", // SettingsGeneralInstalledModsCategoryLayout
    "Category", // SettingsGeneralInstalledModsGroupCategory
    "Status", // SettingsGeneralInstalledModsGroupStatus
    "None", // SettingsGeneralInstalledModsGroupNone
    "List: group by category", // SettingsGeneralInstalledModsLayoutList
    "Folders", // SettingsGeneralInstalledModsLayoutFolders
    "Sort by category first", // SettingsGeneralInstalledModsSortByCategoryFirst
    "Sorts by category order (not necessarily alphabetical)", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
    "Sort by status first", // SettingsGeneralInstalledModsSortByStatusFirst
    "Sorts Active mods first, then Disabled, then Archived", // SettingsGeneralInstalledModsSortByStatusFirstTooltip
    "Show mod status on card", // SettingsGeneralInstalledModsShowModStatusOnCard
    "Show category on card", // SettingsGeneralInstalledModsShowCategoryOnCard
    "Mod state is still shown by the colored status dot", // SettingsGeneralInstalledModsShowCategoryOnCardTooltip
    "Show disabled mods", // SettingsGeneralInstalledModsShowDisabledMods
    "Show archived mods", // SettingsGeneralInstalledModsShowArchivedMods
    "Show uncategorized mods first", // SettingsGeneralInstalledModsShowUncategorizedModsFirst
    "Show empty category folders", // SettingsGeneralInstalledModsShowEmptyCategoryFolders

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
    "When deleting a mod:", // SettingsGeneralOperationalWhenDeletingMod
    "Move to Recycle Bin", // SettingsGeneralOperationalMoveToRecycleBin
    "Delete Permanently", // SettingsGeneralOperationalDeletePermanently
    "XXMI Features", // SettingsGeneralXxmiFeaturesSection
    "Preserve in-game mod settings", // SettingsGeneralOperationalPreserveModSettings
    "Keeps saved in-game mod customization (XXMI persistent settings) when renaming, disabling, archiving, updating, or deleting mods, and across profile switches", // SettingsGeneralOperationalPreserveModSettingsTooltip
    "Let Hestia modify d3dx.ini configuration", // SettingsGeneralOperationalSendReloadHotkey
    "After Hestia changes live XXMI mods while the game is running, send the XXMI reload hotkey so the game picks them up immediately. Also lets the Hotkeys list show each mod's live in-game value and update it in exact steps, including changes you make with the mod's own keys", // SettingsGeneralOperationalSendReloadHotkeyTooltip
    "Hestia needs to modify {code} to reliably read mod customization data", // SettingsGeneralOperationalD3dxBulletAutosave
    "Hestia needs to modify {code} to trigger XXMI reload and mod hotkeys. Mod hotkeys pressed on Hestia will take effect in game", // SettingsGeneralOperationalD3dxBulletForeground
    "Changing this will require a manual game restart", // SettingsGeneralOperationalD3dxBulletRestart
    "Field", // SettingsGeneralOperationalD3dxStatusField
    "Expected", // SettingsGeneralOperationalD3dxStatusExpected
    "Current", // SettingsGeneralOperationalD3dxStatusCurrent
    "missing", // SettingsGeneralOperationalD3dxStatusMissing
    "Apply settings", // SettingsGeneralXxmiFeaturesApplySettings
    "Let Hestia manage", // SettingsGeneralXxmiFeaturesLetHestiaManage
    "Restore original settings", // SettingsGeneralXxmiFeaturesRestoreOriginal
    "Repair settings", // SettingsGeneralXxmiFeaturesRepairSettings
    "Unavailable", // SettingsGeneralXxmiFeaturesUnavailableAction
    "Not configured", // SettingsGeneralXxmiFeaturesNotConfigured
    "Partially configured", // SettingsGeneralXxmiFeaturesPartiallyConfigured
    "Configured manually", // SettingsGeneralXxmiFeaturesConfiguredManually
    "Managed by Hestia", // SettingsGeneralXxmiFeaturesManagedByHestia
    "Needs repair", // SettingsGeneralXxmiFeaturesNeedsRepair
    "Hestia cannot read or modify d3dx.ini", // SettingsGeneralXxmiFeaturesUnavailable
    "Limited capability while game is running", // SettingsGeneralOperationalPreserveLimited
    "Fully functional during gameplay", // SettingsGeneralOperationalPreserveFull
    "Auto-reload XXMI when:", // SettingsGeneralOperationalReloadHotkeyTrigger
    "Enabling mods", // SettingsGeneralOperationalReloadTriggerEnablingMods
    "Disabling mods", // SettingsGeneralOperationalReloadTriggerDisablingMods
    "Installing mods", // SettingsGeneralOperationalReloadTriggerInstallingMods
    "Deleting mods", // SettingsGeneralOperationalReloadTriggerDeletingMods
    "Updating mods", // SettingsGeneralOperationalReloadTriggerUpdatingMods
    "Renaming mods", // SettingsGeneralOperationalReloadTriggerRenamingMods
    "Archiving mods", // SettingsGeneralOperationalReloadTriggerArchivingMods
    "Restoring mods", // SettingsGeneralOperationalReloadTriggerRestoringMods
    "Customizing mods", // SettingsGeneralOperationalReloadTriggerCustomizingMods
    "Profile switch", // SettingsGeneralOperationalReloadTriggerProfileSwitch
    "Modify d3dx.ini for {game}?", // SettingsGeneralOperationalD3dxConflictTitle
    "To reload mods and read their customization while the game runs, Hestia adds two lines under [System]:\n\t➔ additional_foreground_window = Hestia\n\t➔ settings_auto_save_interval = 1", // SettingsGeneralOperationalD3dxConflictIntro
    "Your d3dx.ini already sets:", // SettingsGeneralOperationalD3dxConflictExisting
    "Replace it? Hestia comments out any conflicting value (kept but inactive) and keeps its own settings in a marked block below [System].", // SettingsGeneralOperationalD3dxConflictReplaceDetails
    "Nothing else in d3dx.ini is changed. A backup is saved before every edit.", // SettingsGeneralOperationalD3dxConflictNoOtherConfig
    "Game is running. After Replace,\nrestart it manually to take effect.", // SettingsGeneralOperationalD3dxConflictRunningRestart

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

    // Window: Settings > Games
    "Scan Games", // SettingsGamesScanTitle
    "Hestia can perform a deep search to find supported games", // SettingsGamesScanDescription
    "Scan", // SettingsGamesScanButtonScan
    "Scanning...", // SettingsGamesScanButtonScanning
    "XXMI", // SettingsGamesXxmiSection
    "XXMI Launcher:", // SettingsGamesXxmiLauncher
    "Path not found", // SettingsGamesPathNotFound
    "Protected path", // SettingsGamesProtectedPath
    "Use default XXMI mod path for games", // SettingsGamesUseDefaultXxmiModPath
    "Games", // SettingsGamesGamesSection
    "Game EXE file:", // SettingsGamesGameExeFile
    "{code} Mods Folder:", // SettingsGamesGameModsFolder
    "Mod folder (~mods):", // SettingsGamesUnrealModFolder

    // Window: Settings > Advanced > Appearance
    "Appearance", // SettingsAdvancedAppearanceSection
    "Language:", // SettingsAdvancedAppearanceLanguage
    "Font Style:", // SettingsAdvancedAppearanceFontStyle
    "Classic", // SettingsAdvancedAppearanceFontClassic
    "Modern", // SettingsAdvancedAppearanceFontModern
    "Elegant", // SettingsAdvancedAppearanceFontElegant
    "Traditional", // SettingsAdvancedAppearanceFontTraditional
    "Uses the system UI typeface", // SettingsAdvancedAppearanceFontClassicTooltip
    "Uses 'Selawik' typeface", // SettingsAdvancedAppearanceFontModernTooltip
    "Uses Diphylleia with Gabriela for bold text", // SettingsAdvancedAppearanceFontElegantTooltip
    "Uses New Tegomin with Coustard for bold text", // SettingsAdvancedAppearanceFontTraditionalTooltip
    "Always translate mod details", // SettingsAdvancedAppearanceAlwaysTranslateModDetails
    "When enabled, mod descriptions and metadata are automatically translated to the selected language when viewing details", // SettingsAdvancedAppearanceAlwaysTranslateModDetailsTooltip

    // Window: Settings > Advanced > Content Restriction
    "Content Restriction", // SettingsAdvancedContentRestrictionSection
    "Hide Unsafe Contents:", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "Hide NSFW mods, hide counter", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "Hide NSFW mods, show counter", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "Show with images censored", // SettingsAdvancedContentRestrictionShowImagesCensored
    "Show unrestricted", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Proxy
    "Proxy", // SettingsAdvancedProxySection
    "Proxy address:", // SettingsAdvancedProxyAddress
    "Protocol optional; bare addresses are detected automatically. Use socks5h:// or socks4a:// for proxy DNS.", // SettingsAdvancedProxyHelp
    "Authenticated proxies are not supported.", // SettingsAdvancedProxyCredentialsUnsupported
    "Enter a valid proxy address.", // SettingsAdvancedProxyAddressInvalid
    "Proxy disabled", // SettingsAdvancedProxyDisabled
    "Proxy enabled", // SettingsAdvancedProxyEnabled
    "Could not connect to proxy", // SettingsAdvancedProxyConnectionFailed
    "On app launch, Hestia verifies the proxy connection \nbefore starting operations. If it fails, \nHestia continues without the proxy.", // SettingsAdvancedProxyStartupBehavior

    // Window: Settings > Advanced > Cache and Archive
    "Data Cleanup", // SettingsAdvancedCacheArchiveSection
    "Cache size:", // SettingsAdvancedCacheArchiveCacheSize
    "Current Usage: {gb} GB", // SettingsAdvancedCacheArchiveCurrentUsage
    "Clear Cache", // SettingsAdvancedCacheArchiveClearCache
    "Cache cleared", // SettingsAdvancedCacheArchiveCacheCleared
    "Could not clear cache", // SettingsAdvancedCacheArchiveClearCacheFailed
    "Clear Log", // SettingsAdvancedCacheArchiveClearLog
    "Log Entries: {count}", // SettingsAdvancedCacheArchiveLogEntries
    "Log cleared: {count}", // SettingsAdvancedCacheArchiveLogCleared
    "Could not clear log", // SettingsAdvancedCacheArchiveClearLogFailed
    "Archive Usage: {gb} GB", // SettingsAdvancedCacheArchiveArchiveUsage
    "Clear Archive", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "Recycled", // SettingsAdvancedCacheArchiveRecycled
    "Deleted", // SettingsAdvancedCacheArchiveDeleted
    "{count} archived mods", // SettingsAdvancedCacheArchiveArchivedMods
    "Archives cleared: {count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "No archives to clear", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "Could not clear archives", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "by {authors}", // SettingsAboutBy
    "Version:", // SettingsAboutVersion
    "Click to show What's New", // SettingsAboutVersionTooltip
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
    "Translate (F7)", // TranslationToggleShortcut
    "Retranslate", // TranslationRetranslate
    "Translation failed", // TranslationFailed
    "Translation in progress", // TranslationInProgress

    // Settings: Advanced renderer
    "Renderer", // SettingsAdvancedRendererSection
    "Graphics API:", // SettingsAdvancedRendererGraphicsApi
    "Auto (recommended)", // SettingsAdvancedRendererAuto
    "Active renderer: {backend}", // SettingsAdvancedRendererActive
    "Changes take effect after Hestia restarts.", // SettingsAdvancedRendererRestartHint
    "Restart", // SettingsAdvancedRendererRestart

    // Fullview image overlay
    "Copy image", // OverlayCopyImage
    "Previous image", // OverlayPreviousImage
    "Next image", // OverlayNextImage
    "Image copied to clipboard", // OverlayImageCopied
    "Could not copy image", // OverlayCouldNotCopyImage
    "Clear mod's customization", // LibraryHotkeyClearCustomization
    "Link Mod", // LibraryLinkMod
    "Checking…", // LibraryChecking
    "Reset size and position", // ResetWindowLayout
    "Filter characters", // BrowseFilterCharactersHint
    "No GameBanana source is linked for this mod. Click to link it.", // LibraryUnlinkedClickToLink
    "Name Z-A", // LibraryCategorySortByNameDesc
    "Uncategorized: {status}", // LibraryUncategorizedStatusHeader
    "Layout & Order", // LibrarySortMenuTitle

    // In-game overlay
    "In-game Overlay", // SettingsGameOverlay
    "Press {key} while a supported game runs to browse and switch mods over it.", // SettingsGameOverlayTooltip
    "Another app already uses {key}, so only the pin can open the overlay.", // GameOverlayHotkeyTaken
    "Couldn't register {key}, so only the pin can open the overlay.", // GameOverlayHotkeyFailed
    "Couldn't take focus from the game, so keys won't reach the overlay.", // GameOverlayFocusTakeFailed
    "Couldn't give focus back to the game. Click the game to continue.", // GameOverlayFocusReturnFailed
    "Hestia didn't answer, so the change may not have been made.", // GameOverlayNoAnswer
    "Press {key} in the game to see the change.", // GameOverlayPressReloadKey
    "Installed {name}, turned off.", // GameOverlayInstalledNotice
    "Couldn't install {name}.", // GameOverlayInstallFailedNotice
    "{name} needs an answer. Press {key}.", // GameOverlayNeedsAnswerNotice
    "{name} is now with your mods.", // GameOverlayArrivedNotice
    "to browse", // GameOverlayToBrowse
    "Hide", // GameOverlayHide
    "Close overlay", // GameOverlayClose
    "Keep expanded", // GameOverlayKeepExpanded
    "Unpin · return to {key}", // GameOverlayUnpin
    "Restore main Hestia window", // GameOverlayOpenHestia
    "Could not open Hestia: {error}", // GameOverlayCouldNotOpenHestia
    "Show all characters", // GameOverlayShowAllCharacters
    "Type to search", // GameOverlaySearchHint
    "Opacity", // GameOverlayOpacity
    "Navigate", // GameOverlayHintNavigate
    "Browse mods", // GameOverlayHintMods
    "Browse categories", // GameOverlayHintCategories
    "Enable this mod, disable others", // GameOverlayHintExclusive
    "Enable this mod", // GameOverlayHintEnable
    "Install", // GameOverlayHintInstall
    "Try again", // GameOverlayHintTryAgain
    "Toggle Enable/Disable", // GameOverlayHintToggle
    "Search", // GameOverlayHintSearch
    "Done typing", // GameOverlayHintDoneTyping
    "Edit search", // GameOverlayHintEditSearch
    "Clear search", // GameOverlayHintClearSearch
    "Pick the file to install", // GameOverlayPickFile
    "A mod named \"{folder}\" is already installed", // GameOverlaySameName
    "Replace", // GameOverlayReplace
    "Delete the installed one first", // GameOverlayReplaceDetail
    "Merge", // GameOverlayMerge
    "Add the new files to the installed one", // GameOverlayMergeDetail
    "Keep both", // GameOverlayKeepBoth
    "Install it under another name", // GameOverlayKeepBothDetail
    "Cancel", // GameOverlayCancel
    "No installed mods", // GameOverlayNoInstalledMods
    "Install a mod to see it here.", // GameOverlayInstallToSee
    "No matches", // GameOverlayNoMatches
    "Previous category", // GameOverlayPreviousCategory
    "Next category", // GameOverlayNextCategory
    "Couldn't reach GameBanana. Space to try again.", // GameOverlayGameBananaFailed
    "Nothing for {name} on GameBanana", // GameOverlayGameBananaNothingFor
    "No matches on GameBanana", // GameOverlayGameBananaNoMatches
    "NEW", // GameOverlayNewTag
    "Waiting", // GameOverlayWaiting
    "Downloading {percent}%", // GameOverlayDownloadingPercent
    "Downloading", // GameOverlayDownloading
    "Installing", // GameOverlayInstalling
    "Needs your answer", // GameOverlayNeedsYourAnswer
    "Installed, turned off", // GameOverlayInstalledOff
    "Couldn't install. Space to try again.", // GameOverlayCouldNotInstallTryAgain
    "Couldn't install", // GameOverlayCouldNotInstall
    "No preview", // GameOverlayNoPreview
    "Hotkey:", // SettingsGameOverlayHotkey
    "Click, then press the new keys.", // SettingsGameOverlayKeyButtonTooltip
    "Reset to {key}", // SettingsGameOverlayResetKey
    "Press the keys, or Esc to cancel", // SettingsGameOverlayPressKeys
    "Use Alt or Ctrl with a letter, number or F key.", // SettingsGameOverlayKeyNotAllowed
    "{key} is used by Windows, the overlay or XXMI. Pick another.", // SettingsGameOverlayKeyReserved
    "Another app already uses {key}. Pick another.", // SettingsGameOverlayKeyTaken
    "Briefly show when a game starts", // SettingsGameOverlayArrivalStrip
    "Shows the small \"{key} to browse\" bar for a few seconds when the game starts.", // SettingsGameOverlayArrivalStripTooltip
    "GameBanana mods", // SettingsGameOverlayGameBanana
    "Hotkeys ticker", // SettingsGameOverlayKeyHints
    "Interface Size:", // SettingsInterfaceSize
    "Small", // SettingsInterfaceSizeSmall
    "Normal", // SettingsInterfaceSizeNormal
    "Large", // SettingsInterfaceSizeLarge
    "Preview", // SettingsGameOverlayPreview
    "Shows the overlay over Hestia without a game, to try it out. Mods you turn on or off in it change for real.", // SettingsGameOverlayPreviewTooltip
    "Needs a game that uses XXMI.", // SettingsGameOverlayPreviewUnavailable
    "Stop preview", // SettingsGameOverlayStopPreview
    "Press {key} to open the overlay.", // SettingsGameOverlayPreviewHint
    "Custom", // SettingsInterfaceSizeCustom
    "Also sizes the in-game overlay. Ctrl + = and Ctrl + - change it too, Ctrl + 0 goes back to Normal.", // SettingsInterfaceSizeTooltip
    "Overlay components:", // SettingsGameOverlayComponents
    "Lists GameBanana mods after your own, so you can install them without leaving the game. They install turned off.", // SettingsGameOverlayGameBananaTooltip
    "Shows what the keys do at the top of the overlay.", // SettingsGameOverlayKeyHintsTooltip
    "Transparency slider", // SettingsGameOverlayOpacitySlider
    "Shows the slider at the top of the overlay that makes it see-through.", // SettingsGameOverlayOpacitySliderTooltip
    "Pin button", // SettingsGameOverlayPinButton
    "Shows the pin button, which keeps the overlay open while you play.", // SettingsGameOverlayPinButtonTooltip
    "Mod hotkeys", // GameOverlayHintHotkeys
    "Show hotkeys", // GameOverlayShowHotkeys
    "Hide hotkeys", // GameOverlayHideHotkeys
    "Installed {name}.", // GameOverlayInstalledOnNotice
    "List: group by status", // SettingsGeneralInstalledModsLayoutStatusList

    // Folder batch selection and actions
    "Matching current filters", // FolderContentsVisible
    "All mods in selected folders", // FolderContentsAll
    "{folders} folders selected · {mods} mods", // FolderSelectionSummaryWithMods
    "{folders} folders selected", // FolderSelectionSummaryFoldersOnly
    "Enable disabled mods", // FolderBatchActionEnable
    "Disable enabled mods", // FolderBatchActionDisable
    "Archive contents", // FolderBatchActionArchive
    "Restore and enable", // FolderBatchActionRestore
    "Check for updates", // FolderBatchActionCheckUpdates
    "Update available mods", // FolderBatchActionUpdate
    "Move contents to category…", // FolderBatchActionMoveContents
    "Remove folders, keep mods", // FolderBatchActionRemoveFolders
    "Delete contents, keep folders", // FolderBatchActionDeleteContents
    "Delete folders and contents", // FolderBatchActionDeleteFoldersAndContents
    "Enables every disabled mod in scope; never disables active peers.", // FolderBatchTooltipEnable
    "Disables every enabled mod in scope.", // FolderBatchTooltipDisable
    "Archives every mod in scope.", // FolderBatchTooltipArchive
    "Restores archived mods to Active and enables them.", // FolderBatchTooltipRestore
    "Checks selected mods using your configured auto-update settings.", // FolderBatchTooltipCheckUpdates
    "Updates available mods in scope.", // FolderBatchTooltipUpdate
    "Moves selected contents to a category.", // FolderBatchTooltipMoveContents
    "Unassigns all selected folders' mods to Uncategorized; files stay.", // FolderBatchTooltipRemoveFolders
    "Deletes contents using your configured deletion behavior; folders stay.", // FolderBatchTooltipDeleteContents
    "Deletes folders and contents using your configured deletion behavior.", // FolderBatchTooltipDeleteFoldersAndContents
    "Wait for the current operation to finish.", // FolderBatchBusyTooltip
    "Choose all mods in selected folders before deleting folders and their contents.", // FolderDeleteRequiresAllContents
    "Processing {done}/{total}", // FolderBatchProgress
    "Working…", // FolderBatchWorking
    "{completed} completed · {skipped} skipped · {failed} failed", // FolderBatchResultSummary
    "{completed} completed · {skipped} skipped · {failed} failed · Cancelled", // FolderBatchResultSummaryCancelled
    "Batch operation finished.", // FolderBatchResultSummaryHidden
    "Batch operation cancelled.", // FolderBatchResultSummaryHiddenCancelled
    "Select all folders", // FolderSelectAllTooltip
    "Clear folder selection", // FolderClearSelectionTooltip
    "Choose which mods content actions affect. Folder removal always unassigns all members.", // FolderBatchScopeTooltip
    "Completed", // FolderBatchCompleted
    "Skipped: locked", // FolderBatchSkippedLocked
    "Unsupported", // FolderBatchUnsupported
    "Changed or missing", // FolderBatchChangedMissing
    "Failed", // FolderBatchFailed
    "Hidden mod", // FolderBatchHiddenMod
    "Content actions: matching current filters", // FolderBatchScopeSummaryVisible
    "Content actions: all mods in selected folders", // FolderBatchScopeSummaryAll
    "Change", // FolderBatchScopeChange
    "{mods} mods affected", // FolderBatchScopeModCount
    "Details", // FolderBatchReportDetails
    "Turn this mod on to use its hotkeys.", // GameOverlayHotkeyModOff
    "The key didn't reach the game. Let go of all keys and try again.", // GameOverlayHotkeyNotSent
    "Turn on \"Let Hestia modify d3dx.ini configuration\" in Hestia's settings to press mod hotkeys here.", // GameOverlayHotkeyNotAllowed
    "Press this key", // GameOverlayHintPressHotkey
];
