const RU_RU: [&str; TEXT_KEY_COUNT] = [
    // Window: What's New
    "Что нового", // WhatsNewWindowTitle
    "Нажмите, чтобы открыть опрос обратной связи.", // WhatsNewFeedbackSurveyTooltip

    // Window: Feedback Survey
    "Необязательно", // FeedbackSurveyOptional
    "Отправка…", // FeedbackSurveySubmitting
    "Отправить отзыв", // FeedbackSurveySubmitFeedback
    "Закрыть", // FeedbackSurveyDismiss
    "Напомнить позже", // FeedbackSurveyRemindLater
    "Пропустить эту версию", // FeedbackSurveySkipVersion
    "Больше не спрашивать", // FeedbackSurveyNeverAskAgain
    "Сведения о конфиденциальности", // FeedbackSurveyPrivacyDetails
    "Отзыв отправляется анонимно.\nНевозможно идентифицировать отправителей или связаться с ними.\nРезультаты голосования могут быть опубликованы, но сообщения остаются приватными.\nНа сервер опроса будет отправлен только следующий набор данных:", // FeedbackSurveyPrivacyCopy
    "• Клиент: SHA-256-хеш случайно созданного UUID из файла hestia.toml\n• URL сервера и базы данных: {server_url}\n• Геолокация сервера: Азиатско-Тихоокеанский регион", // FeedbackSurveyPrivacyPayload
    "Результаты опроса можно посмотреть здесь:", // FeedbackSurveyResultsHeader
    "• Текущий: ", // FeedbackSurveyResultsOngoing
    "• Предыдущий: ", // FeedbackSurveyResultsPrevious

    // Window: Log
    "Журнал", // LogWindowTitle
    "Журнал скопирован", // LogCopied

    // Window: Tasks
    "Задачи", // TasksWindowTitle
    "Активные", // TasksOngoing
    "Активные ({count})", // TasksOngoingCount
    "Нет активных задач", // TasksNoActiveTasks
    "Завершено", // TasksCompleted
    "Завершено ({count})", // TasksCompletedCount
    "Нет завершённых задач", // TasksNoCompletedTasks
    "Загрузки", // TasksDownloads
    "Загрузки ({count})", // TasksDownloadsCount
    "Установки", // TasksInstalls
    "Установки ({count})", // TasksInstallsCount
    "С ошибкой", // TasksFailed
    "С ошибкой ({count})", // TasksFailedCount
    "Нет задач", // TasksNoTasks
    "В очереди", // TasksStatusQueued
    "Устанавливается", // TasksStatusInstalling
    "Загружается", // TasksStatusDownloading
    "Отменяется", // TasksStatusCanceling
    "Завершено", // TasksStatusCompleted
    "Ошибка", // TasksStatusFailed
    "Отменено", // TasksStatusCanceled
    "Отменяется…", // TasksCanceling
    "Отменить", // TasksCancel
    "Повторить", // TasksRetry
    "Возобновить", // TasksResume
    "Запуск загрузки…", // TasksStartingDownload
    "В очереди…", // TasksQueuedProgress
    "Установка файлов модов…", // TasksInstallingModFiles
    "Отмена задачи…", // TasksCancelingTask

    // Window: Tools
    "Утилиты", // ToolsWindowTitle
    "Добавляйте ярлыки внешних утилит для этой игры и запускайте их из Hestia. При необходимости настройте параметры запуска и закрепите часто используемые утилиты в строке заголовка.", // ToolsDescription
    "Игра не выбрана", // ToolsNoGameSelected
    "Запустить", // ToolsLaunch
    "Задать параметры запуска", // ToolsSetLaunchOptions
    "Открыть папку", // ToolsOpenFolder
    "Открепить от строки заголовка", // ToolsUnpinFromTitlebar
    "Закрепить в строке заголовка", // ToolsPinToTitlebar
    "Удалить", // ToolsRemove
    "Добавить утилиту", // ToolsAddTool
    "Утилита", // ToolsFallbackLabel
    "Игра для добавления утилиты не выбрана", // ToolsNoGameSelectedForAdd
    "Утилита уже добавлена", // ToolsAlreadyAdded
    "Утилита добавлена", // ToolsToolAdded
    "Утилита удалена", // ToolsToolRemoved
    "Для одной игры в строке заголовка можно показать не более 4 утилит", // ToolsTitlebarLimit
    "Достигнут лимит утилит строки заголовка", // ToolsTitlebarLimitReached
    "Файл утилиты отсутствует", // ToolsExecutableMissing
    "Утилита не найдена: {path}", // ToolsNotFound
    "Запущена утилита: {tool}", // ToolsLaunched
    "Не удалось запустить утилиту", // ToolsCouldNotLaunch
    "Не удалось открыть расположение", // ToolsCouldNotOpenLocation
    "Параметры запуска утилиты сохранены", // ToolsLaunchOptionsSaved
    "Утилита добавлена", // ToolsActionAdded
    "Утилита удалена", // ToolsActionRemoved
    "Утилита запущена", // ToolsActionLaunched

    // Window: Tool Launch Options
    "Задать параметры запуска", // ToolLaunchOptionsWindowTitle
    "Параметры запуска (например, -option value -flag)", // ToolLaunchOptionsHint
    "Сохранить", // ToolLaunchOptionsSave
    "Отмена", // ToolLaunchOptionsCancel

    // Window: Dialogs
    "Сканирование путей…", // DialogScanningPaths
    "Поиск XXMI и путей к играм", // DialogFindingPaths
    "Hestia выполняет глубокое сканирование доступных дисков для поиска XXMI и поддерживаемых игр.", // DialogDeepScanningPaths
    "Результаты сканирования", // DialogScanResults
    "Продолжить", // DialogContinue
    "Остановить сканирование", // DialogStopScan
    "Остановка сканирования…", // DialogStoppingScan
    "Сканирование остановлено.", // DialogScanStopped
    "Сканирование завершено.", // DialogScanCompleted
    "Остановлено", // DialogStopped
    "Не найдено", // DialogNotFound
    "Поиск…", // DialogSearching
    "Найдено", // DialogFound
    "Выбрать…", // DialogChoose
    "Найдено несколько", // DialogMultipleFound
    "Импортированный мод", // DialogImportedMod
    "Отсутствует .ini", // DialogMissingIniTitle
    "В родительской папке архива не найден распознаваемый .ini-файл; возможно, архив содержит несколько модов.\nВыберите папки для установки:", // DialogMissingIniPrompt
    "Установить", // DialogInstall
    "Установить вместе", // DialogInstallMerged
    "Установить выбранные папки в одну папку мода и считать их одним модом", // DialogInstallMergedTooltip
    "Установить отдельно", // DialogInstallSeparately
    "Установить выбранные папки в отдельные папки модов", // DialogInstallSeparatelyTooltip
    "Установка не удалась", // DialogInstallFailed
    "Установка недоступна", // DialogInstallUnavailable
    "Не удалось установить {name}: {error}", // DialogInstallFailedFor
    "Не удалось проверить установку {name}: {error}", // DialogInstallInspectionFailed
    "Не удалось отправить установку {name}", // DialogInstallDispatchFailed
    "Не удалось начать установку для {name}: {error}", // DialogInstallStartFailed
    "Сначала выберите игру.", // DialogSelectGameFirst
    "Папки не выбраны", // DialogNoFoldersSelected
    "Установка отменена", // DialogInstallCanceled
    "Установка отменена: {name}", // DialogInstallCanceledMessage
    "Конфликт установки", // DialogInstallationConflict
    "эта папка", // DialogThisFolder
    "Уже существует в:", // DialogAlreadyExistsIn
    "Заменить", // DialogReplace
    "Объединить", // DialogMerge
    "Оставить оба", // DialogKeepBoth
    "Конфликт (замена)", // DialogConflictReplace
    "Конфликт (объединение)", // DialogConflictMerge
    "Конфликт (оставить оба)", // DialogConflictKeepBoth
    "Конфликт (отмена)", // DialogConflictCancel
    "Перетащите моды, чтобы установить их\n\nили\n\nперетащите изображения, чтобы добавить их в:\n{name}", // DialogDropModsImages
    "Перетащите для установки", // DialogDropToInstall
    "Не поддерживается", // DialogUnsupported
    "Не поддерживается: {file}", // DialogUnsupportedFile
    "файл", // DialogFile
    "Архивы", // DialogFileFilterArchives
    "Исполняемый файл", // DialogFileFilterExecutable
    "Сначала откройте сведения о непривязанном моде", // DialogOpenUnlinkedModDetailFirst
    "Установка: {count} модов", // DialogInstallingCount
    "Не удалось создать папку модов", // DialogCouldNotCreateModsFolder
    "Не удалось отключить установленный мод", // DialogCouldNotDisableInstalledMod
    "Не удалось оставить мод отключённым", // DialogCouldNotKeepModDisabled
    "Установлено", // DialogInstalledAction
    "Установлено: {count} модов", // DialogInstalledCount
    "Установлено: {name}", // DialogInstalledName
    "Синхронизировано", // DialogSyncedAction
    "Обновление недоступно", // DialogUpdateUnavailable
    "Обновление: {title}", // DialogUpdatingTask

    // Main GUI: App Messages
    "Не удалось сохранить настройки", // AppCouldNotSaveSettings
    "Не удалось сохранить данные", // AppCouldNotSaveData
    "Предупреждение: {detail}", // AppLogWarn
    "Ошибка: {detail}", // AppLogError
    "Запуск не удался", // AppLaunchFailed
    "Путь запуска не задан", // AppLaunchPathNotSet
    "Игра не выбрана", // AppGameNotSelected
    "Играть (с модами)", // AppPlayModded
    "Играть (без модов)", // AppPlayVanilla
    "С модами", // AppModded
    "Без модов", // AppVanilla
    "Путь {label} не задан для {game}", // AppLaunchPathNotSetForGame
    "Запущено {game}", // AppLaunchedGame
    "Запущено {game} ({mode})", // AppLaunchedGameMode
    "Для этой версии не настроен опрос обратной связи.", // AppNoFeedbackSurveyConfigured
    "Добавление изображения из буфера обмена…", // AppAddingClipboardImage
    "Не удалось вставить изображение", // AppCouldNotPasteImage
    "Не удалось прикрепить изображения", // AppCouldNotAttachImages
    "Не удалось сохранить изображения", // AppCouldNotSaveImages
    "Изображения добавлены", // AppImagesAddedAction
    "Добавлено изображений: {count}", // AppImagesAdded
    "Не удалось добавить изображения", // AppCouldNotAddImages
    "Смотреть превью", // AppWatchPreview
    "Не удалось открыть браузер", // AppCouldNotOpenBrowser
    "Не удалось обновить моды", // AppCouldNotRefreshMods
    "Просканировано модов: {count}, изменений нет", // AppModsScannedNoChanges
    "Перезагружено: {count} модов, изменений нет", // AppReloadedNoChanges
    "Просканировано модов: {count}", // AppModsScanned
    "Перезагружено: {count} модов", // AppReloaded
    "Добавлено: {count}", // AppReloadAdded
    "Удалено: {count}", // AppReloadRemoved
    "Изменено: {count}", // AppReloadChanged
    "Перезагрузка: {line}", // AppReloadAction
    "Категория", // AppCategoryAction
    "Создано: \"{category}\"", // AppCategoryCreated
    "{mod} не имеет допустимой категории GameBanana; создание категории пропущено", // AppCategorySkippedNoValidGameBananaCategory
    "Опрос", // AppSurveyAction
    "Удалён нечитаемый ожидающий пакет данных отзыва: {error}", // AppSurveyDiscardedUnreadablePendingFeedbackPayload
    "Повторная отправка ожидающего пакета данных отзыва", // AppSurveyRetryingPendingFeedbackPayload
    "Отзыв для {version} отправлен", // AppSurveySubmittedFeedback
    "Ошибка отправки отзыва для {version}: {error}", // AppSurveyFeedbackSubmitFailed
    "Удалён ожидающий пакет данных отзыва для {version}", // AppSurveyDiscardedPendingFeedbackPayload
    "Не удалось отправить отзыв", // AppCouldNotSubmitFeedback
    "Отзыв отправлен", // AppFeedbackSubmitted
    "Загрузка отменена: {title}", // AppDownloadCanceled

    // Main GUI: Chrome
    "\nМенеджер модов", // ChromeAppSubtitle
    "Играть", // ChromePlay
    "Из\nZip/Rar", // ChromeInstallArchive
    "Из\nпапки", // ChromeInstallFolder
    "Обновить", // ChromeReload
    "Игра не установлена или не настроена.", // ChromeGameNotInstalled
    "Запустить игру с модами через XXMI", // ChromeLaunchWithModsTooltip
    "Запустить игру без модов", // ChromeLaunchWithoutModsTooltip
    "Играть с модами", // ChromePlayWithMods
    "Играть без модов", // ChromePlayWithoutMods
    "Установить мод из архива zip/rar/7z", // ChromeInstallArchiveTooltip
    "Установить мод из уже распакованной папки", // ChromeInstallFolderTooltip
    "Поставить", // ChromeInstall
    "Поставить выкл.", // ChromeInstallDisabled
    "Повторно просканировать установленные моды и проверить обновления на GameBanana (Ctrl+R)", // ChromeReloadLibraryTooltip
    "Обновить текущий список (Ctrl+R)", // ChromeReloadBrowseTooltip
    "Закрыть", // ChromeClose
    "Восстановить", // ChromeRestore
    "Развернуть", // ChromeMaximize
    "Свернуть", // ChromeMinimize
    "Моды", // ChromeMyMods
    "Каталог", // ChromeBrowse
    "Утилиты (Ctrl+T)", // ChromeToolsTooltip
    "Задачи (Ctrl+J)", // ChromeTasksTooltip
    "Журнал (Ctrl+L)", // ChromeLogTooltip
    "Настройки (F10)", // ChromeSettingsTooltip
    "Игры не обнаружены или не включены", // ChromeNoGamesDetected
    "См. «Настройки → Игра и путь»", // ChromeSeeSettingsGamePath

    // Main GUI: Browse
    "Найти моды на GameBanana…", // BrowseSearchHint
    "Моды GameBanana", // BrowseModsTitle
    "Персонажи", // BrowseCharacters
    "Популярное", // BrowsePopular
    "Недавно обновлено", // BrowseRecentUpdated
    "Лучшее совпадение", // BrowseBestMatch
    "Модов: {count}", // BrowseModsCount
    "Загрузка…", // BrowseLoading
    "Скрыто из-за NSFW: {count}", // BrowseHiddenNsfwCount
    "Модов: {count}", // BrowseSelectedCharacterModsCount
    "Показать все моды", // BrowseShowAllMods
    "Получение модов с GameBanana…", // BrowseFetchingMods
    "Есть", // BrowseInstalled
    "На страницу", // BrowseOpenInBrowser
    "Не удалось открыть браузер", // BrowseCouldNotOpenBrowser
    "Загрузка дополнительных данных…", // BrowseLoadingMore
    "Для этой игры не настроен список персонажей.", // BrowseNoCharacterList
    "Обновить персонажей", // BrowseRefreshCharacters
    "Очистить этот фильтр", // BrowseClearFilter
    "Выбрано: {name}", // BrowseSelectedCharacter
    "Персонажей: {count}", // BrowseCharacterCount
    "Ожидание", // BrowseWaiting
    "GameBanana не вернул список персонажей.", // BrowseNoCharactersReturned
    "Сведения о моде", // BrowseModDetail
    "Скопировать ID GameBanana", // BrowseCopyGameBananaId
    "ID GameBanana скопирован", // BrowseGameBananaIdCopied
    "Неизвестно", // BrowseUnknown
    "Обновления", // BrowseUpdates
    "Этот мод закрытый.", // BrowsePrivateMod
    "Автоматическая установка отключена. Если у вас есть доступ, можно просмотреть или скачать мод напрямую на GameBanana.", // BrowseAutomaticInstallDisabledAuthorized
    "Этот мод временно скрыт", // BrowseWithheldMod
    "Скрыт пользователем", // BrowseWithheldBy
    "Автоматическая установка отключена, пока скрытие не будет снято.", // BrowseAutomaticInstallDisabledWithheld
    "Нарушение правил", // BrowseRuleViolation
    "Этого мода больше не существует.", // BrowseDeletedModNoLongerExists
    "Этот мод был удалён пользователем", // BrowseDeletedBy
    "Этот мод удалён", // BrowseDeleted
    "Файлы", // BrowseFiles
    "Архивные файлы", // BrowseArchivedFiles
    "Загрузка сведений о моде…", // BrowseLoadingDetails
    "{size} • {date} • {downloads} загрузок", // BrowseFileMetadata
    "Выбрать файлы", // BrowseChooseFiles
    "У этого мода доступно несколько файлов.\nВыберите файлы для загрузки и установки:", // BrowseMultipleFilesPrompt
    "Для этой игры не настроен список категорий персонажей GameBanana.", // BrowseNoConfiguredCharacterCategoryList
    "Персонажи недоступны", // BrowseCharactersUnavailable
    "Ошибка подключения", // BrowseConnectionFailed
    "Время ожидания подключения истекло", // BrowseConnectionTimedOut
    "Ошибка загрузки каталога", // BrowseFailed
    "Ошибка загрузки персонажей", // BrowseCharactersFailed
    "Ошибка загрузки сведений", // BrowseDetailFailed
    "Не удалось загрузить обновления", // BrowseCouldNotLoadUpdates
    "Загружено: {title}", // BrowseDownloaded
    "Не удалось подготовить установку", // BrowseCouldNotPrepareInstall
    "Ошибка загрузки", // BrowseDownloadFailed
    "Подготовка загрузки: {title}", // BrowseResolvingDownload
    "Файлы для загрузки не найдены", // BrowseNoDownloadableFilesFound
    "Файлы не выбраны", // BrowseNoFilesSelected
    "Загрузка добавлена в очередь", // BrowseDownloadQueued
    "Не удалось обновить страницу каталога; используются кэшированные результаты: {warning}", // BrowsePageWarning
    "Ошибка страницы каталога: {error}", // BrowsePageFailed
    "Ошибка обновления категорий персонажей; используются кэшированные результаты: {warning}", // BrowseCharacterCategoriesWarning
    "Ошибка категорий персонажей: {error}", // BrowseCharacterCategoriesFailed
    "Не удалось обновить сведения о моде {mod_id}; используются кэшированные сведения: {warning}", // BrowseDetailWarning
    "Не удалось загрузить сведения о моде {mod_id}: {error}", // BrowseDetailFailedMessage
    "Не удалось обновить данные об обновлениях для мода {mod_id}; используются кэшированные данные: {warning}", // BrowseUpdatesWarning
    "Не удалось загрузить данные об обновлениях для мода {mod_id}: {error}", // BrowseUpdatesFailedMessage
    "Не удалось загрузить {title}: {error}", // BrowseDownloadFailedMessage

    // Main GUI: My Mods
    "Сканирование установленных модов", // LibraryScanningInstalledMods
    "Установите XXMI", // LibraryEnsureXxmiInstalled
    "XXMI Launcher нужен для управления играми.", // LibraryInstallXxmiDescription
    "Настройте XXMI, затем позвольте Hestia найти ваши игры.", // LibrarySetupDescription
    "Скачать XXMI", // LibraryDownloadXxmi
    "Найти игры и исправить пути", // LibraryFindGamesAndFixPaths
    "Найдите XXMI и поддерживаемые игры на дисках.", // LibraryPathScanDescription
    "Настройки игры и путей", // LibraryGamePathSettings
    "Signature bypasser не установлен", // LibraryNteBypasserMissingTitle
    "Моды NTE не будут загружаться, пока не установлен AyakaNTEModLoader.asi или UniversalSigBypasser.asi.", // LibraryNteBypasserMissingDescription
    "AyakaNTEBypasser", // LibraryNteBypasserAyaka
    "UniversalSigBypasser", // LibraryNteBypasserUniversal
    "Фильтр по названию мода...", // LibrarySearchHint
    "Установленные моды", // LibraryInstalledMods
    "Выбрано: {count}", // LibrarySelectedCount
    "Выбрать все видимые моды", // LibrarySelectAllVisibleMods
    "Модов: {count}", // LibraryModsCount
    "1 мод", // LibraryOneMod
    "Назад", // LibraryBack
    "Назад к папкам категорий", // LibraryBackToCategoryFolders
    "{active} Активен • {disabled} Отключён • {archived} Архив", // LibraryCategorySummary
    "Название А-Я", // LibrarySortNameAsc
    "Название Я-А", // LibrarySortNameDesc
    "Новые → старые", // LibrarySortDateDesc
    "Старые → новые", // LibrarySortDateAsc
    "Наименьший → Наибольший размер", // LibrarySortSizeAsc
    "Наибольший → Наименьший размер", // LibrarySortSizeDesc
    "Сортировка, группировка и вид установленных модов", // LibrarySortMenuTooltip
    "Сортировка модов", // LibrarySortModsHeading
    "Сортировка по названию мода, при отсутствии — по имени папки.", // LibrarySortNameTooltip
    "Используется самая новая известная дата установки, изменения содержимого или обновления.", // LibrarySortNewestTooltip
    "Сначала используется самая старая известная дата установки, изменения содержимого или обновления.", // LibrarySortOldestTooltip
    "Сортирует по общему размеру содержимого мода.", // LibrarySortSizeTooltip
    "Группировка модов", // LibraryGroupModsHeading
    "Группирует моды по категориям для каждой игры.", // LibraryGroupCategoryTooltip
    "Группирует моды по разделам «Активные», «Отключённые» и «В архиве».", // LibraryGroupStatusTooltip
    "Показывает один непрерывный отсортированный список модов.", // LibraryGroupNoneTooltip
    "Макет категорий", // LibraryCategoryLayoutHeading
    "Доступно при группировке по категориям.", // LibraryAvailableWhenGroupedByCategory
    "Сначала показывает плитки категорий, затем открывает по одной категории за раз.", // LibraryCategoryFoldersTooltip
    "Показывает каждую категорию отдельным разделом в списке модов.", // LibraryCategoryListTooltip
    "Сортировка категорий", // LibrarySortCategoriesHeading
    "Вручную", // LibraryCategorySortManual
    "По названию (А-Я)", // LibraryCategorySortByNameAsc
    "Меньше всего модов", // LibraryCategorySortByLeastMods
    "Больше всего модов", // LibraryCategorySortByMostMods
    "Используется ваш ручной порядок категорий.", // LibraryCategorySortManualTooltip
    "Сортирует папки и разделы категорий по названию категории.", // LibraryCategorySortByNameTooltip
    "Сначала показываются категории с наибольшим количеством модов.", // LibraryCategorySortByMostModsTooltip
    "Сначала показываются категории с наименьшим количеством модов.", // LibraryCategorySortByLeastModsTooltip
    "Разное", // LibraryMiscellaneousHeading
    "Внутри статусных групп сначала учитывается порядок категорий, затем выбранная сортировка.", // LibrarySortCategoryFirstTooltip
    "Сначала размещает активные моды, затем отключённые и архивные, после чего применяется выбранная сортировка.", // LibrarySortStatusFirstTooltip
    "Доступно при группировке по категориям в виде списка.", // LibraryUncategorizedFirstListOnlyTooltip
    "Показать/скрыть", // LibraryToggleVisibility
    "Состояние мода", // LibraryModStateHeading
    "Показать все состояния модов", // LibraryShowAllModStates
    "Скрыть все состояния модов", // LibraryHideAllModStates
    "Активные моды", // LibraryEnabledMods
    "Отключённые моды", // LibraryDisabledMods
    "Архивные моды", // LibraryArchivedMods
    "Состояние обновления", // LibraryUpdateStateHeading
    "Показать все состояния обновлений", // LibraryShowAllUpdateStates
    "Скрыть все состояния обновлений", // LibraryHideAllUpdateStates
    "Не привязан", // LibraryUnlinked
    "Актуально", // LibraryUpToDate
    "Доступно обновление", // LibraryUpdateAvailable
    "Проверка пропущена", // LibraryCheckSkipped
    "Источник отсутствует", // LibraryMissingSource
    "Изменён локально", // LibraryModifiedLocally
    "Обновление игнорируется", // LibraryIgnoringUpdate
    "Показывает моды, которые игнорируют текущее обновление или игнорируют обновления до отключения этой опции.", // LibraryIgnoringUpdateTooltip
    "Апдейт", // LibraryUpdate
    "Включить", // LibraryEnable
    "Отключить", // LibraryDisable
    "Архив", // LibraryArchive
    "Ещё", // LibraryMore
    "(нет)", // LibraryNone
    "Категорий пока нет.\n\n1. Откройте сведения мода, нажав на карточку.\n2. Нажмите «Без категории» под названием мода.\n3. Нажмите «+ Новая категория» и задайте имя.", // LibraryNoCategoryHelp
    "Категорий пока нет.", // LibraryNoCategoryYet
    "Новая категория", // LibraryNewCategory
    "Открыть", // LibraryOpen
    "Проводник", // LibraryFileExplorer
    "К этому моду не привязан источник GameBanana.", // LibraryNoGameBananaSource
    "Пропустить обновление один раз", // LibraryIgnoreUpdateOnce
    "Игнорирует текущее обновление, если оно доступно. Если обновления пока нет, запоминает текущую удалённую версию и игнорирует следующее обнаруженное обновление.", // LibraryIgnoreUpdateOnceTooltip
    "Синхронизируйте этот мод с GameBanana перед разовым игнорированием обновления.", // LibraryIgnoreUpdateOnceDisabledTooltip
    "Синхронизируйте хотя бы один выбранный мод с GameBanana перед разовым игнорированием обновления.", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "Игнорировать обновление всегда", // LibraryIgnoreUpdateAlways
    "Бессрочно задаёт для мода статус «Игнорировать обновления всегда», пока опция не будет снята.", // LibraryIgnoreUpdateAlwaysTooltip
    "Изменён", // LibraryModified
    "\n(Изменён)", // LibraryModifiedSuffix
    "…и ещё {count}", // LibraryAndMore
    "Изм. игнор разово", // LibraryModifiedIgnoringOnce
    "Изм. игнор всегда", // LibraryModifiedIgnoringAlways
    "Изм. есть обн.", // LibraryModifiedUpdateAvailable
    "Игнор разово", // LibraryIgnoringOnce
    "Игнор всегда", // LibraryIgnoringAlways
    "Отсутствует", // LibraryMissing
    "Пропущено", // LibrarySkipped
    "Пусто", // LibraryEmpty
    "Перемещение", // LibraryMoving
    "Переместить сюда", // LibraryMoveHere
    "Открыть {item}", // LibraryOpenItem
    "Перетащить в категорию", // LibraryDropOnCategory
    "Изменить порядок папки", // LibraryReorderFolder
    "Категории", // LibraryCategoriesHeading
    "Папок: {folders} / модов без категории: {uncategorized}", // LibraryFoldersUncategorizedSummary
    "Перетаскивание переключает порядок на ручной", // LibraryDropSwitchesToManualOrder
    "Переименовать", // LibraryRename
    "Переименовать (F2)", // LibraryRenameShortcut
    "Только папка, моды переместить наружу", // LibraryFolderOnlyMoveModsOutside
    "Папка вместе с модами внутри", // LibraryFolderAndModsInside
    "Папка удалена: {category}", // LibraryDeletedFolder
    "Активен", // LibraryStatusActive
    "Отключён", // LibraryStatusDisabled
    "Архив", // LibraryStatusArchived
    "Сейчас", // RelativeTimeNow
    "Сегодня", // RelativeTimeToday
    "{count}мин", // RelativeTimeMinutes
    "{count}ч", // RelativeTimeHours
    "{count}дн", // RelativeTimeDays
    "Перемещено в корзину", // LibraryRecycledAction
    "Удалено", // LibraryDeletedAction
    "Ошибка удаления", // LibraryDeleteFailed
    "Ошибка отключения", // LibraryDisableFailed
    "Ошибка перемещения в архив", // LibraryArchiveFailed
    "Ошибка включения", // LibraryEnableFailed
    "Ошибка восстановления", // LibraryRestoreFailed
    "Отключено", // LibraryActionDisabled
    "Архивировано", // LibraryActionArchived
    "Включено", // LibraryActionEnabled
    "Возвращено из архива", // LibraryActionUnarchived
    "{action}: {name}", // LibraryActionMessage
    "{action}: {count} модов", // LibraryActionCountMessage
    "{action}: {category} и {count} модов", // LibraryCategoryActionCountMessage
    "В очередь добавлены обновления для {count} модов", // LibraryQueuedUpdates
    "Ошибка переименования", // LibraryRenameFailed
    "Переименовано", // LibraryActionRenamed
    "Переименовано в: {name}", // LibraryRenamedTo
    "Личная заметка", // LibraryPersonalNote
    "Личная заметка сохранена", // LibrarySavedPersonalNote
    "Личная заметка удалена", // LibraryPersonalNoteRemoved
    "Не удалось сохранить личную заметку", // LibraryCouldNotSavePersonalNote
    "Удалить изображение", // LibraryRemoveImage
    "Нажмите здесь, чтобы", // LibraryClickHereTo
    "добавить изображения вручную.", // LibraryManuallyAddImages
    "Вы также можете перетащить изображения сюда,", // LibraryDropImagesHere
    "или вставить из буфера обмена (CTRL + V).", // LibraryPasteFromClipboard
    "Добавление изображений…", // LibraryAddingImages
    "Добавить изображения", // LibraryAddImages
    "Изображения", // LibraryImagesFileDialog
    "Добавление изображений: {count}", // LibraryAddingImagesCount
    "Не удалось добавить изображения", // LibraryCouldNotAddImages
    "Изображение удалено", // LibraryImageRemoved
    "Не удалось удалить изображение", // LibraryCouldNotRemoveImage
    "Описание", // LibraryDescription
    "Метаданные", // LibraryMetadata
    "Требуется RabbitFX", // LibraryRequiresRabbitFx
    "Добавить личную заметку", // LibraryAddPersonalNote
    "Сохранить личную заметку", // LibrarySavePersonalNote
    "Редактируемая заметка пользователя", // LibraryEditableUserNote
    "Изменить личную заметку", // LibraryEditPersonalNote
    "+ Добавить заметку", // LibraryAddNote
    "Локальный", // LibraryLocal
    "Открыть в проводнике", // LibraryOpenInFileExplorer
    "Источник", // LibrarySource
    "• Последняя синхронизация: {age}", // LibraryLastSynced
    "Ресинхр.", // LibraryResync
    "Отвязать", // LibraryUnlink
    "Страница GameBanana", // LibraryGameBananaPage
    "Привяжите мод к GameBanana, чтобы включить отслеживание обновлений и синхронизацию метаданных.", // LibraryLinkGameBananaPrompt
    "URL или ID", // LibraryUrlOrId
    "Синхронизировать мод", // LibrarySyncMod
    "Параметры обновлений:", // LibraryUpdatePreferences
    "Синхронизация с GameBanana…", // LibrarySyncingGameBanana

    // Window: Settings
    "Опции", // SettingsWindowTitle
    "Общие", // SettingsTabGeneral
    "Категории", // SettingsTabCategory
    "Дополнительно", // SettingsTabAdvanced
    "Игра и путь", // SettingsTabGamePath
    "О программе", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "Поведение", // SettingsGeneralBehaviorSection
    "При запуске игры:", // SettingsGeneralBehaviorWhenLaunchingGame
    "После установки мода:", // SettingsGeneralBehaviorAfterInstallingMod
    "При запуске утилиты:", // SettingsGeneralBehaviorWhenLaunchingTool
    "Метаданные в сведениях о моде:", // SettingsGeneralBehaviorModDetailMetadata
    "Ничего не делать", // SettingsGeneralBehaviorDoNothing
    "Свернуть Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "Выйти из Hestia", // SettingsGeneralBehaviorExitHestia
    "Добавить в выделенное", // SettingsGeneralBehaviorAddToSelection
    "Открыть сведения о моде", // SettingsGeneralBehaviorOpenModDetail
    "Не показывать", // SettingsGeneralBehaviorNeverShow
    "Показывать, если нет описания", // SettingsGeneralBehaviorShowIfNoDescription
    "Всегда показывать", // SettingsGeneralBehaviorAlwaysShow

    // Window: Settings > General > Installed Mods List
    "Список установленных модов", // SettingsGeneralInstalledModsListSection
    "Группировать список:", // SettingsGeneralInstalledModsGroupListBy
    "Макет категорий:", // SettingsGeneralInstalledModsCategoryLayout
    "Категория", // SettingsGeneralInstalledModsGroupCategory
    "Статус", // SettingsGeneralInstalledModsGroupStatus
    "Нет", // SettingsGeneralInstalledModsGroupNone
    "Список", // SettingsGeneralInstalledModsLayoutList
    "Папки", // SettingsGeneralInstalledModsLayoutFolders
    "Сначала сортировать по категории", // SettingsGeneralInstalledModsSortByCategoryFirst
    "Сортировка по порядку категорий (не обязательно по алфавиту).", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
    "Сначала сортировать по статусу", // SettingsGeneralInstalledModsSortByStatusFirst
    "Сначала активные моды, затем отключённые и архивные.", // SettingsGeneralInstalledModsSortByStatusFirstTooltip
    "Показывать статус мода на карточке", // SettingsGeneralInstalledModsShowModStatusOnCard
    "Показывать категорию на карточке", // SettingsGeneralInstalledModsShowCategoryOnCard
    "Состояние мода всё равно отображается цветной точкой статуса.", // SettingsGeneralInstalledModsShowCategoryOnCardTooltip
    "Показывать отключённые моды", // SettingsGeneralInstalledModsShowDisabledMods
    "Показывать архивные моды", // SettingsGeneralInstalledModsShowArchivedMods
    "Показывать моды без категории первыми", // SettingsGeneralInstalledModsShowUncategorizedModsFirst

    // Window: Settings > General > Operational
    "Операции", // SettingsGeneralOperationalSection
    "Моды для проверки обновлений:", // SettingsGeneralOperationalModsToCheckForUpdates
    "Автоматически обновлять моды:", // SettingsGeneralOperationalAutomaticallyUpdateMods
    "Активные", // SettingsGeneralOperationalStatusActive
    "Отключённые", // SettingsGeneralOperationalStatusDisabled
    "Архивные", // SettingsGeneralOperationalStatusArchived
    "Также обновлять изменённые моды:", // SettingsGeneralOperationalAlsoUpdateModifiedMods
    "Да", // SettingsGeneralOperationalYes
    "Нет, но показывать кнопку «Обновить»", // SettingsGeneralOperationalNoButShowUpdateButton
    "Нет, скрыть кнопку «Обновить»", // SettingsGeneralOperationalNoAndHideUpdateButton
    "При установке уже существующего мода:", // SettingsGeneralOperationalWhenInstallingExistingMod
    "Всегда спрашивать", // SettingsGeneralOperationalAlwaysAsk
    "Всегда заменять", // SettingsGeneralOperationalAlwaysReplace
    "Всегда объединять", // SettingsGeneralOperationalAlwaysMerge
    "Всегда оставлять оба", // SettingsGeneralOperationalAlwaysKeepBoth
    "При обновлении модов всегда заменять", // SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods
    "При удалении мода:", // SettingsGeneralOperationalWhenDeletingMod
    "Переместить в корзину", // SettingsGeneralOperationalMoveToRecycleBin
    "Удалить навсегда", // SettingsGeneralOperationalDeletePermanently

    // Window: Settings > General > Tasks
    "Задачи", // SettingsGeneralTasksSection
    "Вид задач:", // SettingsGeneralTasksLayout
    "Разделы", // SettingsGeneralTasksLayoutSections
    "Вкладки", // SettingsGeneralTasksLayoutTabbed
    "Единый список", // SettingsGeneralTasksLayoutSingleList
    "Очищать завершённые:", // SettingsGeneralTasksClearCompletedTasks
    "Очистить", // SettingsGeneralTasksClearTasks
    "Порядок задач:", // SettingsGeneralTasksOrder
    "Старые → Новые", // SettingsGeneralTasksOldestToNewest
    "Новые → Старые", // SettingsGeneralTasksNewestToOldest

    // Window: Settings > Category
    "Выберите игру для настройки категорий.", // SettingsCategorySelectGame
    "Каталог", // SettingsCategoryBrowseSection
    "Создавать категории GameBanana автоматически", // SettingsCategoryAutoCreateGameBananaCategories
    "Применяется к {game}.", // SettingsCategoryAppliesToGame
    "Категории", // SettingsCategoryCategoriesSection
    "Выбрать все категории", // SettingsCategorySelectAllCategories
    "Снять выбор со всех категорий", // SettingsCategoryUnselectAllCategories
    "Новая", // SettingsCategoryNew
    "Новая категория (Ctrl+N)", // SettingsCategoryNewTooltip
    "Удалить", // SettingsCategoryDelete
    "Без категории", // SettingsCategoryUncategorized

    // Window: Settings > Game & Path
    "Проблемы с путями?", // SettingsPathScanTitle
    "Hestia может выполнить глубокое сканирование, чтобы найти пути к XXMI и поддерживаемым играм", // SettingsPathScanDescription
    "Сканировать пути", // SettingsPathScanButtonScan
    "Сканирование…", // SettingsPathScanButtonScanning
    "Сканирует доступные диски в поисках XXMI и исполняемых файлов игр.", // SettingsPathScanButtonTooltip
    "XXMI", // SettingsPathXxmiSection
    "Лаунчер XXMI:", // SettingsPathXxmiLauncher
    "Путь не найден", // SettingsPathPathNotFound
    "Использовать путь к модам XXMI по умолчанию для игр", // SettingsPathUseDefaultXxmiModPath
    "Игра", // SettingsPathGameSection
    "EXE-файл игры:", // SettingsPathGameExeFile
    "Папка модов {code}:", // SettingsPathGameModsFolder
    "Папка модов (~mods):", // SettingsPathUnrealModFolder

    // Window: Settings > Advanced > Appearance
    "Внешний вид", // SettingsAdvancedAppearanceSection
    "Язык:", // SettingsAdvancedAppearanceLanguage
    "Стиль шрифта:", // SettingsAdvancedAppearanceFontStyle
    "Классический", // SettingsAdvancedAppearanceFontClassic
    "Современный", // SettingsAdvancedAppearanceFontModern
    "Элегантный", // SettingsAdvancedAppearanceFontElegant
    "Традиционный", // SettingsAdvancedAppearanceFontTraditional
    "Использует системный шрифт интерфейса", // SettingsAdvancedAppearanceFontClassicTooltip
    "Использует шрифт «Selawik»", // SettingsAdvancedAppearanceFontModernTooltip
    "Использует Diphylleia, для жирного текста — Gabriela", // SettingsAdvancedAppearanceFontElegantTooltip
    "Использует New Tegomin, для жирного текста — Coustard", // SettingsAdvancedAppearanceFontTraditionalTooltip
    "Всегда переводить сведения о моде", // SettingsAdvancedAppearanceAlwaysTranslateModDetails
    "При включении описания и метаданные модов автоматически переводятся на выбранный язык при просмотре сведений.", // SettingsAdvancedAppearanceAlwaysTranslateModDetailsTooltip
    // Window: Settings > Advanced > Content Restriction
    "Ограничения содержимого", // SettingsAdvancedContentRestrictionSection
    "NSFW-контент:", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "Скрывать NSFW-моды и счётчик", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "Скрывать NSFW-моды, показывать счётчик", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "Показывать с цензурированными изображениями", // SettingsAdvancedContentRestrictionShowImagesCensored
    "Показывать без ограничений", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Proxy
    "Прокси", // SettingsAdvancedProxySection
    "Адрес прокси:", // SettingsAdvancedProxyAddress
    "Протокол необязателен; адрес без протокола определяется автоматически. Используйте socks5h:// или socks4a:// для DNS через прокси.", // SettingsAdvancedProxyHelp
    "Прокси с аутентификацией не поддерживаются.", // SettingsAdvancedProxyCredentialsUnsupported
    "Введите корректный адрес прокси.", // SettingsAdvancedProxyAddressInvalid
    "Прокси отключен", // SettingsAdvancedProxyDisabled
    "Прокси включен", // SettingsAdvancedProxyEnabled
    "Не удалось подключиться к прокси", // SettingsAdvancedProxyConnectionFailed
    "При запуске Hestia проверяет подключение \nк прокси перед началом операций. \nПри ошибке Hestia продолжит работу без прокси.", // SettingsAdvancedProxyStartupBehavior

    // Window: Settings > Advanced > Cache and Archive
    "Кэш и архив", // SettingsAdvancedCacheArchiveSection
    "Размер кэша:", // SettingsAdvancedCacheArchiveCacheSize
    "Текущее использование: {gb} ГБ", // SettingsAdvancedCacheArchiveCurrentUsage
    "Очистить кэш", // SettingsAdvancedCacheArchiveClearCache
    "Кэш очищен", // SettingsAdvancedCacheArchiveCacheCleared
    "Не удалось очистить кэш", // SettingsAdvancedCacheArchiveClearCacheFailed
    "Использование архива: {gb} ГБ", // SettingsAdvancedCacheArchiveArchiveUsage
    "Удалить архивные моды", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "Перемещено в корзину", // SettingsAdvancedCacheArchiveRecycled
    "Удалено", // SettingsAdvancedCacheArchiveDeleted
    "{count} архивных модов", // SettingsAdvancedCacheArchiveArchivedMods
    "Архивы очищены: {count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "Нет архивов для очистки", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "Не удалось очистить архивы", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "от {authors}", // SettingsAboutBy
    "Версия:", // SettingsAboutVersion
    "Нажмите, чтобы показать «Что нового».", // SettingsAboutVersionTooltip
    "Автоматически проверять наличие обновлений", // SettingsAboutAutomaticallyCheckForUpdate
    "Проверка…", // SettingsAboutUpdateChecking
    "Перезапустить для обновления", // SettingsAboutUpdateRestartToUpdate
    "Проверить наличие обновлений", // SettingsAboutUpdateCheckForUpdate
    "Обновлений нет", // SettingsAboutUpdateUpToDate
    "Не удалось проверить", // SettingsAboutUpdateFailedToCheck
    "Требуется ручное обновление", // SettingsAboutUpdateManualRequired
    "Доступно обновление", // SettingsAboutUpdateAvailable
    "Обновление готово", // SettingsAboutUpdateReady
    "Ошибка обновления", // SettingsAboutUpdateFailed
    "Загрузка обновления отменена", // SettingsAboutUpdateDownloadCanceled
    "Дождитесь завершения активных задач перед обновлением", // SettingsAboutUpdateWaitForActiveTasks
    "Не удалось применить обновление", // SettingsAboutUpdateCouldNotApply
    "Hestia установлена в папку, которую этот процесс не может обновить:\n{path}\nПереместите Hestia в другую папку и попробуйте снова или обновите эту установку из процесса с повышенными правами.", // SettingsAboutUpdateManualInstallFolder
    "Источники", // SettingsAboutAttributionSection
    "Источник данных: GameBanana, API используется с разрешения. Метаданные модов, медиафайлы и данные каталога GameBanana получены из GameBanana.", // SettingsAboutAttributionGameBanana

    // Translation strings
    "Перевести (F7)", // TranslationToggleShortcut
    "Перевести заново", // TranslationRetranslate
    "Перевод не удался", // TranslationFailed
    "Выполняется перевод", // TranslationInProgress
];
