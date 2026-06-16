const TEXT_KEY_COUNT: usize = TextKey::COUNT;

#[derive(Clone, Copy)]
struct TextCatalog {
    language: AppLanguage,
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum TextKey {
    WhatsNewWindowTitle,
    WhatsNewFeedbackSurveyTooltip,

    FeedbackSurveyOptional,
    FeedbackSurveySubmitting,
    FeedbackSurveySubmitFeedback,
    FeedbackSurveyDismiss,
    FeedbackSurveyRemindLater,
    FeedbackSurveySkipVersion,
    FeedbackSurveyNeverAskAgain,
    FeedbackSurveyPrivacyDetails,
    FeedbackSurveyPrivacyCopy,
    FeedbackSurveyPrivacyPayload,
    FeedbackSurveyResultsHeader,
    FeedbackSurveyResultsOngoing,
    FeedbackSurveyResultsPrevious,

    LogWindowTitle,
    LogCopied,

    TasksWindowTitle,
    TasksOngoing,
    TasksOngoingCount,
    TasksNoActiveTasks,
    TasksCompleted,
    TasksCompletedCount,
    TasksNoCompletedTasks,
    TasksDownloads,
    TasksDownloadsCount,
    TasksInstalls,
    TasksInstallsCount,
    TasksFailed,
    TasksFailedCount,
    TasksNoTasks,
    TasksStatusQueued,
    TasksStatusInstalling,
    TasksStatusDownloading,
    TasksStatusCanceling,
    TasksStatusCompleted,
    TasksStatusFailed,
    TasksStatusCanceled,
    TasksCanceling,
    TasksCancel,
    TasksRetry,
    TasksResume,
    TasksStartingDownload,
    TasksQueuedProgress,
    TasksInstallingModFiles,
    TasksCancelingTask,

    ToolsWindowTitle,
    ToolsNoGameSelected,
    ToolsLaunch,
    ToolsSetLaunchOptions,
    ToolsOpenFolder,
    ToolsUnpinFromTitlebar,
    ToolsPinToTitlebar,
    ToolsRemove,
    ToolsAddTool,
    ToolsFallbackLabel,
    ToolsNoGameSelectedForAdd,
    ToolsAlreadyAdded,
    ToolsToolAdded,
    ToolsToolRemoved,
    ToolsTitlebarLimit,
    ToolsTitlebarLimitReached,
    ToolsExecutableMissing,
    ToolsNotFound,
    ToolsLaunched,
    ToolsCouldNotLaunch,
    ToolsCouldNotOpenLocation,
    ToolsLaunchOptionsSaved,
    ToolsActionAdded,
    ToolsActionRemoved,
    ToolsActionLaunched,

    ToolLaunchOptionsWindowTitle,
    ToolLaunchOptionsHint,
    ToolLaunchOptionsSave,
    ToolLaunchOptionsCancel,

    DialogScanningPaths,
    DialogFindingPaths,
    DialogDeepScanningPaths,
    DialogScanResults,
    DialogContinue,
    DialogStopScan,
    DialogStopped,
    DialogNotFound,
    DialogSearching,
    DialogFound,
    DialogChoose,
    DialogMultipleFound,
    DialogImportedMod,
    DialogMissingIniTitle,
    DialogMissingIniPrompt,
    DialogInstall,
    DialogInstallMerged,
    DialogInstallMergedTooltip,
    DialogInstallSeparately,
    DialogInstallSeparatelyTooltip,
    DialogInstallFailed,
    DialogInstallUnavailable,
    DialogInstallFailedFor,
    DialogInstallInspectionFailed,
    DialogInstallDispatchFailed,
    DialogInstallStartFailed,
    DialogSelectGameFirst,
    DialogNoFoldersSelected,
    DialogInstallCanceled,
    DialogInstallCanceledMessage,
    DialogInstallationConflict,
    DialogThisFolder,
    DialogAlreadyExistsIn,
    DialogReplace,
    DialogMerge,
    DialogKeepBoth,
    DialogConflictReplace,
    DialogConflictMerge,
    DialogConflictKeepBoth,
    DialogConflictCancel,
    DialogDropModsImages,
    DialogDropToInstall,
    DialogUnsupported,
    DialogUnsupportedFile,
    DialogFile,
    DialogFileFilterArchives,
    DialogFileFilterExecutable,
    DialogOpenUnlinkedModDetailFirst,
    DialogInstallingCount,
    DialogCouldNotCreateModsFolder,
    DialogCouldNotDisableInstalledMod,
    DialogCouldNotKeepModDisabled,
    DialogInstalledAction,
    DialogInstalledCount,
    DialogInstalledName,
    DialogSyncedAction,
    DialogUpdateUnavailable,
    DialogUpdatingTask,

    AppCouldNotSaveSettings,
    AppCouldNotSaveData,
    AppLogWarn,
    AppLogError,
    AppLaunchFailed,
    AppLaunchPathNotSet,
    AppGameNotSelected,
    AppPlayModded,
    AppPlayVanilla,
    AppModded,
    AppVanilla,
    AppLaunchPathNotSetForGame,
    AppLaunchedGameMode,
    AppNoFeedbackSurveyConfigured,
    AppAddingClipboardImage,
    AppCouldNotPasteImage,
    AppCouldNotAttachImages,
    AppCouldNotSaveImages,
    AppImagesAddedAction,
    AppImagesAdded,
    AppCouldNotAddImages,
    AppWatchPreview,
    AppCouldNotOpenBrowser,
    AppCouldNotRefreshMods,
    AppModsScannedNoChanges,
    AppReloadedNoChanges,
    AppModsScanned,
    AppReloaded,
    AppReloadAdded,
    AppReloadRemoved,
    AppReloadChanged,
    AppReloadAction,
    AppCategoryAction,
    AppCategoryCreated,
    AppCategorySkippedNoValidGameBananaCategory,
    AppSurveyAction,
    AppSurveyDiscardedUnreadablePendingFeedbackPayload,
    AppSurveyRetryingPendingFeedbackPayload,
    AppSurveySubmittedFeedback,
    AppSurveyFeedbackSubmitFailed,
    AppSurveyDiscardedPendingFeedbackPayload,
    AppCouldNotSubmitFeedback,
    AppFeedbackSubmitted,
    AppDownloadCanceled,

    ChromeAppSubtitle,
    ChromePlay,
    ChromeInstallArchive,
    ChromeInstallFolder,
    ChromeReload,
    ChromeGameNotInstalled,
    ChromeLaunchWithModsTooltip,
    ChromeLaunchWithoutModsTooltip,
    ChromePlayWithMods,
    ChromePlayWithoutMods,
    ChromeInstallArchiveTooltip,
    ChromeInstallFolderTooltip,
    ChromeInstall,
    ChromeInstallDisabled,
    ChromeReloadLibraryTooltip,
    ChromeReloadBrowseTooltip,
    ChromeClose,
    ChromeRestore,
    ChromeMaximize,
    ChromeMinimize,
    ChromeMyMods,
    ChromeBrowse,
    ChromeToolsTooltip,
    ChromeTasksTooltip,
    ChromeLogTooltip,
    ChromeSettingsTooltip,
    ChromeNoGamesDetected,
    ChromeSeeSettingsGamePath,

    BrowseSearchHint,
    BrowseModsTitle,
    BrowseCharacters,
    BrowsePopular,
    BrowseRecentUpdated,
    BrowseBestMatch,
    BrowseModsCount,
    BrowseLoading,
    BrowseHiddenNsfwCount,
    BrowseSelectedCharacterModsCount,
    BrowseShowAllMods,
    BrowseFetchingMods,
    BrowseInstalled,
    BrowseOpenInBrowser,
    BrowseCouldNotOpenBrowser,
    BrowseLoadingMore,
    BrowseNoCharacterList,
    BrowseRefreshCharacters,
    BrowseClearFilter,
    BrowseSelectedCharacter,
    BrowseCharacterCount,
    BrowseWaiting,
    BrowseNoCharactersReturned,
    BrowseModDetail,
    BrowseCopyGameBananaId,
    BrowseGameBananaIdCopied,
    BrowseUnknown,
    BrowseUpdates,
    BrowsePrivateMod,
    BrowseAutomaticInstallDisabledAuthorized,
    BrowseWithheldMod,
    BrowseWithheldBy,
    BrowseAutomaticInstallDisabledWithheld,
    BrowseRuleViolation,
    BrowseDeletedModNoLongerExists,
    BrowseDeletedBy,
    BrowseDeleted,
    BrowseFiles,
    BrowseArchivedFiles,
    BrowseLoadingDetails,
    BrowseFileMetadata,
    BrowseChooseFiles,
    BrowseMultipleFilesPrompt,
    BrowseNoConfiguredCharacterCategoryList,
    BrowseCharactersUnavailable,
    BrowseConnectionFailed,
    BrowseFailed,
    BrowseCharactersFailed,
    BrowseDetailFailed,
    BrowseCouldNotLoadUpdates,
    BrowseDownloaded,
    BrowseCouldNotPrepareInstall,
    BrowseDownloadFailed,
    BrowseResolvingDownload,
    BrowseNoDownloadableFilesFound,
    BrowseNoFilesSelected,
    BrowseDownloadQueued,
    BrowsePageWarning,
    BrowsePageFailed,
    BrowseCharacterCategoriesWarning,
    BrowseCharacterCategoriesFailed,
    BrowseDetailWarning,
    BrowseDetailFailedMessage,
    BrowseUpdatesWarning,
    BrowseUpdatesFailedMessage,
    BrowseDownloadFailedMessage,

    LibraryScanningInstalledMods,
    LibraryEnsureXxmiInstalled,
    LibraryDownloadXxmi,
    LibraryBlankInstructions,
    LibraryOpenSettings,
    LibrarySearchHint,
    LibraryInstalledMods,
    LibrarySelectedCount,
    LibrarySelectAllVisibleMods,
    LibraryModsCount,
    LibraryOneMod,
    LibraryBack,
    LibraryBackToCategoryFolders,
    LibraryCategorySummary,
    LibrarySortNameAsc,
    LibrarySortNameDesc,
    LibrarySortDateDesc,
    LibrarySortDateAsc,
    LibrarySortMenuTooltip,
    LibrarySortModsHeading,
    LibrarySortNameTooltip,
    LibrarySortNewestTooltip,
    LibrarySortOldestTooltip,
    LibraryGroupModsHeading,
    LibraryGroupCategoryTooltip,
    LibraryGroupStatusTooltip,
    LibraryGroupNoneTooltip,
    LibraryCategoryLayoutHeading,
    LibraryAvailableWhenGroupedByCategory,
    LibraryCategoryFoldersTooltip,
    LibraryCategoryListTooltip,
    LibrarySortCategoriesHeading,
    LibraryCategorySortManual,
    LibraryCategorySortByNameAsc,
    LibraryCategorySortByLeastMods,
    LibraryCategorySortByMostMods,
    LibraryCategorySortManualTooltip,
    LibraryCategorySortByNameTooltip,
    LibraryCategorySortByMostModsTooltip,
    LibraryCategorySortByLeastModsTooltip,
    LibraryMiscellaneousHeading,
    LibrarySortCategoryFirstTooltip,
    LibrarySortStatusFirstTooltip,
    LibraryUncategorizedFirstListOnlyTooltip,
    LibraryToggleVisibility,
    LibraryModStateHeading,
    LibraryShowAllModStates,
    LibraryHideAllModStates,
    LibraryEnabledMods,
    LibraryDisabledMods,
    LibraryArchivedMods,
    LibraryUpdateStateHeading,
    LibraryShowAllUpdateStates,
    LibraryHideAllUpdateStates,
    LibraryUnlinked,
    LibraryUpToDate,
    LibraryUpdateAvailable,
    LibraryCheckSkipped,
    LibraryMissingSource,
    LibraryModifiedLocally,
    LibraryIgnoringUpdate,
    LibraryIgnoringUpdateTooltip,
    LibraryUpdate,
    LibraryEnable,
    LibraryDisable,
    LibraryArchive,
    LibraryMore,
    LibraryNone,
    LibraryNoCategoryHelp,
    LibraryNoCategoryYet,
    LibraryNewCategory,
    LibraryOpen,
    LibraryFileExplorer,
    LibraryNoGameBananaSource,
    LibraryIgnoreUpdateOnce,
    LibraryIgnoreUpdateOnceTooltip,
    LibraryIgnoreUpdateOnceDisabledTooltip,
    LibraryIgnoreUpdateOnceBulkDisabledTooltip,
    LibraryIgnoreUpdateAlways,
    LibraryIgnoreUpdateAlwaysTooltip,
    LibraryModified,
    LibraryModifiedSuffix,
    LibraryAndMore,
    LibraryModifiedIgnoringOnce,
    LibraryModifiedIgnoringAlways,
    LibraryModifiedUpdateAvailable,
    LibraryIgnoringOnce,
    LibraryIgnoringAlways,
    LibraryMissing,
    LibrarySkipped,
    LibraryEmpty,
    LibraryMoving,
    LibraryMoveHere,
    LibraryOpenItem,
    LibraryDropOnCategory,
    LibraryReorderFolder,
    LibraryCategoriesHeading,
    LibraryFoldersUncategorizedSummary,
    LibraryDropSwitchesToManualOrder,
    LibraryRename,
    LibraryRenameShortcut,
    LibraryFolderOnlyMoveModsOutside,
    LibraryFolderAndModsInside,
    LibraryDeletedFolder,
    LibraryStatusActive,
    LibraryStatusDisabled,
    LibraryStatusArchived,
    LibraryRecycledAction,
    LibraryDeletedAction,
    LibraryDeleteFailed,
    LibraryDisableFailed,
    LibraryArchiveFailed,
    LibraryEnableFailed,
    LibraryRestoreFailed,
    LibraryActionDisabled,
    LibraryActionArchived,
    LibraryActionEnabled,
    LibraryActionUnarchived,
    LibraryActionMessage,
    LibraryActionCountMessage,
    LibraryCategoryActionCountMessage,
    LibraryQueuedUpdates,
    LibraryRenameFailed,
    LibraryActionRenamed,
    LibraryRenamedTo,
    LibraryPersonalNote,
    LibrarySavedPersonalNote,
    LibraryPersonalNoteRemoved,
    LibraryCouldNotSavePersonalNote,
    LibraryRemoveImage,
    LibraryClickHereTo,
    LibraryManuallyAddImages,
    LibraryDropImagesHere,
    LibraryPasteFromClipboard,
    LibraryAddingImages,
    LibraryAddImages,
    LibraryImagesFileDialog,
    LibraryAddingImagesCount,
    LibraryCouldNotAddImages,
    LibraryImageRemoved,
    LibraryCouldNotRemoveImage,
    LibraryDescription,
    LibraryMetadata,
    LibraryRequiresRabbitFx,
    LibraryAddPersonalNote,
    LibrarySavePersonalNote,
    LibraryEditableUserNote,
    LibraryEditPersonalNote,
    LibraryAddNote,
    LibraryLocal,
    LibraryOpenInFileExplorer,
    LibrarySource,
    LibraryLastSynced,
    LibraryResync,
    LibraryUnlink,
    LibraryGameBananaPage,
    LibraryLinkGameBananaPrompt,
    LibraryUrlOrId,
    LibrarySyncMod,
    LibraryUpdatePreferences,
    LibrarySyncingGameBanana,

    SettingsWindowTitle,
    SettingsTabGeneral,
    SettingsTabCategory,
    SettingsTabAdvanced,
    SettingsTabGamePath,
    SettingsTabAbout,

    SettingsGeneralBehaviorSection,
    SettingsGeneralBehaviorWhenLaunchingGame,
    SettingsGeneralBehaviorAfterInstallingMod,
    SettingsGeneralBehaviorWhenLaunchingTool,
    SettingsGeneralBehaviorModDetailMetadata,
    SettingsGeneralBehaviorDoNothing,
    SettingsGeneralBehaviorMinimizeHestia,
    SettingsGeneralBehaviorExitHestia,
    SettingsGeneralBehaviorAddToSelection,
    SettingsGeneralBehaviorOpenModDetail,
    SettingsGeneralBehaviorNeverShow,
    SettingsGeneralBehaviorShowIfNoDescription,
    SettingsGeneralBehaviorAlwaysShow,

    SettingsGeneralInstalledModsListSection,
    SettingsGeneralInstalledModsGroupListBy,
    SettingsGeneralInstalledModsCategoryLayout,
    SettingsGeneralInstalledModsGroupCategory,
    SettingsGeneralInstalledModsGroupStatus,
    SettingsGeneralInstalledModsGroupNone,
    SettingsGeneralInstalledModsLayoutList,
    SettingsGeneralInstalledModsLayoutFolders,
    SettingsGeneralInstalledModsSortByCategoryFirst,
    SettingsGeneralInstalledModsSortByCategoryFirstTooltip,
    SettingsGeneralInstalledModsSortByStatusFirst,
    SettingsGeneralInstalledModsSortByStatusFirstTooltip,
    SettingsGeneralInstalledModsShowModStatusOnCard,
    SettingsGeneralInstalledModsShowCategoryOnCard,
    SettingsGeneralInstalledModsShowCategoryOnCardTooltip,
    SettingsGeneralInstalledModsShowDisabledMods,
    SettingsGeneralInstalledModsShowArchivedMods,
    SettingsGeneralInstalledModsShowUncategorizedModsFirst,

    SettingsGeneralOperationalSection,
    SettingsGeneralOperationalModsToCheckForUpdates,
    SettingsGeneralOperationalAutomaticallyUpdateMods,
    SettingsGeneralOperationalStatusActive,
    SettingsGeneralOperationalStatusDisabled,
    SettingsGeneralOperationalStatusArchived,
    SettingsGeneralOperationalAlsoUpdateModifiedMods,
    SettingsGeneralOperationalYes,
    SettingsGeneralOperationalNoButShowUpdateButton,
    SettingsGeneralOperationalNoAndHideUpdateButton,
    SettingsGeneralOperationalWhenInstallingExistingMod,
    SettingsGeneralOperationalAlwaysAsk,
    SettingsGeneralOperationalAlwaysReplace,
    SettingsGeneralOperationalAlwaysMerge,
    SettingsGeneralOperationalAlwaysKeepBoth,
    SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods,
    SettingsGeneralOperationalWhenDeletingMod,
    SettingsGeneralOperationalMoveToRecycleBin,
    SettingsGeneralOperationalDeletePermanently,

    SettingsGeneralTasksSection,
    SettingsGeneralTasksLayout,
    SettingsGeneralTasksLayoutSections,
    SettingsGeneralTasksLayoutTabbed,
    SettingsGeneralTasksLayoutSingleList,
    SettingsGeneralTasksClearCompletedTasks,
    SettingsGeneralTasksClearTasks,
    SettingsGeneralTasksOrder,
    SettingsGeneralTasksOldestToNewest,
    SettingsGeneralTasksNewestToOldest,

    SettingsCategorySelectGame,
    SettingsCategoryBrowseSection,
    SettingsCategoryAutoCreateGameBananaCategories,
    SettingsCategoryAppliesToGame,
    SettingsCategoryCategoriesSection,
    SettingsCategorySelectAllCategories,
    SettingsCategoryUnselectAllCategories,
    SettingsCategoryNew,
    SettingsCategoryNewTooltip,
    SettingsCategoryDelete,
    SettingsCategoryUncategorized,

    SettingsPathScanTitle,
    SettingsPathScanDescription,
    SettingsPathScanButtonScan,
    SettingsPathScanButtonScanning,
    SettingsPathScanButtonTooltip,
    SettingsPathXxmiSection,
    SettingsPathXxmiLauncher,
    SettingsPathPathNotFound,
    SettingsPathUseDefaultXxmiModPath,
    SettingsPathGameSection,
    SettingsPathGameExeFile,
    SettingsPathGameModsFolder,

    SettingsAdvancedAppearanceSection,
    SettingsAdvancedAppearanceLanguage,
    SettingsAdvancedAppearanceFontStyle,
    SettingsAdvancedAppearanceFontClassic,
    SettingsAdvancedAppearanceFontModern,
    SettingsAdvancedAppearanceFontClassicTooltip,
    SettingsAdvancedAppearanceFontModernTooltip,

    SettingsAdvancedContentRestrictionSection,
    SettingsAdvancedContentRestrictionHideUnsafeContents,
    SettingsAdvancedContentRestrictionHideNsfwHideCounter,
    SettingsAdvancedContentRestrictionHideNsfwShowCounter,
    SettingsAdvancedContentRestrictionShowImagesCensored,
    SettingsAdvancedContentRestrictionShowUnrestricted,

    SettingsAdvancedCacheArchiveSection,
    SettingsAdvancedCacheArchiveCacheSize,
    SettingsAdvancedCacheArchiveCurrentUsage,
    SettingsAdvancedCacheArchiveClearCache,
    SettingsAdvancedCacheArchiveCacheCleared,
    SettingsAdvancedCacheArchiveClearCacheFailed,
    SettingsAdvancedCacheArchiveArchiveUsage,
    SettingsAdvancedCacheArchiveDeleteArchivedMods,
    SettingsAdvancedCacheArchiveRecycled,
    SettingsAdvancedCacheArchiveDeleted,
    SettingsAdvancedCacheArchiveArchivedMods,
    SettingsAdvancedCacheArchiveArchivesCleared,
    SettingsAdvancedCacheArchiveNoArchivesToClear,
    SettingsAdvancedCacheArchiveClearArchivesFailed,

    SettingsAboutBy,
    SettingsAboutVersion,
    SettingsAboutVersionTooltip,
    SettingsAboutAutomaticallyCheckForUpdate,
    SettingsAboutUpdateChecking,
    SettingsAboutUpdateRestartToUpdate,
    SettingsAboutUpdateCheckForUpdate,
    SettingsAboutUpdateUpToDate,
    SettingsAboutUpdateFailedToCheck,
    SettingsAboutUpdateManualRequired,
    SettingsAboutUpdateAvailable,
    SettingsAboutUpdateReady,
    SettingsAboutUpdateFailed,
    SettingsAboutUpdateDownloadCanceled,
    SettingsAboutUpdateWaitForActiveTasks,
    SettingsAboutUpdateCouldNotApply,
    SettingsAboutUpdateManualInstallFolder,
    SettingsAboutAttributionSection,
    SettingsAboutAttributionGameBanana,

    TranslationFailed,
    TranslationInProgress,
}

impl TextKey {
    const COUNT: usize = Self::TranslationInProgress as usize + 1;
}

include!("i18n/en_us.rs");
include!("i18n/id_id.rs");
include!("i18n/zh_cn.rs");
include!("i18n/ru_ru.rs");

impl TextCatalog {
    fn new(language: AppLanguage) -> Self {
        Self { language }
    }

    fn get(self, key: TextKey) -> &'static str {
        let index = key as usize;
        match self.language {
            AppLanguage::English => EN_US[index],
            AppLanguage::Indonesian => ID_ID[index],
            AppLanguage::ChineseSimplified => ZH_CN[index],
            AppLanguage::Russian => RU_RU[index],
        }
    }

    fn count_label(self, key: TextKey, count: usize) -> String {
        self.get(key).replace("{count}", &count.to_string())
    }

    fn whats_new(self) -> &'static str {
        self.get(TextKey::WhatsNewWindowTitle)
    }

    fn whats_new_feedback_survey_tooltip(self) -> &'static str {
        self.get(TextKey::WhatsNewFeedbackSurveyTooltip)
    }

    fn feedback_survey_optional(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyOptional)
    }

    fn feedback_survey_submit_label(self, submitting: bool) -> &'static str {
        if submitting {
            self.get(TextKey::FeedbackSurveySubmitting)
        } else {
            self.get(TextKey::FeedbackSurveySubmitFeedback)
        }
    }

    fn feedback_survey_dismiss(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyDismiss)
    }

    fn feedback_survey_remind_later(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyRemindLater)
    }

    fn feedback_survey_skip_version(self) -> &'static str {
        self.get(TextKey::FeedbackSurveySkipVersion)
    }

    fn feedback_survey_never_ask_again(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyNeverAskAgain)
    }

    fn feedback_survey_privacy_details(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyPrivacyDetails)
    }

    fn feedback_survey_privacy_copy(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyPrivacyCopy)
    }

    fn feedback_survey_privacy_payload(self, server_url: &str) -> String {
        self.get(TextKey::FeedbackSurveyPrivacyPayload)
            .replace("{server_url}", server_url)
    }

    fn feedback_survey_results_header(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyResultsHeader)
    }

    fn feedback_survey_results_ongoing(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyResultsOngoing)
    }

    fn feedback_survey_results_previous(self) -> &'static str {
        self.get(TextKey::FeedbackSurveyResultsPrevious)
    }

    fn log(self) -> &'static str {
        self.get(TextKey::LogWindowTitle)
    }

    fn log_copied(self) -> &'static str {
        self.get(TextKey::LogCopied)
    }

    fn tasks_window(self) -> &'static str {
        self.get(TextKey::TasksWindowTitle)
    }

    fn tasks_ongoing(self) -> &'static str {
        self.get(TextKey::TasksOngoing)
    }

    fn tasks_ongoing_count(self, count: usize) -> String {
        self.count_label(TextKey::TasksOngoingCount, count)
    }

    fn no_active_tasks(self) -> &'static str {
        self.get(TextKey::TasksNoActiveTasks)
    }

    fn tasks_completed(self) -> &'static str {
        self.get(TextKey::TasksCompleted)
    }

    fn tasks_completed_count(self, count: usize) -> String {
        self.count_label(TextKey::TasksCompletedCount, count)
    }

    fn no_completed_tasks(self) -> &'static str {
        self.get(TextKey::TasksNoCompletedTasks)
    }

    fn tasks_downloads(self) -> &'static str {
        self.get(TextKey::TasksDownloads)
    }

    fn tasks_downloads_count(self, count: usize) -> String {
        self.count_label(TextKey::TasksDownloadsCount, count)
    }

    fn tasks_installs(self) -> &'static str {
        self.get(TextKey::TasksInstalls)
    }

    fn tasks_installs_count(self, count: usize) -> String {
        self.count_label(TextKey::TasksInstallsCount, count)
    }

    fn tasks_failed(self) -> &'static str {
        self.get(TextKey::TasksFailed)
    }

    fn tasks_failed_count(self, count: usize) -> String {
        self.count_label(TextKey::TasksFailedCount, count)
    }

    fn no_tasks(self) -> &'static str {
        self.get(TextKey::TasksNoTasks)
    }

    fn task_status_label(self, status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Queued => self.get(TextKey::TasksStatusQueued),
            TaskStatus::Installing => self.get(TextKey::TasksStatusInstalling),
            TaskStatus::Downloading => self.get(TextKey::TasksStatusDownloading),
            TaskStatus::Canceling => self.get(TextKey::TasksStatusCanceling),
            TaskStatus::Completed => self.get(TextKey::TasksStatusCompleted),
            TaskStatus::Failed => self.get(TextKey::TasksStatusFailed),
            TaskStatus::Canceled => self.get(TextKey::TasksStatusCanceled),
        }
    }

    fn task_canceling(self) -> &'static str {
        self.get(TextKey::TasksCanceling)
    }

    fn task_cancel(self) -> &'static str {
        self.get(TextKey::TasksCancel)
    }

    fn task_retry(self) -> &'static str {
        self.get(TextKey::TasksRetry)
    }

    fn task_resume(self) -> &'static str {
        self.get(TextKey::TasksResume)
    }

    fn task_starting_download(self) -> &'static str {
        self.get(TextKey::TasksStartingDownload)
    }

    fn task_queued_progress(self) -> &'static str {
        self.get(TextKey::TasksQueuedProgress)
    }

    fn task_installing_mod_files(self) -> &'static str {
        self.get(TextKey::TasksInstallingModFiles)
    }

    fn task_canceling_task(self) -> &'static str {
        self.get(TextKey::TasksCancelingTask)
    }

    fn tools(self) -> &'static str {
        self.get(TextKey::ToolsWindowTitle)
    }

    fn no_game_selected(self) -> &'static str {
        self.get(TextKey::ToolsNoGameSelected)
    }

    fn launch(self) -> &'static str {
        self.get(TextKey::ToolsLaunch)
    }

    fn set_launch_options(self) -> &'static str {
        self.get(TextKey::ToolsSetLaunchOptions)
    }

    fn open_folder(self) -> &'static str {
        self.get(TextKey::ToolsOpenFolder)
    }

    fn unpin_from_titlebar(self) -> &'static str {
        self.get(TextKey::ToolsUnpinFromTitlebar)
    }

    fn pin_to_titlebar(self) -> &'static str {
        self.get(TextKey::ToolsPinToTitlebar)
    }

    fn remove(self) -> &'static str {
        self.get(TextKey::ToolsRemove)
    }

    fn add_tool(self) -> &'static str {
        self.get(TextKey::ToolsAddTool)
    }

    fn tool_fallback_label(self) -> &'static str {
        self.get(TextKey::ToolsFallbackLabel)
    }

    fn no_game_selected_for_tool_add(self) -> &'static str {
        self.get(TextKey::ToolsNoGameSelectedForAdd)
    }

    fn tool_already_added(self) -> &'static str {
        self.get(TextKey::ToolsAlreadyAdded)
    }

    fn tool_added(self) -> &'static str {
        self.get(TextKey::ToolsToolAdded)
    }

    fn tool_removed(self) -> &'static str {
        self.get(TextKey::ToolsToolRemoved)
    }

    fn titlebar_tool_limit(self) -> &'static str {
        self.get(TextKey::ToolsTitlebarLimit)
    }

    fn titlebar_tool_limit_reached(self) -> &'static str {
        self.get(TextKey::ToolsTitlebarLimitReached)
    }

    fn tool_executable_missing(self) -> &'static str {
        self.get(TextKey::ToolsExecutableMissing)
    }

    fn tool_not_found(self, path: &str) -> String {
        self.get(TextKey::ToolsNotFound).replace("{path}", path)
    }

    fn launched_tool(self, tool: &str) -> String {
        self.get(TextKey::ToolsLaunched).replace("{tool}", tool)
    }

    fn could_not_launch_tool(self) -> &'static str {
        self.get(TextKey::ToolsCouldNotLaunch)
    }

    fn could_not_open_location(self) -> &'static str {
        self.get(TextKey::ToolsCouldNotOpenLocation)
    }

    fn tool_launch_options_saved(self) -> &'static str {
        self.get(TextKey::ToolsLaunchOptionsSaved)
    }

    fn tool_action_added(self) -> &'static str {
        self.get(TextKey::ToolsActionAdded)
    }

    fn tool_action_removed(self) -> &'static str {
        self.get(TextKey::ToolsActionRemoved)
    }

    fn tool_action_launched(self) -> &'static str {
        self.get(TextKey::ToolsActionLaunched)
    }

    fn tool_launch_options(self) -> &'static str {
        self.get(TextKey::ToolLaunchOptionsWindowTitle)
    }

    fn tool_launch_options_hint(self) -> &'static str {
        self.get(TextKey::ToolLaunchOptionsHint)
    }

    fn save(self) -> &'static str {
        self.get(TextKey::ToolLaunchOptionsSave)
    }

    fn cancel(self) -> &'static str {
        self.get(TextKey::ToolLaunchOptionsCancel)
    }

    fn scanning_paths(self) -> &'static str {
        self.get(TextKey::DialogScanningPaths)
    }

    fn finding_paths(self) -> &'static str {
        self.get(TextKey::DialogFindingPaths)
    }

    fn deep_scanning_paths(self) -> &'static str {
        self.get(TextKey::DialogDeepScanningPaths)
    }

    fn scan_results(self) -> &'static str {
        self.get(TextKey::DialogScanResults)
    }

    fn continue_label(self) -> &'static str {
        self.get(TextKey::DialogContinue)
    }

    fn stop_scan(self) -> &'static str {
        self.get(TextKey::DialogStopScan)
    }

    fn stopped(self) -> &'static str {
        self.get(TextKey::DialogStopped)
    }

    fn not_found(self) -> &'static str {
        self.get(TextKey::DialogNotFound)
    }

    fn searching(self) -> &'static str {
        self.get(TextKey::DialogSearching)
    }

    fn found(self) -> &'static str {
        self.get(TextKey::DialogFound)
    }

    fn choose(self) -> &'static str {
        self.get(TextKey::DialogChoose)
    }

    fn multiple_found(self) -> &'static str {
        self.get(TextKey::DialogMultipleFound)
    }

    fn imported_mod(self) -> &'static str {
        self.get(TextKey::DialogImportedMod)
    }

    fn missing_ini_title(self) -> &'static str {
        self.get(TextKey::DialogMissingIniTitle)
    }

    fn missing_ini_prompt(self) -> &'static str {
        self.get(TextKey::DialogMissingIniPrompt)
    }

    fn install_label(self) -> &'static str {
        self.get(TextKey::DialogInstall)
    }

    fn install_merged(self) -> &'static str {
        self.get(TextKey::DialogInstallMerged)
    }

    fn install_merged_tooltip(self) -> &'static str {
        self.get(TextKey::DialogInstallMergedTooltip)
    }

    fn install_separately(self) -> &'static str {
        self.get(TextKey::DialogInstallSeparately)
    }

    fn install_separately_tooltip(self) -> &'static str {
        self.get(TextKey::DialogInstallSeparatelyTooltip)
    }

    fn install_failed(self) -> &'static str {
        self.get(TextKey::DialogInstallFailed)
    }

    fn install_unavailable(self) -> &'static str {
        self.get(TextKey::DialogInstallUnavailable)
    }

    fn install_failed_for(self, name: &str, error: &str) -> String {
        self.get(TextKey::DialogInstallFailedFor)
            .replace("{name}", name)
            .replace("{error}", error)
    }

    fn install_inspection_failed(self, name: &str, error: &str) -> String {
        self.get(TextKey::DialogInstallInspectionFailed)
            .replace("{name}", name)
            .replace("{error}", error)
    }

    fn install_dispatch_failed(self, name: &str) -> String {
        self.get(TextKey::DialogInstallDispatchFailed)
            .replace("{name}", name)
    }

    fn install_start_failed(self, name: &str, error: &str) -> String {
        self.get(TextKey::DialogInstallStartFailed)
            .replace("{name}", name)
            .replace("{error}", error)
    }

    fn install_canceled_label(self) -> &'static str {
        self.get(TextKey::DialogInstallCanceled)
    }

    fn select_game_first(self) -> &'static str {
        self.get(TextKey::DialogSelectGameFirst)
    }

    fn no_folders_selected(self) -> &'static str {
        self.get(TextKey::DialogNoFoldersSelected)
    }

    fn install_canceled(self, name: &str) -> String {
        self.get(TextKey::DialogInstallCanceledMessage)
            .replace("{name}", name)
    }

    fn installation_conflict(self) -> &'static str {
        self.get(TextKey::DialogInstallationConflict)
    }

    fn this_folder(self) -> &'static str {
        self.get(TextKey::DialogThisFolder)
    }

    fn already_exists_in(self) -> &'static str {
        self.get(TextKey::DialogAlreadyExistsIn)
    }

    fn replace(self) -> &'static str {
        self.get(TextKey::DialogReplace)
    }

    fn merge(self) -> &'static str {
        self.get(TextKey::DialogMerge)
    }

    fn keep_both(self) -> &'static str {
        self.get(TextKey::DialogKeepBoth)
    }

    fn conflict_replace(self) -> &'static str {
        self.get(TextKey::DialogConflictReplace)
    }

    fn conflict_merge(self) -> &'static str {
        self.get(TextKey::DialogConflictMerge)
    }

    fn conflict_keep_both(self) -> &'static str {
        self.get(TextKey::DialogConflictKeepBoth)
    }

    fn conflict_cancel(self) -> &'static str {
        self.get(TextKey::DialogConflictCancel)
    }

    fn drop_mods_images(self, name: &str) -> String {
        self.get(TextKey::DialogDropModsImages)
            .replace("{name}", name)
    }

    fn drop_to_install(self) -> &'static str {
        self.get(TextKey::DialogDropToInstall)
    }

    fn unsupported(self) -> &'static str {
        self.get(TextKey::DialogUnsupported)
    }

    fn unsupported_file(self, file: &str) -> String {
        self.get(TextKey::DialogUnsupportedFile)
            .replace("{file}", file)
    }

    fn file(self) -> &'static str {
        self.get(TextKey::DialogFile)
    }

    fn file_filter_archives(self) -> &'static str {
        self.get(TextKey::DialogFileFilterArchives)
    }

    fn file_filter_executable(self) -> &'static str {
        self.get(TextKey::DialogFileFilterExecutable)
    }

    fn open_unlinked_mod_detail_first(self) -> &'static str {
        self.get(TextKey::DialogOpenUnlinkedModDetailFirst)
    }

    fn installing_count(self, count: usize) -> String {
        self.count_label(TextKey::DialogInstallingCount, count)
    }

    fn could_not_create_mods_folder(self) -> &'static str {
        self.get(TextKey::DialogCouldNotCreateModsFolder)
    }

    fn could_not_disable_installed_mod(self) -> &'static str {
        self.get(TextKey::DialogCouldNotDisableInstalledMod)
    }

    fn could_not_keep_mod_disabled(self) -> &'static str {
        self.get(TextKey::DialogCouldNotKeepModDisabled)
    }

    fn installed_action(self) -> &'static str {
        self.get(TextKey::DialogInstalledAction)
    }

    fn installed_count(self, count: usize) -> String {
        self.count_label(TextKey::DialogInstalledCount, count)
    }

    fn installed_name(self, name: &str) -> String {
        self.get(TextKey::DialogInstalledName)
            .replace("{name}", name)
    }

    fn synced_action(self) -> &'static str {
        self.get(TextKey::DialogSyncedAction)
    }

    fn update_unavailable(self) -> &'static str {
        self.get(TextKey::DialogUpdateUnavailable)
    }

    fn updating_task(self, title: &str) -> String {
        self.get(TextKey::DialogUpdatingTask)
            .replace("{title}", title)
    }

    fn could_not_save_settings(self) -> &'static str {
        self.get(TextKey::AppCouldNotSaveSettings)
    }

    fn could_not_save_data(self) -> &'static str {
        self.get(TextKey::AppCouldNotSaveData)
    }

    fn log_warn(self, detail: &str) -> String {
        self.get(TextKey::AppLogWarn).replace("{detail}", detail)
    }

    fn log_error(self, detail: &str) -> String {
        self.get(TextKey::AppLogError).replace("{detail}", detail)
    }

    fn launch_failed(self) -> &'static str {
        self.get(TextKey::AppLaunchFailed)
    }

    fn launch_path_not_set(self) -> &'static str {
        self.get(TextKey::AppLaunchPathNotSet)
    }

    fn game_not_selected(self) -> &'static str {
        self.get(TextKey::AppGameNotSelected)
    }

    fn play_modded(self) -> &'static str {
        self.get(TextKey::AppPlayModded)
    }

    fn play_vanilla(self) -> &'static str {
        self.get(TextKey::AppPlayVanilla)
    }

    fn modded(self) -> &'static str {
        self.get(TextKey::AppModded)
    }

    fn vanilla(self) -> &'static str {
        self.get(TextKey::AppVanilla)
    }

    fn launch_path_not_set_for_game(self, label: &str, game: &str) -> String {
        self.get(TextKey::AppLaunchPathNotSetForGame)
            .replace("{label}", label)
            .replace("{game}", game)
    }

    fn launched_game_mode(self, game: &str, mode: &str) -> String {
        self.get(TextKey::AppLaunchedGameMode)
            .replace("{game}", game)
            .replace("{mode}", mode)
    }

    fn no_feedback_survey_configured(self) -> &'static str {
        self.get(TextKey::AppNoFeedbackSurveyConfigured)
    }

    fn adding_clipboard_image(self) -> &'static str {
        self.get(TextKey::AppAddingClipboardImage)
    }

    fn could_not_paste_image(self) -> &'static str {
        self.get(TextKey::AppCouldNotPasteImage)
    }

    fn could_not_attach_images(self) -> &'static str {
        self.get(TextKey::AppCouldNotAttachImages)
    }

    fn could_not_save_images(self) -> &'static str {
        self.get(TextKey::AppCouldNotSaveImages)
    }

    fn images_added_action(self) -> &'static str {
        self.get(TextKey::AppImagesAddedAction)
    }

    fn images_added(self, count: usize) -> String {
        self.count_label(TextKey::AppImagesAdded, count)
    }

    fn app_could_not_add_images(self) -> &'static str {
        self.get(TextKey::AppCouldNotAddImages)
    }

    fn watch_preview(self) -> &'static str {
        self.get(TextKey::AppWatchPreview)
    }

    fn app_could_not_open_browser(self) -> &'static str {
        self.get(TextKey::AppCouldNotOpenBrowser)
    }

    fn could_not_refresh_mods(self) -> &'static str {
        self.get(TextKey::AppCouldNotRefreshMods)
    }

    fn mods_scanned_no_changes(self, count: usize) -> String {
        self.count_label(TextKey::AppModsScannedNoChanges, count)
    }

    fn reloaded_no_changes(self, count: usize) -> String {
        self.count_label(TextKey::AppReloadedNoChanges, count)
    }

    fn mods_scanned(self, count: usize) -> String {
        self.count_label(TextKey::AppModsScanned, count)
    }

    fn reloaded(self, count: usize) -> String {
        self.count_label(TextKey::AppReloaded, count)
    }

    fn reload_added(self, count: usize) -> String {
        self.count_label(TextKey::AppReloadAdded, count)
    }

    fn reload_removed(self, count: usize) -> String {
        self.count_label(TextKey::AppReloadRemoved, count)
    }

    fn reload_changed(self, count: usize) -> String {
        self.count_label(TextKey::AppReloadChanged, count)
    }

    fn reload_action(self, line: &str) -> String {
        self.get(TextKey::AppReloadAction).replace("{line}", line)
    }

    fn category_action(self) -> &'static str {
        self.get(TextKey::AppCategoryAction)
    }

    fn category_created(self, category: &str) -> String {
        self.get(TextKey::AppCategoryCreated)
            .replace("{category}", category)
    }

    fn category_skipped_no_valid_gamebanana_category(self, mod_name: &str) -> String {
        self.get(TextKey::AppCategorySkippedNoValidGameBananaCategory)
            .replace("{mod}", mod_name)
    }

    fn survey_action(self) -> &'static str {
        self.get(TextKey::AppSurveyAction)
    }

    fn survey_discarded_unreadable_pending_feedback_payload(self, error: &str) -> String {
        self.get(TextKey::AppSurveyDiscardedUnreadablePendingFeedbackPayload)
            .replace("{error}", error)
    }

    fn survey_retrying_pending_feedback_payload(self) -> &'static str {
        self.get(TextKey::AppSurveyRetryingPendingFeedbackPayload)
    }

    fn survey_submitted_feedback(self, version: &str) -> String {
        self.get(TextKey::AppSurveySubmittedFeedback)
            .replace("{version}", version)
    }

    fn survey_feedback_submit_failed(self, version: &str, error: &str) -> String {
        self.get(TextKey::AppSurveyFeedbackSubmitFailed)
            .replace("{version}", version)
            .replace("{error}", error)
    }

    fn survey_discarded_pending_feedback_payload(self, version: &str) -> String {
        self.get(TextKey::AppSurveyDiscardedPendingFeedbackPayload)
            .replace("{version}", version)
    }

    fn could_not_submit_feedback(self) -> &'static str {
        self.get(TextKey::AppCouldNotSubmitFeedback)
    }

    fn feedback_submitted(self) -> &'static str {
        self.get(TextKey::AppFeedbackSubmitted)
    }

    fn download_canceled(self, title: &str) -> String {
        self.get(TextKey::AppDownloadCanceled)
            .replace("{title}", title)
    }

    fn app_subtitle(self) -> &'static str {
        self.get(TextKey::ChromeAppSubtitle)
    }

    fn play(self) -> &'static str {
        self.get(TextKey::ChromePlay)
    }

    fn install_archive(self) -> &'static str {
        self.get(TextKey::ChromeInstallArchive)
    }

    fn install_folder(self) -> &'static str {
        self.get(TextKey::ChromeInstallFolder)
    }

    fn reload(self) -> &'static str {
        self.get(TextKey::ChromeReload)
    }

    fn game_not_installed(self) -> &'static str {
        self.get(TextKey::ChromeGameNotInstalled)
    }

    fn launch_with_mods_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeLaunchWithModsTooltip)
    }

    fn launch_without_mods_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeLaunchWithoutModsTooltip)
    }

    fn play_with_mods(self) -> &'static str {
        self.get(TextKey::ChromePlayWithMods)
    }

    fn play_without_mods(self) -> &'static str {
        self.get(TextKey::ChromePlayWithoutMods)
    }

    fn install_archive_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeInstallArchiveTooltip)
    }

    fn install_folder_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeInstallFolderTooltip)
    }

    fn install(self) -> &'static str {
        self.get(TextKey::ChromeInstall)
    }

    fn install_disabled(self) -> &'static str {
        self.get(TextKey::ChromeInstallDisabled)
    }

    fn reload_library_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeReloadLibraryTooltip)
    }

    fn reload_browse_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeReloadBrowseTooltip)
    }

    fn close(self) -> &'static str {
        self.get(TextKey::ChromeClose)
    }

    fn restore(self) -> &'static str {
        self.get(TextKey::ChromeRestore)
    }

    fn maximize(self) -> &'static str {
        self.get(TextKey::ChromeMaximize)
    }

    fn minimize(self) -> &'static str {
        self.get(TextKey::ChromeMinimize)
    }

    fn my_mods(self) -> &'static str {
        self.get(TextKey::ChromeMyMods)
    }

    fn browse(self) -> &'static str {
        self.get(TextKey::ChromeBrowse)
    }

    fn tools_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeToolsTooltip)
    }

    fn tasks_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeTasksTooltip)
    }

    fn log_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeLogTooltip)
    }

    fn settings_tooltip(self) -> &'static str {
        self.get(TextKey::ChromeSettingsTooltip)
    }

    fn no_games_detected(self) -> &'static str {
        self.get(TextKey::ChromeNoGamesDetected)
    }

    fn see_settings_game_path(self) -> &'static str {
        self.get(TextKey::ChromeSeeSettingsGamePath)
    }

    fn browse_search_hint(self) -> &'static str {
        self.get(TextKey::BrowseSearchHint)
    }

    fn browse_mods_title(self) -> &'static str {
        self.get(TextKey::BrowseModsTitle)
    }

    fn browse_characters(self) -> &'static str {
        self.get(TextKey::BrowseCharacters)
    }

    fn browse_popular(self) -> &'static str {
        self.get(TextKey::BrowsePopular)
    }

    fn browse_recent_updated(self) -> &'static str {
        self.get(TextKey::BrowseRecentUpdated)
    }

    fn browse_best_match(self) -> &'static str {
        self.get(TextKey::BrowseBestMatch)
    }

    fn browse_mods_count(self, count: usize) -> String {
        self.get(TextKey::BrowseModsCount)
            .replace("{count}", &count.to_string())
    }

    fn browse_loading(self) -> &'static str {
        self.get(TextKey::BrowseLoading)
    }

    fn browse_hidden_nsfw_count(self, count: usize) -> String {
        self.get(TextKey::BrowseHiddenNsfwCount)
            .replace("{count}", &count.to_string())
    }

    fn browse_selected_character_mods_count(self, count: u64) -> String {
        self.get(TextKey::BrowseSelectedCharacterModsCount)
            .replace("{count}", &count.to_string())
    }

    fn browse_show_all_mods(self) -> &'static str {
        self.get(TextKey::BrowseShowAllMods)
    }

    fn browse_fetching_mods(self) -> &'static str {
        self.get(TextKey::BrowseFetchingMods)
    }

    fn installed(self) -> &'static str {
        self.get(TextKey::BrowseInstalled)
    }

    fn open_in_browser(self) -> &'static str {
        self.get(TextKey::BrowseOpenInBrowser)
    }

    fn could_not_open_browser(self) -> &'static str {
        self.get(TextKey::BrowseCouldNotOpenBrowser)
    }

    fn browse_loading_more(self) -> &'static str {
        self.get(TextKey::BrowseLoadingMore)
    }

    fn browse_no_character_list(self) -> &'static str {
        self.get(TextKey::BrowseNoCharacterList)
    }

    fn browse_refresh_characters(self) -> &'static str {
        self.get(TextKey::BrowseRefreshCharacters)
    }

    fn browse_clear_filter(self) -> &'static str {
        self.get(TextKey::BrowseClearFilter)
    }

    fn browse_selected_character(self, name: &str) -> String {
        self.get(TextKey::BrowseSelectedCharacter)
            .replace("{name}", name)
    }

    fn browse_character_count(self, count: usize) -> String {
        self.get(TextKey::BrowseCharacterCount)
            .replace("{count}", &count.to_string())
    }

    fn browse_waiting(self) -> &'static str {
        self.get(TextKey::BrowseWaiting)
    }

    fn browse_no_characters_returned(self) -> &'static str {
        self.get(TextKey::BrowseNoCharactersReturned)
    }

    fn browse_mod_detail(self) -> &'static str {
        self.get(TextKey::BrowseModDetail)
    }

    fn copy_gamebanana_id(self) -> &'static str {
        self.get(TextKey::BrowseCopyGameBananaId)
    }

    fn gamebanana_id_copied(self) -> &'static str {
        self.get(TextKey::BrowseGameBananaIdCopied)
    }

    fn unknown(self) -> &'static str {
        self.get(TextKey::BrowseUnknown)
    }

    fn browse_updates(self) -> &'static str {
        self.get(TextKey::BrowseUpdates)
    }

    fn browse_private_mod(self) -> &'static str {
        self.get(TextKey::BrowsePrivateMod)
    }

    fn browse_automatic_install_disabled_authorized(self) -> &'static str {
        self.get(TextKey::BrowseAutomaticInstallDisabledAuthorized)
    }

    fn browse_withheld_mod(self) -> &'static str {
        self.get(TextKey::BrowseWithheldMod)
    }

    fn browse_withheld_by(self) -> &'static str {
        self.get(TextKey::BrowseWithheldBy)
    }

    fn browse_automatic_install_disabled_withheld(self) -> &'static str {
        self.get(TextKey::BrowseAutomaticInstallDisabledWithheld)
    }

    fn browse_rule_violation(self) -> &'static str {
        self.get(TextKey::BrowseRuleViolation)
    }

    fn browse_deleted_mod_no_longer_exists(self) -> &'static str {
        self.get(TextKey::BrowseDeletedModNoLongerExists)
    }

    fn browse_deleted_by(self) -> &'static str {
        self.get(TextKey::BrowseDeletedBy)
    }

    fn browse_deleted(self) -> &'static str {
        self.get(TextKey::BrowseDeleted)
    }

    fn browse_files(self) -> &'static str {
        self.get(TextKey::BrowseFiles)
    }

    fn browse_archived_files(self) -> &'static str {
        self.get(TextKey::BrowseArchivedFiles)
    }

    fn browse_loading_details(self) -> &'static str {
        self.get(TextKey::BrowseLoadingDetails)
    }

    fn browse_file_metadata(self, size: String, date: String, downloads: u64) -> String {
        self.get(TextKey::BrowseFileMetadata)
            .replace("{size}", &size)
            .replace("{date}", &date)
            .replace("{downloads}", &downloads.to_string())
    }

    fn browse_choose_files(self) -> &'static str {
        self.get(TextKey::BrowseChooseFiles)
    }

    fn browse_multiple_files_prompt(self) -> &'static str {
        self.get(TextKey::BrowseMultipleFilesPrompt)
    }

    fn no_configured_character_category_list(self) -> &'static str {
        self.get(TextKey::BrowseNoConfiguredCharacterCategoryList)
    }

    fn characters_unavailable(self) -> &'static str {
        self.get(TextKey::BrowseCharactersUnavailable)
    }

    fn connection_failed(self) -> &'static str {
        self.get(TextKey::BrowseConnectionFailed)
    }

    fn browse_failed(self) -> &'static str {
        self.get(TextKey::BrowseFailed)
    }

    fn characters_failed(self) -> &'static str {
        self.get(TextKey::BrowseCharactersFailed)
    }

    fn browse_detail_failed(self) -> &'static str {
        self.get(TextKey::BrowseDetailFailed)
    }

    fn could_not_load_updates(self) -> &'static str {
        self.get(TextKey::BrowseCouldNotLoadUpdates)
    }

    fn downloaded(self, title: &str) -> String {
        self.get(TextKey::BrowseDownloaded).replace("{title}", title)
    }

    fn could_not_prepare_install(self) -> &'static str {
        self.get(TextKey::BrowseCouldNotPrepareInstall)
    }

    fn download_failed(self) -> &'static str {
        self.get(TextKey::BrowseDownloadFailed)
    }

    fn resolving_download(self, title: &str) -> String {
        self.get(TextKey::BrowseResolvingDownload)
            .replace("{title}", title)
    }

    fn no_downloadable_files_found(self) -> &'static str {
        self.get(TextKey::BrowseNoDownloadableFilesFound)
    }

    fn no_files_selected(self) -> &'static str {
        self.get(TextKey::BrowseNoFilesSelected)
    }

    fn download_queued(self) -> &'static str {
        self.get(TextKey::BrowseDownloadQueued)
    }

    fn browse_page_warning(self, warning: &str) -> String {
        self.get(TextKey::BrowsePageWarning)
            .replace("{warning}", warning)
    }

    fn browse_page_failed_message(self, error: &str) -> String {
        self.get(TextKey::BrowsePageFailed)
            .replace("{error}", error)
    }

    fn character_categories_warning(self, warning: &str) -> String {
        self.get(TextKey::BrowseCharacterCategoriesWarning)
            .replace("{warning}", warning)
    }

    fn character_categories_failed_message(self, error: &str) -> String {
        self.get(TextKey::BrowseCharacterCategoriesFailed)
            .replace("{error}", error)
    }

    fn browse_detail_warning(self, mod_id: u64, warning: &str) -> String {
        self.get(TextKey::BrowseDetailWarning)
            .replace("{mod_id}", &mod_id.to_string())
            .replace("{warning}", warning)
    }

    fn browse_detail_failed_message(self, mod_id: u64, error: &str) -> String {
        self.get(TextKey::BrowseDetailFailedMessage)
            .replace("{mod_id}", &mod_id.to_string())
            .replace("{error}", error)
    }

    fn browse_updates_warning(self, mod_id: u64, warning: &str) -> String {
        self.get(TextKey::BrowseUpdatesWarning)
            .replace("{mod_id}", &mod_id.to_string())
            .replace("{warning}", warning)
    }

    fn browse_updates_failed_message(self, mod_id: u64, error: &str) -> String {
        self.get(TextKey::BrowseUpdatesFailedMessage)
            .replace("{mod_id}", &mod_id.to_string())
            .replace("{error}", error)
    }

    fn browse_download_failed_message(self, title: &str, error: &str) -> String {
        self.get(TextKey::BrowseDownloadFailedMessage)
            .replace("{title}", title)
            .replace("{error}", error)
    }

    fn scanning_installed_mods(self) -> &'static str {
        self.get(TextKey::LibraryScanningInstalledMods)
    }

    fn ensure_xxmi_installed(self) -> &'static str {
        self.get(TextKey::LibraryEnsureXxmiInstalled)
    }

    fn download_xxmi(self) -> &'static str {
        self.get(TextKey::LibraryDownloadXxmi)
    }

    fn library_blank_instructions(self) -> &'static str {
        self.get(TextKey::LibraryBlankInstructions)
    }

    fn open_settings(self) -> &'static str {
        self.get(TextKey::LibraryOpenSettings)
    }

    fn library_search_hint(self) -> &'static str {
        self.get(TextKey::LibrarySearchHint)
    }

    fn installed_mods(self) -> &'static str {
        self.get(TextKey::LibraryInstalledMods)
    }

    fn selected_count(self, count: usize) -> String {
        self.get(TextKey::LibrarySelectedCount)
            .replace("{count}", &count.to_string())
    }

    fn select_all_visible_mods(self) -> &'static str {
        self.get(TextKey::LibrarySelectAllVisibleMods)
    }

    fn library_mods_count(self, count: usize) -> String {
        self.get(TextKey::LibraryModsCount)
            .replace("{count}", &count.to_string())
    }

    fn library_one_mod(self) -> &'static str {
        self.get(TextKey::LibraryOneMod)
    }

    fn back(self) -> &'static str {
        self.get(TextKey::LibraryBack)
    }

    fn back_to_category_folders(self) -> &'static str {
        self.get(TextKey::LibraryBackToCategoryFolders)
    }

    fn library_category_summary(self, active: usize, disabled: usize, archived: usize) -> String {
        self.get(TextKey::LibraryCategorySummary)
            .replace("{active}", &active.to_string())
            .replace("{disabled}", &disabled.to_string())
            .replace("{archived}", &archived.to_string())
    }

    fn library_sort_label(self, sort: LibrarySort) -> &'static str {
        match sort {
            LibrarySort::NameAsc => self.get(TextKey::LibrarySortNameAsc),
            LibrarySort::NameDesc => self.get(TextKey::LibrarySortNameDesc),
            LibrarySort::DateDesc => self.get(TextKey::LibrarySortDateDesc),
            LibrarySort::DateAsc => self.get(TextKey::LibrarySortDateAsc),
        }
    }

    fn library_sort_menu_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortMenuTooltip)
    }

    fn library_sort_mods_heading(self) -> &'static str {
        self.get(TextKey::LibrarySortModsHeading)
    }

    fn library_sort_name_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortNameTooltip)
    }

    fn library_sort_newest_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortNewestTooltip)
    }

    fn library_sort_oldest_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortOldestTooltip)
    }

    fn library_group_mods_heading(self) -> &'static str {
        self.get(TextKey::LibraryGroupModsHeading)
    }

    fn library_group_category_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryGroupCategoryTooltip)
    }

    fn library_group_status_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryGroupStatusTooltip)
    }

    fn library_group_none_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryGroupNoneTooltip)
    }

    fn library_category_layout_heading(self) -> &'static str {
        self.get(TextKey::LibraryCategoryLayoutHeading)
    }

    fn library_available_when_grouped_by_category(self) -> &'static str {
        self.get(TextKey::LibraryAvailableWhenGroupedByCategory)
    }

    fn library_category_folders_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategoryFoldersTooltip)
    }

    fn library_category_list_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategoryListTooltip)
    }

    fn library_sort_categories_heading(self) -> &'static str {
        self.get(TextKey::LibrarySortCategoriesHeading)
    }

    fn library_category_sort_label(self, mode: ModCategorySortMode) -> &'static str {
        match mode {
            ModCategorySortMode::Manual => self.get(TextKey::LibraryCategorySortManual),
            ModCategorySortMode::ByNameAsc => self.get(TextKey::LibraryCategorySortByNameAsc),
            ModCategorySortMode::ByModCountAsc => {
                self.get(TextKey::LibraryCategorySortByLeastMods)
            }
            ModCategorySortMode::ByModCountDesc => {
                self.get(TextKey::LibraryCategorySortByMostMods)
            }
        }
    }

    fn library_category_sort_manual_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategorySortManualTooltip)
    }

    fn library_category_sort_by_name_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategorySortByNameTooltip)
    }

    fn library_category_sort_by_most_mods_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategorySortByMostModsTooltip)
    }

    fn library_category_sort_by_least_mods_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryCategorySortByLeastModsTooltip)
    }

    fn library_miscellaneous_heading(self) -> &'static str {
        self.get(TextKey::LibraryMiscellaneousHeading)
    }

    fn library_sort_category_first_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortCategoryFirstTooltip)
    }

    fn library_sort_status_first_tooltip(self) -> &'static str {
        self.get(TextKey::LibrarySortStatusFirstTooltip)
    }

    fn library_uncategorized_first_list_only_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryUncategorizedFirstListOnlyTooltip)
    }

    fn toggle_visibility(self) -> &'static str {
        self.get(TextKey::LibraryToggleVisibility)
    }

    fn mod_state_heading(self) -> &'static str {
        self.get(TextKey::LibraryModStateHeading)
    }

    fn show_all_mod_states(self) -> &'static str {
        self.get(TextKey::LibraryShowAllModStates)
    }

    fn hide_all_mod_states(self) -> &'static str {
        self.get(TextKey::LibraryHideAllModStates)
    }

    fn enabled_mods(self) -> &'static str {
        self.get(TextKey::LibraryEnabledMods)
    }

    fn disabled_mods(self) -> &'static str {
        self.get(TextKey::LibraryDisabledMods)
    }

    fn archived_mods(self) -> &'static str {
        self.get(TextKey::LibraryArchivedMods)
    }

    fn update_state_heading(self) -> &'static str {
        self.get(TextKey::LibraryUpdateStateHeading)
    }

    fn show_all_update_states(self) -> &'static str {
        self.get(TextKey::LibraryShowAllUpdateStates)
    }

    fn hide_all_update_states(self) -> &'static str {
        self.get(TextKey::LibraryHideAllUpdateStates)
    }

    fn unlinked(self) -> &'static str {
        self.get(TextKey::LibraryUnlinked)
    }

    fn up_to_date(self) -> &'static str {
        self.get(TextKey::LibraryUpToDate)
    }

    fn update_available(self) -> &'static str {
        self.get(TextKey::LibraryUpdateAvailable)
    }

    fn check_skipped(self) -> &'static str {
        self.get(TextKey::LibraryCheckSkipped)
    }

    fn missing_source(self) -> &'static str {
        self.get(TextKey::LibraryMissingSource)
    }

    fn modified_locally(self) -> &'static str {
        self.get(TextKey::LibraryModifiedLocally)
    }

    fn ignoring_update(self) -> &'static str {
        self.get(TextKey::LibraryIgnoringUpdate)
    }

    fn ignoring_update_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryIgnoringUpdateTooltip)
    }

    fn update_button(self) -> &'static str {
        self.get(TextKey::LibraryUpdate)
    }

    fn enable(self) -> &'static str {
        self.get(TextKey::LibraryEnable)
    }

    fn disable(self) -> &'static str {
        self.get(TextKey::LibraryDisable)
    }

    fn archive(self) -> &'static str {
        self.get(TextKey::LibraryArchive)
    }

    fn more(self) -> &'static str {
        self.get(TextKey::LibraryMore)
    }

    fn none_label(self) -> &'static str {
        self.get(TextKey::LibraryNone)
    }

    fn no_category_help(self) -> &'static str {
        self.get(TextKey::LibraryNoCategoryHelp)
    }

    fn no_category_yet(self) -> &'static str {
        self.get(TextKey::LibraryNoCategoryYet)
    }

    fn new_category_name(self) -> &'static str {
        self.get(TextKey::LibraryNewCategory)
    }

    fn open(self) -> &'static str {
        self.get(TextKey::LibraryOpen)
    }

    fn file_explorer(self) -> &'static str {
        self.get(TextKey::LibraryFileExplorer)
    }

    fn no_gamebanana_source(self) -> &'static str {
        self.get(TextKey::LibraryNoGameBananaSource)
    }

    fn ignore_update_once(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateOnce)
    }

    fn ignore_update_once_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateOnceTooltip)
    }

    fn ignore_update_once_disabled_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateOnceDisabledTooltip)
    }

    fn ignore_update_once_bulk_disabled_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateOnceBulkDisabledTooltip)
    }

    fn ignore_update_always(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateAlways)
    }

    fn ignore_update_always_tooltip(self) -> &'static str {
        self.get(TextKey::LibraryIgnoreUpdateAlwaysTooltip)
    }

    fn modified(self) -> &'static str {
        self.get(TextKey::LibraryModified)
    }

    fn modified_suffix(self) -> &'static str {
        self.get(TextKey::LibraryModifiedSuffix)
    }

    fn and_more(self, count: usize) -> String {
        self.count_label(TextKey::LibraryAndMore, count)
    }

    fn modified_ignoring_once(self) -> &'static str {
        self.get(TextKey::LibraryModifiedIgnoringOnce)
    }

    fn modified_ignoring_always(self) -> &'static str {
        self.get(TextKey::LibraryModifiedIgnoringAlways)
    }

    fn modified_update_available(self) -> &'static str {
        self.get(TextKey::LibraryModifiedUpdateAvailable)
    }

    fn ignoring_once(self) -> &'static str {
        self.get(TextKey::LibraryIgnoringOnce)
    }

    fn ignoring_always(self) -> &'static str {
        self.get(TextKey::LibraryIgnoringAlways)
    }

    fn missing(self) -> &'static str {
        self.get(TextKey::LibraryMissing)
    }

    fn skipped(self) -> &'static str {
        self.get(TextKey::LibrarySkipped)
    }

    fn empty(self) -> &'static str {
        self.get(TextKey::LibraryEmpty)
    }

    fn moving(self) -> &'static str {
        self.get(TextKey::LibraryMoving)
    }

    fn move_here(self) -> &'static str {
        self.get(TextKey::LibraryMoveHere)
    }

    fn open_item(self, item: &str) -> String {
        self.get(TextKey::LibraryOpenItem).replace("{item}", item)
    }

    fn drop_on_category(self) -> &'static str {
        self.get(TextKey::LibraryDropOnCategory)
    }

    fn reorder_folder(self) -> &'static str {
        self.get(TextKey::LibraryReorderFolder)
    }

    fn categories_heading(self) -> &'static str {
        self.get(TextKey::LibraryCategoriesHeading)
    }

    fn folders_uncategorized_summary(self, folders: usize, uncategorized: usize) -> String {
        self.get(TextKey::LibraryFoldersUncategorizedSummary)
            .replace("{folders}", &folders.to_string())
            .replace("{uncategorized}", &uncategorized.to_string())
    }

    fn drop_switches_to_manual_order(self) -> &'static str {
        self.get(TextKey::LibraryDropSwitchesToManualOrder)
    }

    fn rename(self) -> &'static str {
        self.get(TextKey::LibraryRename)
    }

    fn rename_shortcut(self) -> &'static str {
        self.get(TextKey::LibraryRenameShortcut)
    }

    fn folder_only_move_mods_outside(self) -> &'static str {
        self.get(TextKey::LibraryFolderOnlyMoveModsOutside)
    }

    fn folder_and_mods_inside(self) -> &'static str {
        self.get(TextKey::LibraryFolderAndModsInside)
    }

    fn deleted_folder(self, category_name: &str) -> String {
        self.get(TextKey::LibraryDeletedFolder)
            .replace("{category}", category_name)
    }

    fn mod_status_label(self, status: &ModStatus) -> &'static str {
        match status {
            ModStatus::Active => self.get(TextKey::LibraryStatusActive),
            ModStatus::Disabled => self.get(TextKey::LibraryStatusDisabled),
            ModStatus::Archived => self.get(TextKey::LibraryStatusArchived),
        }
    }

    fn delete_action(self, behavior: DeleteBehavior) -> &'static str {
        match behavior {
            DeleteBehavior::RecycleBin => self.get(TextKey::LibraryRecycledAction),
            DeleteBehavior::Permanent => self.get(TextKey::LibraryDeletedAction),
        }
    }

    fn delete_failed(self) -> &'static str {
        self.get(TextKey::LibraryDeleteFailed)
    }

    fn disable_failed(self) -> &'static str {
        self.get(TextKey::LibraryDisableFailed)
    }

    fn archive_failed(self) -> &'static str {
        self.get(TextKey::LibraryArchiveFailed)
    }

    fn enable_failed(self) -> &'static str {
        self.get(TextKey::LibraryEnableFailed)
    }

    fn restore_failed(self) -> &'static str {
        self.get(TextKey::LibraryRestoreFailed)
    }

    fn action_disabled(self) -> &'static str {
        self.get(TextKey::LibraryActionDisabled)
    }

    fn action_archived(self) -> &'static str {
        self.get(TextKey::LibraryActionArchived)
    }

    fn action_enabled(self) -> &'static str {
        self.get(TextKey::LibraryActionEnabled)
    }

    fn action_unarchived(self) -> &'static str {
        self.get(TextKey::LibraryActionUnarchived)
    }

    fn action_message(self, action: &str, name: &str) -> String {
        self.get(TextKey::LibraryActionMessage)
            .replace("{action}", action)
            .replace("{name}", name)
    }

    fn action_count_message(self, action: &str, count: usize) -> String {
        self.get(TextKey::LibraryActionCountMessage)
            .replace("{action}", action)
            .replace("{count}", &count.to_string())
    }

    fn category_action_count_message(self, action: &str, category: &str, count: usize) -> String {
        self.get(TextKey::LibraryCategoryActionCountMessage)
            .replace("{action}", action)
            .replace("{category}", category)
            .replace("{count}", &count.to_string())
    }

    fn queued_updates(self, count: usize) -> String {
        self.count_label(TextKey::LibraryQueuedUpdates, count)
    }

    fn rename_failed(self) -> &'static str {
        self.get(TextKey::LibraryRenameFailed)
    }

    fn action_renamed(self) -> &'static str {
        self.get(TextKey::LibraryActionRenamed)
    }

    fn renamed_to(self, name: &str) -> String {
        self.get(TextKey::LibraryRenamedTo)
            .replace("{name}", name)
    }

    fn personal_note(self) -> &'static str {
        self.get(TextKey::LibraryPersonalNote)
    }

    fn saved_personal_note(self) -> &'static str {
        self.get(TextKey::LibrarySavedPersonalNote)
    }

    fn personal_note_removed(self) -> &'static str {
        self.get(TextKey::LibraryPersonalNoteRemoved)
    }

    fn could_not_save_personal_note(self) -> &'static str {
        self.get(TextKey::LibraryCouldNotSavePersonalNote)
    }

    fn remove_image(self) -> &'static str {
        self.get(TextKey::LibraryRemoveImage)
    }

    fn click_here_to(self) -> &'static str {
        self.get(TextKey::LibraryClickHereTo)
    }

    fn manually_add_images(self) -> &'static str {
        self.get(TextKey::LibraryManuallyAddImages)
    }

    fn drop_images_here(self) -> &'static str {
        self.get(TextKey::LibraryDropImagesHere)
    }

    fn paste_from_clipboard(self) -> &'static str {
        self.get(TextKey::LibraryPasteFromClipboard)
    }

    fn adding_images(self) -> &'static str {
        self.get(TextKey::LibraryAddingImages)
    }

    fn add_images(self) -> &'static str {
        self.get(TextKey::LibraryAddImages)
    }

    fn images_file_dialog(self) -> &'static str {
        self.get(TextKey::LibraryImagesFileDialog)
    }

    fn adding_images_count(self, count: usize) -> String {
        self.count_label(TextKey::LibraryAddingImagesCount, count)
    }

    fn could_not_add_images(self) -> &'static str {
        self.get(TextKey::LibraryCouldNotAddImages)
    }

    fn image_removed(self) -> &'static str {
        self.get(TextKey::LibraryImageRemoved)
    }

    fn could_not_remove_image(self) -> &'static str {
        self.get(TextKey::LibraryCouldNotRemoveImage)
    }

    fn description(self) -> &'static str {
        self.get(TextKey::LibraryDescription)
    }

    fn metadata(self) -> &'static str {
        self.get(TextKey::LibraryMetadata)
    }

    fn requires_rabbitfx(self) -> &'static str {
        self.get(TextKey::LibraryRequiresRabbitFx)
    }

    fn add_personal_note(self) -> &'static str {
        self.get(TextKey::LibraryAddPersonalNote)
    }

    fn save_personal_note(self) -> &'static str {
        self.get(TextKey::LibrarySavePersonalNote)
    }

    fn editable_user_note(self) -> &'static str {
        self.get(TextKey::LibraryEditableUserNote)
    }

    fn edit_personal_note(self) -> &'static str {
        self.get(TextKey::LibraryEditPersonalNote)
    }

    fn add_note(self) -> &'static str {
        self.get(TextKey::LibraryAddNote)
    }

    fn local(self) -> &'static str {
        self.get(TextKey::LibraryLocal)
    }

    fn open_in_file_explorer(self) -> &'static str {
        self.get(TextKey::LibraryOpenInFileExplorer)
    }

    fn source(self) -> &'static str {
        self.get(TextKey::LibrarySource)
    }

    fn last_synced(self, age: &str) -> String {
        self.get(TextKey::LibraryLastSynced).replace("{age}", age)
    }

    fn resync(self) -> &'static str {
        self.get(TextKey::LibraryResync)
    }

    fn unlink(self) -> &'static str {
        self.get(TextKey::LibraryUnlink)
    }

    fn gamebanana_page(self) -> &'static str {
        self.get(TextKey::LibraryGameBananaPage)
    }

    fn link_gamebanana_prompt(self) -> &'static str {
        self.get(TextKey::LibraryLinkGameBananaPrompt)
    }

    fn url_or_id(self) -> &'static str {
        self.get(TextKey::LibraryUrlOrId)
    }

    fn sync_mod(self) -> &'static str {
        self.get(TextKey::LibrarySyncMod)
    }

    fn update_preferences(self) -> &'static str {
        self.get(TextKey::LibraryUpdatePreferences)
    }

    fn syncing_gamebanana(self) -> &'static str {
        self.get(TextKey::LibrarySyncingGameBanana)
    }

    fn settings(self) -> &'static str {
        self.get(TextKey::SettingsWindowTitle)
    }

    fn settings_tab_general(self) -> &'static str {
        self.get(TextKey::SettingsTabGeneral)
    }

    fn settings_tab_category(self) -> &'static str {
        self.get(TextKey::SettingsTabCategory)
    }

    fn settings_tab_advanced(self) -> &'static str {
        self.get(TextKey::SettingsTabAdvanced)
    }

    fn settings_tab_game_path(self) -> &'static str {
        self.get(TextKey::SettingsTabGamePath)
    }

    fn settings_tab_about(self) -> &'static str {
        self.get(TextKey::SettingsTabAbout)
    }

    fn behavior(self) -> &'static str {
        self.get(TextKey::SettingsGeneralBehaviorSection)
    }

    fn installed_mods_list(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsListSection)
    }

    fn operational(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalSection)
    }

    fn group_list_by(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsGroupListBy)
    }

    fn category_layout(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsCategoryLayout)
    }

    fn library_group_mode(self, group_mode: LibraryGroupMode) -> &'static str {
        match group_mode {
            LibraryGroupMode::Category => self.get(TextKey::SettingsGeneralInstalledModsGroupCategory),
            LibraryGroupMode::Status => self.get(TextKey::SettingsGeneralInstalledModsGroupStatus),
            LibraryGroupMode::None => self.get(TextKey::SettingsGeneralInstalledModsGroupNone),
        }
    }

    fn library_category_display_mode(
        self,
        display_mode: LibraryCategoryDisplayMode,
    ) -> &'static str {
        match display_mode {
            LibraryCategoryDisplayMode::GroupedSections => {
                self.get(TextKey::SettingsGeneralInstalledModsLayoutList)
            }
            LibraryCategoryDisplayMode::Folders => {
                self.get(TextKey::SettingsGeneralInstalledModsLayoutFolders)
            }
        }
    }

    fn sort_by_category_first(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsSortByCategoryFirst)
    }

    fn sort_by_category_first_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsSortByCategoryFirstTooltip)
    }

    fn sort_by_status_first(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsSortByStatusFirst)
    }

    fn sort_by_status_first_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsSortByStatusFirstTooltip)
    }

    fn show_mod_status_on_card(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowModStatusOnCard)
    }

    fn show_category_on_card(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowCategoryOnCard)
    }

    fn show_category_on_card_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowCategoryOnCardTooltip)
    }

    fn show_disabled_mods(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowDisabledMods)
    }

    fn show_archived_mods(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowArchivedMods)
    }

    fn show_uncategorized_mods_first(self) -> &'static str {
        self.get(TextKey::SettingsGeneralInstalledModsShowUncategorizedModsFirst)
    }

    fn mods_to_check_for_updates(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalModsToCheckForUpdates)
    }

    fn automatically_update_mods(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalAutomaticallyUpdateMods)
    }

    fn status_target_active(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalStatusActive)
    }

    fn status_target_disabled(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalStatusDisabled)
    }

    fn status_target_archived(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalStatusArchived)
    }

    fn also_update_modified_mods(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalAlsoUpdateModifiedMods)
    }

    fn modified_update_behavior(self, behavior: ModifiedUpdateBehavior) -> &'static str {
        match behavior {
            ModifiedUpdateBehavior::Yes => self.get(TextKey::SettingsGeneralOperationalYes),
            ModifiedUpdateBehavior::ShowButton => {
                self.get(TextKey::SettingsGeneralOperationalNoButShowUpdateButton)
            }
            ModifiedUpdateBehavior::HideButton => {
                self.get(TextKey::SettingsGeneralOperationalNoAndHideUpdateButton)
            }
        }
    }

    fn when_installing_existing_mod(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalWhenInstallingExistingMod)
    }

    fn import_resolution(self, resolution: ImportResolution) -> &'static str {
        match resolution {
            ImportResolution::Ask => self.get(TextKey::SettingsGeneralOperationalAlwaysAsk),
            ImportResolution::Replace => self.get(TextKey::SettingsGeneralOperationalAlwaysReplace),
            ImportResolution::Merge => self.get(TextKey::SettingsGeneralOperationalAlwaysMerge),
            ImportResolution::KeepBoth => {
                self.get(TextKey::SettingsGeneralOperationalAlwaysKeepBoth)
            }
        }
    }

    fn always_replace_on_updating_mods(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods)
    }

    fn when_deleting_mod(self) -> &'static str {
        self.get(TextKey::SettingsGeneralOperationalWhenDeletingMod)
    }

    fn delete_behavior(self, behavior: DeleteBehavior) -> &'static str {
        match behavior {
            DeleteBehavior::RecycleBin => self.get(TextKey::SettingsGeneralOperationalMoveToRecycleBin),
            DeleteBehavior::Permanent => self.get(TextKey::SettingsGeneralOperationalDeletePermanently),
        }
    }

    fn tasks(self) -> &'static str {
        self.get(TextKey::SettingsGeneralTasksSection)
    }

    fn tasks_layout(self) -> &'static str {
        self.get(TextKey::SettingsGeneralTasksLayout)
    }

    fn tasks_layout_value(self, layout: TasksLayout) -> &'static str {
        match layout {
            TasksLayout::Sections => self.get(TextKey::SettingsGeneralTasksLayoutSections),
            TasksLayout::Tabbed => self.get(TextKey::SettingsGeneralTasksLayoutTabbed),
            TasksLayout::SingleList => self.get(TextKey::SettingsGeneralTasksLayoutSingleList),
        }
    }

    fn clear_completed_tasks(self) -> &'static str {
        self.get(TextKey::SettingsGeneralTasksClearCompletedTasks)
    }

    fn clear_tasks(self) -> &'static str {
        self.get(TextKey::SettingsGeneralTasksClearTasks)
    }

    fn task_order(self) -> &'static str {
        self.get(TextKey::SettingsGeneralTasksOrder)
    }

    fn tasks_order_value(self, order: TasksOrder) -> &'static str {
        match order {
            TasksOrder::OldestFirst => self.get(TextKey::SettingsGeneralTasksOldestToNewest),
            TasksOrder::NewestFirst => self.get(TextKey::SettingsGeneralTasksNewestToOldest),
        }
    }

    fn category_select_game(self) -> &'static str {
        self.get(TextKey::SettingsCategorySelectGame)
    }

    fn category_browse(self) -> &'static str {
        self.get(TextKey::SettingsCategoryBrowseSection)
    }

    fn auto_create_gamebanana_categories(self) -> &'static str {
        self.get(TextKey::SettingsCategoryAutoCreateGameBananaCategories)
    }

    fn applies_to_game(self, game_name: &str) -> String {
        self.get(TextKey::SettingsCategoryAppliesToGame)
            .replace("{game}", game_name)
    }

    fn categories(self) -> &'static str {
        self.get(TextKey::SettingsCategoryCategoriesSection)
    }

    fn select_all_categories(self) -> &'static str {
        self.get(TextKey::SettingsCategorySelectAllCategories)
    }

    fn unselect_all_categories(self) -> &'static str {
        self.get(TextKey::SettingsCategoryUnselectAllCategories)
    }

    fn new_category(self) -> &'static str {
        self.get(TextKey::SettingsCategoryNew)
    }

    fn new_category_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsCategoryNewTooltip)
    }

    fn delete(self) -> &'static str {
        self.get(TextKey::SettingsCategoryDelete)
    }

    fn uncategorized(self) -> &'static str {
        self.get(TextKey::SettingsCategoryUncategorized)
    }

    fn path_scan_title(self) -> &'static str {
        self.get(TextKey::SettingsPathScanTitle)
    }

    fn path_scan_description(self) -> &'static str {
        self.get(TextKey::SettingsPathScanDescription)
    }

    fn path_scan_button(self, scanning: bool) -> &'static str {
        if scanning {
            self.get(TextKey::SettingsPathScanButtonScanning)
        } else {
            self.get(TextKey::SettingsPathScanButtonScan)
        }
    }

    fn path_scan_button_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsPathScanButtonTooltip)
    }

    fn path_xxmi_section(self) -> &'static str {
        self.get(TextKey::SettingsPathXxmiSection)
    }

    fn path_xxmi_launcher(self) -> &'static str {
        self.get(TextKey::SettingsPathXxmiLauncher)
    }

    fn path_not_found(self) -> &'static str {
        self.get(TextKey::SettingsPathPathNotFound)
    }

    fn path_use_default_xxmi_mod_path(self) -> &'static str {
        self.get(TextKey::SettingsPathUseDefaultXxmiModPath)
    }

    fn path_game_section(self) -> &'static str {
        self.get(TextKey::SettingsPathGameSection)
    }

    fn path_game_exe_file(self) -> &'static str {
        self.get(TextKey::SettingsPathGameExeFile)
    }

    fn path_game_mods_folder(self, xxmi_code: &str) -> String {
        self.get(TextKey::SettingsPathGameModsFolder)
            .replace("{code}", xxmi_code)
    }

    fn when_launching_game(self) -> &'static str {
        self.get(TextKey::SettingsGeneralBehaviorWhenLaunchingGame)
    }

    fn after_installing_mod(self) -> &'static str {
        self.get(TextKey::SettingsGeneralBehaviorAfterInstallingMod)
    }

    fn when_launching_tool(self) -> &'static str {
        self.get(TextKey::SettingsGeneralBehaviorWhenLaunchingTool)
    }

    fn mod_detail_metadata(self) -> &'static str {
        self.get(TextKey::SettingsGeneralBehaviorModDetailMetadata)
    }

    fn launch_behavior(self, behavior: LaunchBehavior) -> &'static str {
        match behavior {
            LaunchBehavior::DoNothing => self.get(TextKey::SettingsGeneralBehaviorDoNothing),
            LaunchBehavior::Minimize => self.get(TextKey::SettingsGeneralBehaviorMinimizeHestia),
            LaunchBehavior::Exit => self.get(TextKey::SettingsGeneralBehaviorExitHestia),
        }
    }

    fn after_install_behavior(self, behavior: AfterInstallBehavior) -> &'static str {
        match behavior {
            AfterInstallBehavior::DoNothing => self.get(TextKey::SettingsGeneralBehaviorDoNothing),
            AfterInstallBehavior::AddToSelection => {
                self.get(TextKey::SettingsGeneralBehaviorAddToSelection)
            }
            AfterInstallBehavior::OpenModDetail => {
                self.get(TextKey::SettingsGeneralBehaviorOpenModDetail)
            }
        }
    }

    fn metadata_visibility(self, visibility: MetadataVisibility) -> &'static str {
        match visibility {
            MetadataVisibility::Never => self.get(TextKey::SettingsGeneralBehaviorNeverShow),
            MetadataVisibility::OnlyIfNoDescription => {
                self.get(TextKey::SettingsGeneralBehaviorShowIfNoDescription)
            }
            MetadataVisibility::Always => self.get(TextKey::SettingsGeneralBehaviorAlwaysShow),
        }
    }

    fn appearance(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceSection)
    }

    fn language(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceLanguage)
    }

    fn font_style(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceFontStyle)
    }

    fn font_classic(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceFontClassic)
    }

    fn font_modern(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceFontModern)
    }

    fn font_classic_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceFontClassicTooltip)
    }

    fn font_modern_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedAppearanceFontModernTooltip)
    }

    fn content_restriction(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedContentRestrictionSection)
    }

    fn hide_unsafe_contents(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedContentRestrictionHideUnsafeContents)
    }

    fn unsafe_content_mode(self, mode: UnsafeContentMode) -> &'static str {
        match mode {
            UnsafeContentMode::HideNoCounter => {
                self.get(TextKey::SettingsAdvancedContentRestrictionHideNsfwHideCounter)
            }
            UnsafeContentMode::HideShowCounter => {
                self.get(TextKey::SettingsAdvancedContentRestrictionHideNsfwShowCounter)
            }
            UnsafeContentMode::Censor => {
                self.get(TextKey::SettingsAdvancedContentRestrictionShowImagesCensored)
            }
            UnsafeContentMode::Show => {
                self.get(TextKey::SettingsAdvancedContentRestrictionShowUnrestricted)
            }
        }
    }

    fn cache_and_archive(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveSection)
    }

    fn cache_size(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveCacheSize)
    }

    fn current_usage(self, gb: f64) -> String {
        self.get(TextKey::SettingsAdvancedCacheArchiveCurrentUsage)
            .replace("{gb}", &format!("{gb:.2}"))
    }

    fn clear_cache(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveClearCache)
    }

    fn cache_cleared(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveCacheCleared)
    }

    fn clear_cache_failed(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveClearCacheFailed)
    }

    fn archive_usage(self, gb: f64) -> String {
        self.get(TextKey::SettingsAdvancedCacheArchiveArchiveUsage)
            .replace("{gb}", &format!("{gb:.2}"))
    }

    fn delete_archived_mods(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveDeleteArchivedMods)
    }

    fn archive_delete_action(self, behavior: DeleteBehavior) -> &'static str {
        match behavior {
            DeleteBehavior::RecycleBin => self.get(TextKey::SettingsAdvancedCacheArchiveRecycled),
            DeleteBehavior::Permanent => self.get(TextKey::SettingsAdvancedCacheArchiveDeleted),
        }
    }

    fn archived_mods_count(self, count: usize) -> String {
        self.get(TextKey::SettingsAdvancedCacheArchiveArchivedMods)
            .replace("{count}", &count.to_string())
    }

    fn archives_cleared(self, count: usize) -> String {
        self.get(TextKey::SettingsAdvancedCacheArchiveArchivesCleared)
            .replace("{count}", &count.to_string())
    }

    fn no_archives_to_clear(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveNoArchivesToClear)
    }

    fn clear_archives_failed(self) -> &'static str {
        self.get(TextKey::SettingsAdvancedCacheArchiveClearArchivesFailed)
    }

    fn about_by(self, authors: &str) -> String {
        self.get(TextKey::SettingsAboutBy)
            .replace("{authors}", authors)
    }

    fn about_version(self) -> &'static str {
        self.get(TextKey::SettingsAboutVersion)
    }

    fn about_version_tooltip(self) -> &'static str {
        self.get(TextKey::SettingsAboutVersionTooltip)
    }

    fn automatically_check_for_update(self) -> &'static str {
        self.get(TextKey::SettingsAboutAutomaticallyCheckForUpdate)
    }

    fn app_update_checking(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateChecking)
    }

    fn app_update_restart_to_update(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateRestartToUpdate)
    }

    fn app_update_check_for_update(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateCheckForUpdate)
    }

    fn app_update_up_to_date(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateUpToDate)
    }

    fn app_update_failed_to_check(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateFailedToCheck)
    }

    fn app_update_manual_required(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateManualRequired)
    }

    fn app_update_available(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateAvailable)
    }

    fn app_update_ready(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateReady)
    }

    fn app_update_failed(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateFailed)
    }

    fn app_update_download_canceled(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateDownloadCanceled)
    }

    fn app_update_wait_for_active_tasks(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateWaitForActiveTasks)
    }

    fn app_update_could_not_apply(self) -> &'static str {
        self.get(TextKey::SettingsAboutUpdateCouldNotApply)
    }

    fn app_update_manual_install_folder(self, path: &str) -> String {
        self.get(TextKey::SettingsAboutUpdateManualInstallFolder)
            .replace("{path}", path)
    }

    fn attribution(self) -> &'static str {
        self.get(TextKey::SettingsAboutAttributionSection)
    }

    fn attribution_gamebanana(self) -> &'static str {
        self.get(TextKey::SettingsAboutAttributionGameBanana)
    }

    fn translation_failed(self) -> &'static str {
        self.get(TextKey::TranslationFailed)
    }

    fn translation_in_progress(self) -> &'static str {
        self.get(TextKey::TranslationInProgress)
    }
}

impl HestiaApp {
    fn text(&self) -> TextCatalog {
        TextCatalog::new(self.state.static_prefs.language)
    }
}

#[cfg(test)]
mod tests {
    fn is_key_name(value: &str) -> bool {
        let mut chars = value.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn text_key_names() -> Vec<&'static str> {
        let source = include_str!("i18n.rs");
        let enum_start = source.find("enum TextKey {").expect("TextKey enum exists");
        let body_start = enum_start + "enum TextKey {".len();
        let body_end = source[body_start..]
            .find("\n}")
            .map(|offset| body_start + offset)
            .expect("TextKey enum closes");
        source[body_start..body_end]
            .lines()
            .filter_map(|line| {
                let name = line.trim().strip_suffix(',')?;
                is_key_name(name).then_some(name)
            })
            .collect()
    }

    fn catalog_comment_keys(source: &'static str) -> Vec<&'static str> {
        source
            .lines()
            .filter_map(|line| {
                let (_, comment) = line.rsplit_once("//")?;
                let key = comment.trim();
                is_key_name(key).then_some(key)
            })
            .collect()
    }

    #[test]
    fn language_catalog_comments_match_text_key_order() {
        let keys = text_key_names();
        let catalogs = [
            ("en_us", catalog_comment_keys(include_str!("i18n/en_us.rs"))),
            ("id_id", catalog_comment_keys(include_str!("i18n/id_id.rs"))),
            ("zh_cn", catalog_comment_keys(include_str!("i18n/zh_cn.rs"))),
        ];

        for (language, comments) in catalogs {
            assert_eq!(
                comments.len(),
                keys.len(),
                "{language} catalog key count changed"
            );
            for (index, (expected, actual)) in keys.iter().zip(comments.iter()).enumerate() {
                assert_eq!(
                    actual,
                    expected,
                    "{language} catalog key mismatch at index {index}"
                );
            }
        }
    }
}
