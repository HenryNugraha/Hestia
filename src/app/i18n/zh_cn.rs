const ZH_CN: [&str; TEXT_KEY_COUNT] = [
    // Window: What's New
    "新功能", // WhatsNewWindowTitle
    "点击显示反馈调查。", // WhatsNewFeedbackSurveyTooltip

    // Window: Feedback Survey
    "可选", // FeedbackSurveyOptional
    "正在提交…", // FeedbackSurveySubmitting
    "提交反馈", // FeedbackSurveySubmitFeedback
    "关闭", // FeedbackSurveyDismiss
    "稍后提醒我", // FeedbackSurveyRemindLater
    "跳过此版本", // FeedbackSurveySkipVersion
    "不再询问", // FeedbackSurveyNeverAskAgain
    "隐私详情", // FeedbackSurveyPrivacyDetails
    "反馈将匿名提交。\n无法识别或联系提交者。\n统计结果可能会公开，但具体留言内容会保持私密。\n只会向调查服务器发送以下数据：", // FeedbackSurveyPrivacyCopy
    "• 客户端：hestia.toml 文件中随机生成 UUID 的 SHA-256 哈希\n• 服务器和数据库 URL：{server_url}\n• 服务器地理位置：Asia Pacific", // FeedbackSurveyPrivacyPayload
    "在这里查看调查结果：", // FeedbackSurveyResultsHeader
    "• 进行中：", // FeedbackSurveyResultsOngoing
    "• 之前：", // FeedbackSurveyResultsPrevious

    // Window: Log
    "日志", // LogWindowTitle
    "日志已复制", // LogCopied

    // Window: Tasks
    "任务", // TasksWindowTitle
    "进行中", // TasksOngoing
    "进行中 ({count})", // TasksOngoingCount
    "没有活动任务", // TasksNoActiveTasks
    "已完成", // TasksCompleted
    "已完成 ({count})", // TasksCompletedCount
    "没有已完成任务", // TasksNoCompletedTasks
    "下载", // TasksDownloads
    "下载 ({count})", // TasksDownloadsCount
    "安装", // TasksInstalls
    "安装 ({count})", // TasksInstallsCount
    "失败", // TasksFailed
    "失败 ({count})", // TasksFailedCount
    "没有任务", // TasksNoTasks
    "已排队", // TasksStatusQueued
    "正在安装", // TasksStatusInstalling
    "正在下载", // TasksStatusDownloading
    "正在取消", // TasksStatusCanceling
    "已完成", // TasksStatusCompleted
    "失败", // TasksStatusFailed
    "已取消", // TasksStatusCanceled
    "正在取消…", // TasksCanceling
    "取消", // TasksCancel
    "重试", // TasksRetry
    "继续", // TasksResume
    "正在启动下载…", // TasksStartingDownload
    "已排队…", // TasksQueuedProgress
    "正在安装 Mod 文件…", // TasksInstallingModFiles
    "正在取消任务…", // TasksCancelingTask

    // Window: Tools
    "工具", // ToolsWindowTitle
    "为这款游戏添加外部工具快捷方式，然后从 Hestia 启动它们。可按需设置自定义启动选项，并将常用工具固定到标题栏。", // ToolsDescription
    "未选择游戏", // ToolsNoGameSelected
    "启动", // ToolsLaunch
    "设置启动选项", // ToolsSetLaunchOptions
    "打开文件夹", // ToolsOpenFolder
    "从标题栏取消固定", // ToolsUnpinFromTitlebar
    "固定到标题栏", // ToolsPinToTitlebar
    "移除", // ToolsRemove
    "添加工具", // ToolsAddTool
    "工具", // ToolsFallbackLabel
    "未选择要添加工具的游戏", // ToolsNoGameSelectedForAdd
    "工具已存在", // ToolsAlreadyAdded
    "工具已添加", // ToolsToolAdded
    "工具已移除", // ToolsToolRemoved
    "每个游戏最多只能在标题栏显示 4 个工具", // ToolsTitlebarLimit
    "已达到标题栏工具上限", // ToolsTitlebarLimitReached
    "工具可执行文件缺失", // ToolsExecutableMissing
    "未找到工具：{path}", // ToolsNotFound
    "已启动工具：{tool}", // ToolsLaunched
    "无法启动工具", // ToolsCouldNotLaunch
    "无法打开位置", // ToolsCouldNotOpenLocation
    "工具启动选项已保存", // ToolsLaunchOptionsSaved
    "工具已添加", // ToolsActionAdded
    "工具已移除", // ToolsActionRemoved
    "工具已启动", // ToolsActionLaunched

    // Window: Tool Launch Options
    "设置启动选项", // ToolLaunchOptionsWindowTitle
    "启动选项 (例如 -option value -flag)", // ToolLaunchOptionsHint
    "保存", // ToolLaunchOptionsSave
    "取消", // ToolLaunchOptionsCancel

    // Window: Dialogs
    "正在扫描路径…", // DialogScanningPaths
    "正在查找 XXMI 和游戏路径", // DialogFindingPaths
    "Hestia 正在深度扫描可访问的驱动器，以查找 XXMI 和支持的游戏。", // DialogDeepScanningPaths
    "扫描结果", // DialogScanResults
    "继续", // DialogContinue
    "停止扫描", // DialogStopScan
    "正在停止扫描…", // DialogStoppingScan
    "扫描已停止。", // DialogScanStopped
    "扫描已完成。", // DialogScanCompleted
    "已停止", // DialogStopped
    "未找到", // DialogNotFound
    "正在搜索…", // DialogSearching
    "已找到", // DialogFound
    "选择…", // DialogChoose
    "找到多个", // DialogMultipleFound
    "导入的 Mod", // DialogImportedMod
    "缺少 .ini", // DialogMissingIniTitle
    "在压缩包的父路径中没有找到可识别的 .ini 文件，压缩包可能包含多个 Mod。\n请选择要安装的文件夹：", // DialogMissingIniPrompt
    "安装", // DialogInstall
    "合并安装", // DialogInstallMerged
    "将所选文件夹安装到同一个 Mod 文件夹，并作为单个 Mod 处理", // DialogInstallMergedTooltip
    "分别安装", // DialogInstallSeparately
    "将所选文件夹安装到各自的 Mod 文件夹", // DialogInstallSeparatelyTooltip
    "安装失败", // DialogInstallFailed
    "安装不可用", // DialogInstallUnavailable
    "{name} 安装失败：{error}", // DialogInstallFailedFor
    "{name} 的安装检查失败：{error}", // DialogInstallInspectionFailed
    "无法提交 {name} 的安装任务", // DialogInstallDispatchFailed
    "无法开始安装 {name}：{error}", // DialogInstallStartFailed
    "请先选择游戏。", // DialogSelectGameFirst
    "未选择文件夹", // DialogNoFoldersSelected
    "安装已取消", // DialogInstallCanceled
    "安装已取消：{name}", // DialogInstallCanceledMessage
    "安装冲突", // DialogInstallationConflict
    "此文件夹", // DialogThisFolder
    "已存在于：", // DialogAlreadyExistsIn
    "替换", // DialogReplace
    "合并", // DialogMerge
    "保留两者", // DialogKeepBoth
    "冲突 (替换)", // DialogConflictReplace
    "冲突 (合并)", // DialogConflictMerge
    "冲突 (保留两者)", // DialogConflictKeepBoth
    "冲突 (取消)", // DialogConflictCancel
    "拖放 Mod 以安装\n\n或\n\n拖放图片以添加到：\n{name}", // DialogDropModsImages
    "拖放以安装", // DialogDropToInstall
    "不支持", // DialogUnsupported
    "不支持：{file}", // DialogUnsupportedFile
    "文件", // DialogFile
    "压缩包", // DialogFileFilterArchives
    "可执行文件", // DialogFileFilterExecutable
    "请先打开未链接 Mod 的详情", // DialogOpenUnlinkedModDetailFirst
    "正在安装：{count} 个 Mod", // DialogInstallingCount
    "无法创建 Mod 文件夹", // DialogCouldNotCreateModsFolder
    "无法禁用已安装的 Mod", // DialogCouldNotDisableInstalledMod
    "无法保持 Mod 为禁用状态", // DialogCouldNotKeepModDisabled
    "已安装", // DialogInstalledAction
    "已安装 {count} 个 Mod", // DialogInstalledCount
    "已安装：{name}", // DialogInstalledName
    "已同步", // DialogSyncedAction
    "更新不可用", // DialogUpdateUnavailable
    "正在更新：{title}", // DialogUpdatingTask

    // Main GUI: App Messages
    "无法保存设置", // AppCouldNotSaveSettings
    "无法保存数据", // AppCouldNotSaveData
    "警告：{detail}", // AppLogWarn
    "错误：{detail}", // AppLogError
    "启动失败", // AppLaunchFailed
    "启动路径未设置", // AppLaunchPathNotSet
    "未选择游戏", // AppGameNotSelected
    "启动 (Modded)", // AppPlayModded
    "启动 (Vanilla)", // AppPlayVanilla
    "Modded", // AppModded
    "Vanilla", // AppVanilla
    "{game} 的 {label} 路径未设置", // AppLaunchPathNotSetForGame
    "已启动 {game} ({mode})", // AppLaunchedGameMode
    "此版本未设置反馈调查。", // AppNoFeedbackSurveyConfigured
    "正在添加剪贴板图片…", // AppAddingClipboardImage
    "无法粘贴图片", // AppCouldNotPasteImage
    "无法附加图片", // AppCouldNotAttachImages
    "无法保存图片", // AppCouldNotSaveImages
    "图片已添加", // AppImagesAddedAction
    "已添加 {count} 张图片", // AppImagesAdded
    "无法添加图片", // AppCouldNotAddImages
    "查看预览", // AppWatchPreview
    "无法打开浏览器", // AppCouldNotOpenBrowser
    "无法刷新 Mod", // AppCouldNotRefreshMods
    "已扫描 {count} 个 Mod，没有变化", // AppModsScannedNoChanges
    "已重新加载：{count} 个 Mod，没有变化", // AppReloadedNoChanges
    "已扫描 {count} 个 Mod", // AppModsScanned
    "已重新加载：{count} 个 Mod", // AppReloaded
    "新增 {count} 个", // AppReloadAdded
    "移除 {count} 个", // AppReloadRemoved
    "变更 {count} 个", // AppReloadChanged
    "重新加载：{line}", // AppReloadAction
    "分类", // AppCategoryAction
    "已创建 \"{category}\"", // AppCategoryCreated
    "{mod} 没有有效的 GameBanana 分类；已跳过分类创建", // AppCategorySkippedNoValidGameBananaCategory
    "调查", // AppSurveyAction
    "已丢弃无法读取的待处理反馈数据：{error}", // AppSurveyDiscardedUnreadablePendingFeedbackPayload
    "正在重试待处理反馈数据", // AppSurveyRetryingPendingFeedbackPayload
    "已提交 {version} 的反馈", // AppSurveySubmittedFeedback
    "{version} 的反馈提交失败：{error}", // AppSurveyFeedbackSubmitFailed
    "已丢弃 {version} 的待处理反馈数据", // AppSurveyDiscardedPendingFeedbackPayload
    "无法提交反馈", // AppCouldNotSubmitFeedback
    "反馈已提交", // AppFeedbackSubmitted
    "下载已取消：{title}", // AppDownloadCanceled

    // Main GUI: Chrome
    "\nMod 管理器", // ChromeAppSubtitle
    "启动", // ChromePlay
    "安装\nZip/Rar", // ChromeInstallArchive
    "安装\n文件夹", // ChromeInstallFolder
    "重新加载", // ChromeReload
    "游戏未安装或未设置。", // ChromeGameNotInstalled
    "通过 XXMI 启动带 Mod 的游戏", // ChromeLaunchWithModsTooltip
    "启动不带 Mod 的游戏", // ChromeLaunchWithoutModsTooltip
    "带 Mod 启动", // ChromePlayWithMods
    "不带 Mod 启动", // ChromePlayWithoutMods
    "从 zip/rar/7z 压缩包安装 Mod", // ChromeInstallArchiveTooltip
    "从已解压的文件夹安装 Mod", // ChromeInstallFolderTooltip
    "安装", // ChromeInstall
    "安装并禁用", // ChromeInstallDisabled
    "重新扫描已安装 Mod 并在 GameBanana 检查更新 (Ctrl+R)", // ChromeReloadLibraryTooltip
    "重新加载当前列表 (Ctrl+R)", // ChromeReloadBrowseTooltip
    "关闭", // ChromeClose
    "还原", // ChromeRestore
    "最大化", // ChromeMaximize
    "最小化", // ChromeMinimize
    "我的 Mod", // ChromeMyMods
    "浏览", // ChromeBrowse
    "工具 (Ctrl+T)", // ChromeToolsTooltip
    "下载 (Ctrl+J)", // ChromeTasksTooltip
    "日志 (Ctrl+L)", // ChromeLogTooltip
    "设置 (F10)", // ChromeSettingsTooltip
    "未检测到或未启用游戏", // ChromeNoGamesDetected
    "查看“设置 → 游戏和路径”", // ChromeSeeSettingsGamePath

    // Main GUI: Browse
    "在 GameBanana 查找 Mod…", // BrowseSearchHint
    "GameBanana Mod", // BrowseModsTitle
    "角色", // BrowseCharacters
    "热门", // BrowsePopular
    "最近更新", // BrowseRecentUpdated
    "最佳匹配", // BrowseBestMatch
    "{count} 个 Mod", // BrowseModsCount
    "正在加载…", // BrowseLoading
    "因 NSFW 隐藏 {count} 个", // BrowseHiddenNsfwCount
    "{count} 个 Mod", // BrowseSelectedCharacterModsCount
    "显示所有 Mod", // BrowseShowAllMods
    "正在从 GameBanana 获取 Mod…", // BrowseFetchingMods
    "已安装", // BrowseInstalled
    "在浏览器中打开", // BrowseOpenInBrowser
    "无法打开浏览器", // BrowseCouldNotOpenBrowser
    "正在加载更多…", // BrowseLoadingMore
    "此游戏未设置角色列表。", // BrowseNoCharacterList
    "刷新角色", // BrowseRefreshCharacters
    "清除此筛选", // BrowseClearFilter
    "已选择：{name}", // BrowseSelectedCharacter
    "{count} 个角色", // BrowseCharacterCount
    "等待中", // BrowseWaiting
    "GameBanana 未返回任何角色。", // BrowseNoCharactersReturned
    "Mod 详情", // BrowseModDetail
    "复制 GameBanana ID", // BrowseCopyGameBananaId
    "GameBanana ID 已复制", // BrowseGameBananaIdCopied
    "未知", // BrowseUnknown
    "更新", // BrowseUpdates
    "此 Mod 是私有的。", // BrowsePrivateMod
    "自动安装已禁用。如果你有权限，可以直接在 GameBanana 查看或下载。", // BrowseAutomaticInstallDisabledAuthorized
    "此 Mod 已被暂扣", // BrowseWithheldMod
    "暂扣方", // BrowseWithheldBy
    "在暂扣解决之前，自动安装将保持禁用。", // BrowseAutomaticInstallDisabledWithheld
    "规则违规", // BrowseRuleViolation
    "此 Mod 已不存在。", // BrowseDeletedModNoLongerExists
    "此 Mod 已被删除，删除者为", // BrowseDeletedBy
    "此 Mod 已被删除", // BrowseDeleted
    "文件", // BrowseFiles
    "归档文件", // BrowseArchivedFiles
    "正在加载 Mod 详情…", // BrowseLoadingDetails
    "{size} • {date} • {downloads} 次下载", // BrowseFileMetadata
    "选择文件", // BrowseChooseFiles
    "此 Mod 有多个可用文件。\n请选择要下载并安装的文件：", // BrowseMultipleFilesPrompt
    "此游戏未设置 GameBanana 角色分类列表。", // BrowseNoConfiguredCharacterCategoryList
    "角色不可用", // BrowseCharactersUnavailable
    "连接失败", // BrowseConnectionFailed
    "连接超时", // BrowseConnectionTimedOut
    "浏览失败", // BrowseFailed
    "角色加载失败", // BrowseCharactersFailed
    "浏览详情失败", // BrowseDetailFailed
    "无法加载更新", // BrowseCouldNotLoadUpdates
    "已下载：{title}", // BrowseDownloaded
    "无法准备安装", // BrowseCouldNotPrepareInstall
    "下载失败", // BrowseDownloadFailed
    "正在准备下载：{title}", // BrowseResolvingDownload
    "未找到可下载文件", // BrowseNoDownloadableFilesFound
    "未选择文件", // BrowseNoFilesSelected
    "下载已排队", // BrowseDownloadQueued
    "浏览页面刷新失败；正在使用缓存结果：{warning}", // BrowsePageWarning
    "浏览页面失败：{error}", // BrowsePageFailed
    "角色分类刷新失败；正在使用缓存结果：{warning}", // BrowseCharacterCategoriesWarning
    "角色分类失败：{error}", // BrowseCharacterCategoriesFailed
    "Mod {mod_id} 的浏览详情刷新失败；正在使用缓存详情：{warning}", // BrowseDetailWarning
    "Mod {mod_id} 的浏览详情失败：{error}", // BrowseDetailFailedMessage
    "Mod {mod_id} 的浏览更新刷新失败；正在使用缓存更新：{warning}", // BrowseUpdatesWarning
    "Mod {mod_id} 的浏览更新失败：{error}", // BrowseUpdatesFailedMessage
    "{title} 下载失败：{error}", // BrowseDownloadFailedMessage

    // Main GUI: My Mods
    "正在扫描已安装 Mod", // LibraryScanningInstalledMods
    "安装 XXMI", // LibraryEnsureXxmiInstalled
    "需要 XXMI Launcher 才能管理受支持的游戏。", // LibraryInstallXxmiDescription
    "先设置 XXMI，再让 Hestia 查找您的游戏。", // LibrarySetupDescription
    "下载 XXMI", // LibraryDownloadXxmi
    "查找游戏并修复路径", // LibraryFindGamesAndFixPaths
    "扫描可访问的驱动器以查找 XXMI 和受支持游戏的安装位置。", // LibraryPathScanDescription
    "游戏和路径设置", // LibraryGamePathSettings
    "筛选 Mod 名称…", // LibrarySearchHint
    "已安装 Mod", // LibraryInstalledMods
    "{count} 个已选", // LibrarySelectedCount
    "选择所有可见 Mod", // LibrarySelectAllVisibleMods
    "{count} 个 Mod", // LibraryModsCount
    "1 个 Mod", // LibraryOneMod
    "返回", // LibraryBack
    "返回分类文件夹", // LibraryBackToCategoryFolders
    "{active} 个启用 • {disabled} 个禁用 • {archived} 个归档", // LibraryCategorySummary
    "名称 A-Z", // LibrarySortNameAsc
    "名称 Z-A", // LibrarySortNameDesc
    "最新 → 最旧", // LibrarySortDateDesc
    "最旧 → 最新", // LibrarySortDateAsc
    "排序、分组并设置已安装 Mod 的布局", // LibrarySortMenuTooltip
    "排序 Mod", // LibrarySortModsHeading
    "按 Mod 标题排序，没有标题时使用文件夹名。", // LibrarySortNameTooltip
    "使用已知最新的安装、内容或刷新时间。", // LibrarySortNewestTooltip
    "优先使用已知最旧的安装、内容或刷新时间。", // LibrarySortOldestTooltip
    "分组 Mod", // LibraryGroupModsHeading
    "按每个游戏的分类对 Mod 分组。", // LibraryGroupCategoryTooltip
    "将 Mod 分组到启用、禁用和已归档分区。", // LibraryGroupStatusTooltip
    "显示一个连续排序的 Mod 列表。", // LibraryGroupNoneTooltip
    "分类布局", // LibraryCategoryLayoutHeading
    "按分类分组时可用。", // LibraryAvailableWhenGroupedByCategory
    "先显示分类磁贴，然后一次打开一个分类。", // LibraryCategoryFoldersTooltip
    "将每个分类作为 Mod 列表中的一个分区显示。", // LibraryCategoryListTooltip
    "排序分类", // LibrarySortCategoriesHeading
    "手动", // LibraryCategorySortManual
    "按名称 (A-Z)", // LibraryCategorySortByNameAsc
    "Mod 最少", // LibraryCategorySortByLeastMods
    "Mod 最多", // LibraryCategorySortByMostMods
    "使用你的手动分类顺序。", // LibraryCategorySortManualTooltip
    "按分类名称排序分类文件夹和分区。", // LibraryCategorySortByNameTooltip
    "优先显示 Mod 数量最多的分类。", // LibraryCategorySortByMostModsTooltip
    "优先显示 Mod 数量最少的分类。", // LibraryCategorySortByLeastModsTooltip
    "其他", // LibraryMiscellaneousHeading
    "在状态分组内，先按分类顺序排列，再使用所选排序。", // LibrarySortCategoryFirstTooltip
    "在所选排序之前，先显示启用的 Mod，然后是禁用和已归档的 Mod。", // LibrarySortStatusFirstTooltip
    "在分类分组的列表布局中可用。", // LibraryUncategorizedFirstListOnlyTooltip
    "切换可见性", // LibraryToggleVisibility
    "Mod 状态", // LibraryModStateHeading
    "显示所有 Mod 状态", // LibraryShowAllModStates
    "隐藏所有 Mod 状态", // LibraryHideAllModStates
    "启用的 Mod", // LibraryEnabledMods
    "禁用的 Mod", // LibraryDisabledMods
    "已归档的 Mod", // LibraryArchivedMods
    "更新状态", // LibraryUpdateStateHeading
    "显示所有更新状态", // LibraryShowAllUpdateStates
    "隐藏所有更新状态", // LibraryHideAllUpdateStates
    "未链接", // LibraryUnlinked
    "已是最新", // LibraryUpToDate
    "有可用更新", // LibraryUpdateAvailable
    "检查已跳过", // LibraryCheckSkipped
    "来源缺失", // LibraryMissingSource
    "本地已修改", // LibraryModifiedLocally
    "正在忽略更新", // LibraryIgnoringUpdate
    "显示正在忽略当前更新或在关闭前一直忽略更新的 Mod。", // LibraryIgnoringUpdateTooltip
    "更新", // LibraryUpdate
    "启用", // LibraryEnable
    "禁用", // LibraryDisable
    "归档", // LibraryArchive
    "更多", // LibraryMore
    "(无)", // LibraryNone
    "还没有分类。\n\n1. 点击 Mod 卡片打开详情。\n2. 点击 Mod 名称下方的“未分类”。\n3. 点击“+ 新建分类”并命名。", // LibraryNoCategoryHelp
    "还没有分类。", // LibraryNoCategoryYet
    "新建分类", // LibraryNewCategory
    "打开", // LibraryOpen
    "文件资源管理器", // LibraryFileExplorer
    "此 Mod 未链接 GameBanana 来源。", // LibraryNoGameBananaSource
    "忽略一次更新", // LibraryIgnoreUpdateOnce
    "如果当前有可用更新，则忽略当前更新。如果尚无可用更新，则记住当前远程版本并忽略下一次检测到的更新。", // LibraryIgnoreUpdateOnceTooltip
    "使用忽略一次之前，请先将此 Mod 与 GameBanana 同步。", // LibraryIgnoreUpdateOnceDisabledTooltip
    "使用忽略一次之前，请先将至少一个已选择 Mod 与 GameBanana 同步。", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "始终忽略更新", // LibraryIgnoreUpdateAlways
    "将此 Mod 的更新状态无限期设为“始终忽略更新”，直到取消勾选。", // LibraryIgnoreUpdateAlwaysTooltip
    "已修改", // LibraryModified
    "\n(已修改)", // LibraryModifiedSuffix
    "…以及另外 {count} 个", // LibraryAndMore
    "已修改并忽略一次", // LibraryModifiedIgnoringOnce
    "已修改并始终忽略", // LibraryModifiedIgnoringAlways
    "已修改且有可用更新", // LibraryModifiedUpdateAvailable
    "忽略一次", // LibraryIgnoringOnce
    "始终忽略", // LibraryIgnoringAlways
    "缺失", // LibraryMissing
    "已跳过", // LibrarySkipped
    "空", // LibraryEmpty
    "正在移动", // LibraryMoving
    "移动到这里", // LibraryMoveHere
    "打开 {item}", // LibraryOpenItem
    "拖放到分类", // LibraryDropOnCategory
    "重新排序文件夹", // LibraryReorderFolder
    "分类", // LibraryCategoriesHeading
    "{folders} 个文件夹 / {uncategorized} 个未分类 Mod", // LibraryFoldersUncategorizedSummary
    "拖放后将切换为手动排序", // LibraryDropSwitchesToManualOrder
    "重命名", // LibraryRename
    "重命名 (F2)", // LibraryRenameShortcut
    "仅删除文件夹，将 Mod 移到外面", // LibraryFolderOnlyMoveModsOutside
    "删除文件夹及其中的 Mod", // LibraryFolderAndModsInside
    "已删除文件夹：{category}", // LibraryDeletedFolder
    "已启用", // LibraryStatusActive
    "禁用", // LibraryStatusDisabled
    "已归档", // LibraryStatusArchived
    "已移至回收站", // LibraryRecycledAction
    "已删除", // LibraryDeletedAction
    "删除失败", // LibraryDeleteFailed
    "禁用失败", // LibraryDisableFailed
    "归档失败", // LibraryArchiveFailed
    "启用失败", // LibraryEnableFailed
    "还原失败", // LibraryRestoreFailed
    "已禁用", // LibraryActionDisabled
    "已归档", // LibraryActionArchived
    "已启用", // LibraryActionEnabled
    "已取消归档", // LibraryActionUnarchived
    "{action}：{name}", // LibraryActionMessage
    "{action} {count} 个 Mod", // LibraryActionCountMessage
    "{action} {category} 及其中 {count} 个 Mod", // LibraryCategoryActionCountMessage
    "已为 {count} 个 Mod 排队更新", // LibraryQueuedUpdates
    "重命名失败", // LibraryRenameFailed
    "重命名", // LibraryActionRenamed
    "已重命名为：{name}", // LibraryRenamedTo
    "个人备注", // LibraryPersonalNote
    "个人备注已保存", // LibrarySavedPersonalNote
    "个人备注已移除", // LibraryPersonalNoteRemoved
    "无法保存个人备注", // LibraryCouldNotSavePersonalNote
    "移除图片", // LibraryRemoveImage
    "点击这里", // LibraryClickHereTo
    "手动添加图片。", // LibraryManuallyAddImages
    "你也可以将图片拖放到这里，", // LibraryDropImagesHere
    "或从剪贴板粘贴 (CTRL + V)。", // LibraryPasteFromClipboard
    "正在添加图片…", // LibraryAddingImages
    "添加图片", // LibraryAddImages
    "图片", // LibraryImagesFileDialog
    "正在添加 {count} 张图片", // LibraryAddingImagesCount
    "无法添加图片", // LibraryCouldNotAddImages
    "图片已移除", // LibraryImageRemoved
    "无法移除图片", // LibraryCouldNotRemoveImage
    "描述", // LibraryDescription
    "元数据", // LibraryMetadata
    "需要 RabbitFX", // LibraryRequiresRabbitFx
    "添加个人备注", // LibraryAddPersonalNote
    "保存个人备注", // LibrarySavePersonalNote
    "可编辑的用户备注", // LibraryEditableUserNote
    "编辑个人备注", // LibraryEditPersonalNote
    "+ 添加备注", // LibraryAddNote
    "本地", // LibraryLocal
    "在文件资源管理器中打开", // LibraryOpenInFileExplorer
    "来源", // LibrarySource
    "• 上次同步：{age}", // LibraryLastSynced
    "重新同步", // LibraryResync
    "取消链接", // LibraryUnlink
    "GameBanana 页面", // LibraryGameBananaPage
    "链接到 GameBanana 以启用更新跟踪和元数据同步。", // LibraryLinkGameBananaPrompt
    "URL 或 ID", // LibraryUrlOrId
    "同步 Mod", // LibrarySyncMod
    "更新偏好：", // LibraryUpdatePreferences
    "正在与 GameBanana 同步…", // LibrarySyncingGameBanana

    // Window: Settings
    "设置", // SettingsWindowTitle
    "常规", // SettingsTabGeneral
    "分类", // SettingsTabCategory
    "高级", // SettingsTabAdvanced
    "游戏和路径", // SettingsTabGamePath
    "关于", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "行为", // SettingsGeneralBehaviorSection
    "启动游戏时：", // SettingsGeneralBehaviorWhenLaunchingGame
    "安装 Mod 后：", // SettingsGeneralBehaviorAfterInstallingMod
    "启动工具时：", // SettingsGeneralBehaviorWhenLaunchingTool
    "Mod 详情元数据：", // SettingsGeneralBehaviorModDetailMetadata
    "不执行操作", // SettingsGeneralBehaviorDoNothing
    "最小化 Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "退出 Hestia", // SettingsGeneralBehaviorExitHestia
    "加入选中项", // SettingsGeneralBehaviorAddToSelection
    "打开 Mod 详情", // SettingsGeneralBehaviorOpenModDetail
    "从不显示", // SettingsGeneralBehaviorNeverShow
    "没有描述时显示", // SettingsGeneralBehaviorShowIfNoDescription
    "始终显示", // SettingsGeneralBehaviorAlwaysShow

    // Window: Settings > General > Installed Mods List
    "已安装 Mod 列表", // SettingsGeneralInstalledModsListSection
    "列表分组方式：", // SettingsGeneralInstalledModsGroupListBy
    "分类布局：", // SettingsGeneralInstalledModsCategoryLayout
    "分类", // SettingsGeneralInstalledModsGroupCategory
    "状态", // SettingsGeneralInstalledModsGroupStatus
    "无", // SettingsGeneralInstalledModsGroupNone
    "列表", // SettingsGeneralInstalledModsLayoutList
    "文件夹", // SettingsGeneralInstalledModsLayoutFolders
    "优先按分类排序", // SettingsGeneralInstalledModsSortByCategoryFirst
    "按分类的预设顺序排序（不一定是字母顺序）。", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
    "优先按状态排序", // SettingsGeneralInstalledModsSortByStatusFirst
    "先显示启用的 Mod，然后是禁用和已归档的 Mod。", // SettingsGeneralInstalledModsSortByStatusFirstTooltip
    "在卡片上显示 Mod 状态", // SettingsGeneralInstalledModsShowModStatusOnCard
    "在卡片上显示分类", // SettingsGeneralInstalledModsShowCategoryOnCard
    "Mod 状态仍会通过彩色状态点显示。", // SettingsGeneralInstalledModsShowCategoryOnCardTooltip
    "显示禁用的 Mod", // SettingsGeneralInstalledModsShowDisabledMods
    "显示已归档的 Mod", // SettingsGeneralInstalledModsShowArchivedMods
    "优先显示未分类的 Mod", // SettingsGeneralInstalledModsShowUncategorizedModsFirst

    // Window: Settings > General > Operational
    "操作", // SettingsGeneralOperationalSection
    "要检查更新的 Mod：", // SettingsGeneralOperationalModsToCheckForUpdates
    "自动更新 Mod：", // SettingsGeneralOperationalAutomaticallyUpdateMods
    "启用", // SettingsGeneralOperationalStatusActive
    "禁用", // SettingsGeneralOperationalStatusDisabled
    "已归档", // SettingsGeneralOperationalStatusArchived
    "同时更新已被修改的 Mod：", // SettingsGeneralOperationalAlsoUpdateModifiedMods
    "是", // SettingsGeneralOperationalYes
    "否，但显示“更新”按钮", // SettingsGeneralOperationalNoButShowUpdateButton
    "否，并隐藏“更新”按钮", // SettingsGeneralOperationalNoAndHideUpdateButton
    "安装已存在的 Mod 时：", // SettingsGeneralOperationalWhenInstallingExistingMod
    "始终询问", // SettingsGeneralOperationalAlwaysAsk
    "始终替换", // SettingsGeneralOperationalAlwaysReplace
    "始终合并", // SettingsGeneralOperationalAlwaysMerge
    "始终保留两者", // SettingsGeneralOperationalAlwaysKeepBoth
    "更新 Mod 时始终替换", // SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods
    "删除 Mod 时：", // SettingsGeneralOperationalWhenDeletingMod
    "移至回收站", // SettingsGeneralOperationalMoveToRecycleBin
    "永久删除", // SettingsGeneralOperationalDeletePermanently

    // Window: Settings > General > Tasks
    "任务", // SettingsGeneralTasksSection
    "显示方式：", // SettingsGeneralTasksLayout
    "分区", // SettingsGeneralTasksLayoutSections
    "标签页", // SettingsGeneralTasksLayoutTabbed
    "单一列表", // SettingsGeneralTasksLayoutSingleList
    "清除已完成任务：", // SettingsGeneralTasksClearCompletedTasks
    "清除", // SettingsGeneralTasksClearTasks
    "顺序：", // SettingsGeneralTasksOrder
    "最旧 → 最新", // SettingsGeneralTasksOldestToNewest
    "最新 → 最旧", // SettingsGeneralTasksNewestToOldest

    // Window: Settings > Category
    "请选择一个游戏来设置分类。", // SettingsCategorySelectGame
    "浏览", // SettingsCategoryBrowseSection
    "为下载的 Mod 自动创建 GameBanana 分类", // SettingsCategoryAutoCreateGameBananaCategories
    "适用于 {game}。", // SettingsCategoryAppliesToGame
    "分类", // SettingsCategoryCategoriesSection
    "选择所有分类", // SettingsCategorySelectAllCategories
    "取消选择所有分类", // SettingsCategoryUnselectAllCategories
    "新建", // SettingsCategoryNew
    "新建分类 (Ctrl+N)", // SettingsCategoryNewTooltip
    "删除", // SettingsCategoryDelete
    "未分类", // SettingsCategoryUncategorized

    // Window: Settings > Game & Path
    "路径有问题？", // SettingsPathScanTitle
    "Hestia 可以执行深度扫描来检测 XXMI 和受支持游戏的路径", // SettingsPathScanDescription
    "扫描路径", // SettingsPathScanButtonScan
    "正在扫描…", // SettingsPathScanButtonScanning
    "扫描可访问的驱动器以查找 XXMI 和游戏可执行文件。", // SettingsPathScanButtonTooltip
    "XXMI", // SettingsPathXxmiSection
    "XXMI 启动器：", // SettingsPathXxmiLauncher
    "路径未找到", // SettingsPathPathNotFound
    "为游戏使用默认 XXMI Mod 路径", // SettingsPathUseDefaultXxmiModPath
    "游戏", // SettingsPathGameSection
    "游戏 EXE 文件：", // SettingsPathGameExeFile
    "{code} Mod 文件夹：", // SettingsPathGameModsFolder

    // Window: Settings > Advanced > Appearance
    "外观", // SettingsAdvancedAppearanceSection
    "语言：", // SettingsAdvancedAppearanceLanguage
    "字体样式：", // SettingsAdvancedAppearanceFontStyle
    "经典", // SettingsAdvancedAppearanceFontClassic
    "现代", // SettingsAdvancedAppearanceFontModern
    "使用 Segoe UI 字体", // SettingsAdvancedAppearanceFontClassicTooltip
    "使用 Selawik 字体", // SettingsAdvancedAppearanceFontModernTooltip
    "始终翻译 Mod 详情", // SettingsAdvancedAppearanceAlwaysTranslateModDetails
    "启用后，查看 Mod 详情时会自动将标题和描述翻译为所选语言。", // SettingsAdvancedAppearanceAlwaysTranslateModDetailsTooltip

    // Window: Settings > Advanced > Content Restriction
    "内容限制", // SettingsAdvancedContentRestrictionSection
    "隐藏不安全内容：", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "隐藏 NSFW Mod，并隐藏计数", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "隐藏 NSFW Mod，并显示计数", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "显示但遮蔽图片", // SettingsAdvancedContentRestrictionShowImagesCensored
    "不限制显示", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Cache and Archive
    "缓存和归档", // SettingsAdvancedCacheArchiveSection
    "缓存大小：", // SettingsAdvancedCacheArchiveCacheSize
    "当前使用量：{gb} GB", // SettingsAdvancedCacheArchiveCurrentUsage
    "清除缓存", // SettingsAdvancedCacheArchiveClearCache
    "缓存已清除", // SettingsAdvancedCacheArchiveCacheCleared
    "无法清除缓存", // SettingsAdvancedCacheArchiveClearCacheFailed
    "归档使用量：{gb} GB", // SettingsAdvancedCacheArchiveArchiveUsage
    "删除已归档的 Mod", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "已移至回收站", // SettingsAdvancedCacheArchiveRecycled
    "已删除", // SettingsAdvancedCacheArchiveDeleted
    "{count} 个已归档的 Mod", // SettingsAdvancedCacheArchiveArchivedMods
    "归档已清除：{count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "没有可清除的归档", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "无法清除归档", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "作者：{authors}", // SettingsAboutBy
    "版本：", // SettingsAboutVersion
    "点击显示新功能。", // SettingsAboutVersionTooltip
    "自动检查更新", // SettingsAboutAutomaticallyCheckForUpdate
    "正在检查…", // SettingsAboutUpdateChecking
    "重启以更新", // SettingsAboutUpdateRestartToUpdate
    "检查更新", // SettingsAboutUpdateCheckForUpdate
    "已是最新", // SettingsAboutUpdateUpToDate
    "检查失败", // SettingsAboutUpdateFailedToCheck
    "需要手动更新", // SettingsAboutUpdateManualRequired
    "有可用更新", // SettingsAboutUpdateAvailable
    "更新已准备好", // SettingsAboutUpdateReady
    "更新失败", // SettingsAboutUpdateFailed
    "更新下载已取消", // SettingsAboutUpdateDownloadCanceled
    "请等待活动任务完成后再更新", // SettingsAboutUpdateWaitForActiveTasks
    "无法应用更新", // SettingsAboutUpdateCouldNotApply
    "Hestia 安装在当前进程无法更新的文件夹中：\n{path}\n请将 Hestia 移动到其他文件夹后重试，或从权限更高的进程更新此安装。", // SettingsAboutUpdateManualInstallFolder
    "来源说明", // SettingsAboutAttributionSection
    "数据来源：GameBanana，API 经授权使用。GameBanana 的 Mod 元数据、媒体和浏览数据均来自 GameBanana。", // SettingsAboutAttributionGameBanana

    // Translation strings
    "翻译 (F7)", // TranslationToggleShortcut
    "重新翻译", // TranslationRetranslate
    "翻译失败", // TranslationFailed
    "正在翻译", // TranslationInProgress
];
