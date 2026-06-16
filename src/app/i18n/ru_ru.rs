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
    "Подробнее о конфиденциальности", // FeedbackSurveyPrivacyDetails
    "Отзыв отправляется анонимно.\nНевозможно определить или связаться с отправителем.\nРезультаты голосований могут быть опубликованы публично, но сообщения остаются приватными.\nНа сервер опроса будет отправлен только следующий набор данных:", // FeedbackSurveyPrivacyCopy
    "• Клиент: SHA-256 хэш случайно созданного UUID из файла hestia.toml\n• URL сервера и базы данных: {server_url}\n• Геолокация сервера: Asia Pacific", // FeedbackSurveyPrivacyPayload
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
    "Ошибки", // TasksFailed
    "Ошибки ({count})", // TasksFailedCount
    "Нет задач", // TasksNoTasks
    "В очереди", // TasksStatusQueued
    "Установка", // TasksStatusInstalling
    "Загрузка", // TasksStatusDownloading
    "Отмена", // TasksStatusCanceling
    "Завершено", // TasksStatusCompleted
    "Ошибка", // TasksStatusFailed
    "Отменено", // TasksStatusCanceled
    "Отменяется…", // TasksCanceling
    "Отмена", // TasksCancel
    "Повторить", // TasksRetry
    "Возобновить", // TasksResume
    "Запуск загрузки…", // TasksStartingDownload
    "В очереди…", // TasksQueuedProgress
    "Установка файлов мода…", // TasksInstallingModFiles
    "Отмена задачи…", // TasksCancelingTask

    // Window: Tools
    "Инструменты", // ToolsWindowTitle
    "Игра не выбрана", // ToolsNoGameSelected
    "Запуск", // ToolsLaunch
    "Настроить параметры запуска", // ToolsSetLaunchOptions
    "Открыть папку", // ToolsOpenFolder
    "Открепить от строки заголовка", // ToolsUnpinFromTitlebar
    "Закрепить в строке заголовка", // ToolsPinToTitlebar
    "Удалить", // ToolsRemove
    "Добавить инструмент", // ToolsAddTool
    "Инструмент", // ToolsFallbackLabel
    "Сначала выберите игру, чтобы добавить инструмент", // ToolsNoGameSelectedForAdd
    "Инструмент уже добавлен", // ToolsAlreadyAdded
    "Инструмент добавлен", // ToolsToolAdded
    "Инструмент удалён", // ToolsToolRemoved
    "Для одной игры в строке заголовка можно показать не более 4 инструментов", // ToolsTitlebarLimit
    "Достигнут лимит инструментов строки заголовка", // ToolsTitlebarLimitReached
    "Отсутствует исполняемый файл инструмента", // ToolsExecutableMissing
    "Инструмент не найден: {path}", // ToolsNotFound
    "Инструмент запущен: {tool}", // ToolsLaunched
    "Не удалось запустить инструмент", // ToolsCouldNotLaunch
    "Не удалось открыть расположение", // ToolsCouldNotOpenLocation
    "Параметры запуска инструмента сохранены", // ToolsLaunchOptionsSaved
    "Инструмент добавлен", // ToolsActionAdded
    "Инструмент удалён", // ToolsActionRemoved
    "Инструмент запущен", // ToolsActionLaunched

    // Window: Tool Launch Options
    "Параметры запуска", // ToolLaunchOptionsWindowTitle
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
    "Остановлено", // DialogStopped
    "Не найдено", // DialogNotFound
    "Поиск…", // DialogSearching
    "Найдено", // DialogFound
    "Выбрать…", // DialogChoose
    "Найдено несколько", // DialogMultipleFound
    "Импортированный мод", // DialogImportedMod
    "Отсутствует .ini", // DialogMissingIniTitle
    "В родительской папке архива не найден распознаваемый .ini-файл. Возможно, архив содержит несколько модов.\nВыберите папки для установки:", // DialogMissingIniPrompt
    "Установить", // DialogInstall
    "Установить объединённо", // DialogInstallMerged
    "Установить выбранные папки в одну папку мода и считать их одним модом", // DialogInstallMergedTooltip
    "Установить отдельно", // DialogInstallSeparately
    "Установить выбранные папки в отдельные папки модов", // DialogInstallSeparatelyTooltip
    "Ошибка установки", // DialogInstallFailed
    "Установка недоступна", // DialogInstallUnavailable
    "Ошибка установки для {name}: {error}", // DialogInstallFailedFor
    "Ошибка проверки установки для {name}: {error}", // DialogInstallInspectionFailed
    "Ошибка отправки установки для {name}", // DialogInstallDispatchFailed
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
    "Конфликт (заменить)", // DialogConflictReplace
    "Конфликт (объединить)", // DialogConflictMerge
    "Конфликт (оставить оба)", // DialogConflictKeepBoth
    "Конфликт (отмена)", // DialogConflictCancel
    "Перетащите моды для установки\n\nили\n\nперетащите изображения, чтобы добавить в:\n{name}", // DialogDropModsImages
    "Перетащите для установки", // DialogDropToInstall
    "Не поддерживается", // DialogUnsupported
    "Не поддерживается: {file}", // DialogUnsupportedFile
    "файл", // DialogFile
    "Архивы", // DialogFileFilterArchives
    "Исполняемый файл", // DialogFileFilterExecutable
    "Сначала откройте сведения о несвязанном моде", // DialogOpenUnlinkedModDetailFirst
    "Установка: {count} мод.", // DialogInstallingCount
    "Не удалось создать папку модов", // DialogCouldNotCreateModsFolder
    "Не удалось отключить установленный мод", // DialogCouldNotDisableInstalledMod
    "Не удалось оставить мод отключённым", // DialogCouldNotKeepModDisabled
    "Установлено", // DialogInstalledAction
    "Установлено модов: {count}", // DialogInstalledCount
    "Установлено: {name}", // DialogInstalledName
    "Синхронизировано", // DialogSyncedAction
    "Обновление недоступно", // DialogUpdateUnavailable
    "Обновление: {title}", // DialogUpdatingTask

    // Main GUI: App Messages
    "Не удалось сохранить настройки", // AppCouldNotSaveSettings
    "Не удалось сохранить данные", // AppCouldNotSaveData
    "Предупреждение: {detail}", // AppLogWarn
    "Ошибка: {detail}", // AppLogError
    "Ошибка запуска", // AppLaunchFailed
    "Путь запуска не задан", // AppLaunchPathNotSet
    "Игра не выбрана", // AppGameNotSelected
    "Играть (с модами)", // AppPlayModded
    "Играть (без модов)", // AppPlayVanilla
    "С модами", // AppModded
    "Без модов", // AppVanilla
    "Путь {label} не задан для {game}", // AppLaunchPathNotSetForGame
    "Запущено: {game} ({mode})", // AppLaunchedGameMode
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
    "Перезагружено: {count} мод., изменений нет", // AppReloadedNoChanges
    "Просканировано модов: {count}", // AppModsScanned
    "Перезагружено: {count} мод.", // AppReloaded
    "Добавлено: {count}", // AppReloadAdded
    "Удалено: {count}", // AppReloadRemoved
    "Изменено: {count}", // AppReloadChanged
    "Перезагрузить: {line}", // AppReloadAction
    "Категория", // AppCategoryAction
    "Создано: \"{category}\"", // AppCategoryCreated
    "{mod} не имеет допустимой категории GameBanana; создание категории пропущено", // AppCategorySkippedNoValidGameBananaCategory
    "Опрос", // AppSurveyAction
    "Нечитаемый ожидающий отзыв удалён: {error}", // AppSurveyDiscardedUnreadablePendingFeedbackPayload
    "Повторная отправка ожидающего отзыва", // AppSurveyRetryingPendingFeedbackPayload
    "Отзыв для {version} отправлен", // AppSurveySubmittedFeedback
    "Ошибка отправки отзыва для {version}: {error}", // AppSurveyFeedbackSubmitFailed
    "Ожидающий отзыв для {version} удалён", // AppSurveyDiscardedPendingFeedbackPayload
    "Не удалось отправить отзыв", // AppCouldNotSubmitFeedback
    "Отзыв отправлен", // AppFeedbackSubmitted
    "Загрузка отменена: {title}", // AppDownloadCanceled

    // Main GUI: Chrome
    "\nМенеджер модов", // ChromeAppSubtitle
    "Играть", // ChromePlay
    "Установить\nZip/Rar", // ChromeInstallArchive
    "Установить\nПапку", // ChromeInstallFolder
    "Обновить", // ChromeReload
    "Игра не установлена или не настроена.", // ChromeGameNotInstalled
    "Запустить игру с модами через XXMI", // ChromeLaunchWithModsTooltip
    "Запустить игру без модов", // ChromeLaunchWithoutModsTooltip
    "Играть с модами", // ChromePlayWithMods
    "Играть без модов", // ChromePlayWithoutMods
    "Установить мод из архива zip/rar/7z", // ChromeInstallArchiveTooltip
    "Установить мод из уже распакованной папки", // ChromeInstallFolderTooltip
    "Установить", // ChromeInstall
    "Установить и отключить", // ChromeInstallDisabled
    "Повторно просканировать установленные моды и проверить обновления на GameBanana (Ctrl+R)", // ChromeReloadLibraryTooltip
    "Обновить текущий список (Ctrl+R)", // ChromeReloadBrowseTooltip
    "Закрыть", // ChromeClose
    "Восстановить", // ChromeRestore
    "Развернуть", // ChromeMaximize
    "Свернуть", // ChromeMinimize
    "Мои моды", // ChromeMyMods
    "Обзор", // ChromeBrowse
    "Инструменты (Ctrl+T)", // ChromeToolsTooltip
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
    "Установлено", // BrowseInstalled
    "Открыть в браузере", // BrowseOpenInBrowser
    "Не удалось открыть браузер", // BrowseCouldNotOpenBrowser
    "Загрузка ещё…", // BrowseLoadingMore
    "Для этой игры не настроен список персонажей.", // BrowseNoCharacterList
    "Обновить персонажей", // BrowseRefreshCharacters
    "Очистить этот фильтр", // BrowseClearFilter
    "Выбрано: {name}", // BrowseSelectedCharacter
    "Персонажей: {count}", // BrowseCharacterCount
    "Ожидание", // BrowseWaiting
    "GameBanana не вернула список персонажей.", // BrowseNoCharactersReturned
    "Сведения о моде", // BrowseModDetail
    "Скопировать ID GameBanana", // BrowseCopyGameBananaId
    "ID GameBanana скопирован", // BrowseGameBananaIdCopied
    "Неизвестно", // BrowseUnknown
    "Обновления", // BrowseUpdates
    "Этот мод приватный.", // BrowsePrivateMod
    "Автоматическая установка отключена. Если у вас есть доступ, можно просмотреть или скачать мод напрямую на GameBanana.", // BrowseAutomaticInstallDisabledAuthorized
    "Этот мод удерживается", // BrowseWithheldMod
    "Удерживается", // BrowseWithheldBy
    "Автоматическая установка отключена до снятия удержания.", // BrowseAutomaticInstallDisabledWithheld
    "Нарушение правил", // BrowseRuleViolation
    "Этого мода больше не существует.", // BrowseDeletedModNoLongerExists
    "Этот мод удалён пользователем", // BrowseDeletedBy
    "Этот мод удалён", // BrowseDeleted
    "Файлы", // BrowseFiles
    "Архивные файлы", // BrowseArchivedFiles
    "Загрузка сведений о моде…", // BrowseLoadingDetails
    "{size} • {date} • загрузок: {downloads}", // BrowseFileMetadata
    "Выбрать файлы", // BrowseChooseFiles
    "У этого мода доступно несколько файлов.\nВыберите файлы для загрузки и установки:", // BrowseMultipleFilesPrompt
    "Для этой игры не настроен список категорий персонажей GameBanana.", // BrowseNoConfiguredCharacterCategoryList
    "Персонажи недоступны", // BrowseCharactersUnavailable
    "Ошибка подключения", // BrowseConnectionFailed
    "Ошибка обзора", // BrowseFailed
    "Ошибка загрузки персонажей", // BrowseCharactersFailed
    "Ошибка сведений обзора", // BrowseDetailFailed
    "Не удалось загрузить обновления", // BrowseCouldNotLoadUpdates
    "Загружено: {title}", // BrowseDownloaded
    "Не удалось подготовить установку", // BrowseCouldNotPrepareInstall
    "Ошибка загрузки", // BrowseDownloadFailed
    "Подготовка загрузки: {title}", // BrowseResolvingDownload
    "Файлы для загрузки не найдены", // BrowseNoDownloadableFilesFound
    "Файлы не выбраны", // BrowseNoFilesSelected
    "Загрузка добавлена в очередь", // BrowseDownloadQueued
    "Ошибка обновления страницы обзора; используются кэшированные результаты: {warning}", // BrowsePageWarning
    "Ошибка страницы обзора: {error}", // BrowsePageFailed
    "Ошибка обновления категорий персонажей; используются кэшированные результаты: {warning}", // BrowseCharacterCategoriesWarning
    "Ошибка категорий персонажей: {error}", // BrowseCharacterCategoriesFailed
    "Ошибка обновления сведений обзора для мода {mod_id}; используются кэшированные сведения: {warning}", // BrowseDetailWarning
    "Ошибка сведений обзора для мода {mod_id}: {error}", // BrowseDetailFailedMessage
    "Ошибка обновления обзоров для мода {mod_id}; используются кэшированные обновления: {warning}", // BrowseUpdatesWarning
    "Ошибка обновлений обзора для мода {mod_id}: {error}", // BrowseUpdatesFailedMessage
    "Ошибка загрузки для {title}: {error}", // BrowseDownloadFailedMessage

    // Main GUI: My Mods
    "Сканирование установленных модов", // LibraryScanningInstalledMods
    "Убедитесь, что XXMI установлен правильно.", // LibraryEnsureXxmiInstalled
    "- Скачать XXMI: ", // LibraryDownloadXxmi
    "Затем откройте настройки, чтобы включить игру и при необходимости исправить путь к игре.\n- Нажмите на значок игры, чтобы включить или отключить её.\n- Вручную выберите путь, нажав кнопку […].", // LibraryBlankInstructions
    "Открыть настройки", // LibraryOpenSettings
    "Фильтр по названию мода…", // LibrarySearchHint
    "Установленные моды", // LibraryInstalledMods
    "Выбрано: {count}", // LibrarySelectedCount
    "Выбрать все видимые моды", // LibrarySelectAllVisibleMods
    "Модов: {count}", // LibraryModsCount
    "1 мод", // LibraryOneMod
    "Назад", // LibraryBack
    "Назад к папкам категорий", // LibraryBackToCategoryFolders
    "{active} активных • {disabled} отключено • {archived} в архиве", // LibraryCategorySummary
    "Название А-Я", // LibrarySortNameAsc
    "Название Я-А", // LibrarySortNameDesc
    "Новые → Старые", // LibrarySortDateDesc
    "Старые → Новые", // LibrarySortDateAsc
    "Сортировка, группировка и раскладка установленных модов", // LibrarySortMenuTooltip
    "Сортировка модов", // LibrarySortModsHeading
    "Сортировка по названию мода, при отсутствии — по имени папки.", // LibrarySortNameTooltip
    "Используется самая новая известная дата установки, изменения содержимого или обновления.", // LibrarySortNewestTooltip
    "Сначала используется самая старая известная дата установки, изменения содержимого или обновления.", // LibrarySortOldestTooltip
    "Группировка модов", // LibraryGroupModsHeading
    "Группирует моды по категориям для каждой игры.", // LibraryGroupCategoryTooltip
    "Группирует моды по разделам «Активные», «Отключённые» и «Архив».", // LibraryGroupStatusTooltip
    "Показывает один непрерывный отсортированный список модов.", // LibraryGroupNoneTooltip
    "Раскладка категорий", // LibraryCategoryLayoutHeading
    "Доступно при группировке по категориям.", // LibraryAvailableWhenGroupedByCategory
    "Сначала показывает плитки категорий, затем открывает по одной категории.", // LibraryCategoryFoldersTooltip
    "Показывает каждую категорию отдельным разделом в списке модов.", // LibraryCategoryListTooltip
    "Сортировка категорий", // LibrarySortCategoriesHeading
    "Вручную", // LibraryCategorySortManual
    "По названию (А-Я)", // LibraryCategorySortByNameAsc
    "Меньше модов", // LibraryCategorySortByLeastMods
    "Больше модов", // LibraryCategorySortByMostMods
    "Используется ваш ручной порядок категорий.", // LibraryCategorySortManualTooltip
    "Категории сортируются по названию.", // LibraryCategorySortByNameTooltip
    "Сначала показываются категории с наибольшим количеством модов.", // LibraryCategorySortByMostModsTooltip
    "Сначала показываются категории с наименьшим количеством модов.", // LibraryCategorySortByLeastModsTooltip
    "Разное", // LibraryMiscellaneousHeading
    "Внутри статусных групп сначала учитывается порядок категорий, затем выбранная сортировка.", // LibrarySortCategoryFirstTooltip
    "Сначала активные моды, затем отключённые и архивные, после чего применяется выбранная сортировка.", // LibrarySortStatusFirstTooltip
    "Доступно при группировке по категориям в списке.", // LibraryUncategorizedFirstListOnlyTooltip
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
    "Без связи", // LibraryUnlinked
    "Актуально", // LibraryUpToDate
    "Доступно обновление", // LibraryUpdateAvailable
    "Проверка пропущена", // LibraryCheckSkipped
    "Источник отсутствует", // LibraryMissingSource
    "Изменён локально", // LibraryModifiedLocally
    "Обновление игнорируется", // LibraryIgnoringUpdate
    "Показывает моды, которые игнорируют текущее обновление или игнорируют обновления до отключения.", // LibraryIgnoringUpdateTooltip
    "Обновить", // LibraryUpdate
    "Включить", // LibraryEnable
    "Отключить", // LibraryDisable
    "В архив", // LibraryArchive
    "Ещё", // LibraryMore
    "(нет)", // LibraryNone
    "Категорий пока нет.\n\n1. Откройте сведения мода, нажав на карточку.\n2. Нажмите «Без категории» под названием мода.\n3. Нажмите «+ Новая категория» и задайте имя.", // LibraryNoCategoryHelp
    "Категорий пока нет.", // LibraryNoCategoryYet
    "Новая категория", // LibraryNewCategory
    "Открыть", // LibraryOpen
    "Проводник", // LibraryFileExplorer
    "Для этого мода не указан источник GameBanana.", // LibraryNoGameBananaSource
    "Пропустить обновление один раз", // LibraryIgnoreUpdateOnce
    "Игнорирует текущее обновление, если оно доступно. Если обновления ещё нет, запоминает текущую удалённую версию и игнорирует следующее обнаруженное обновление.", // LibraryIgnoreUpdateOnceTooltip
    "Перед использованием «пропустить один раз» синхронизируйте этот мод с GameBanana.", // LibraryIgnoreUpdateOnceDisabledTooltip
    "Перед использованием «пропустить один раз» синхронизируйте хотя бы один выбранный мод с GameBanana.", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "Игнорировать обновление всегда", // LibraryIgnoreUpdateAlways
    "Бессрочно устанавливает статус обновления мода в «Игнорировать обновление всегда» до снятия отметки.", // LibraryIgnoreUpdateAlwaysTooltip
    "Изменён", // LibraryModified
    "\n(Изменён)", // LibraryModifiedSuffix
    "…и ещё {count}", // LibraryAndMore
    "Изменён и пропущен один раз", // LibraryModifiedIgnoringOnce
    "Изменён и пропущен всегда", // LibraryModifiedIgnoringAlways
    "Изменён и доступно обновление", // LibraryModifiedUpdateAvailable
    "Пропущен один раз", // LibraryIgnoringOnce
    "Пропущен всегда", // LibraryIgnoringAlways
    "Отсутствует", // LibraryMissing
    "Пропущен", // LibrarySkipped
    "Пусто", // LibraryEmpty
    "Перемещение", // LibraryMoving
    "Переместить сюда", // LibraryMoveHere
    "Открыть {item}", // LibraryOpenItem
    "Перетащить в категорию", // LibraryDropOnCategory
    "Переупорядочить папку", // LibraryReorderFolder
    "Категории", // LibraryCategoriesHeading
    "Папок: {folders} / без категории: {uncategorized}", // LibraryFoldersUncategorizedSummary
    "Перетаскивание переключает порядок на ручной", // LibraryDropSwitchesToManualOrder
    "Переименовать", // LibraryRename
    "Переименовать (F2)", // LibraryRenameShortcut
    "Только папка, переместить моды наружу", // LibraryFolderOnlyMoveModsOutside
    "Папка и моды внутри", // LibraryFolderAndModsInside
    "Папка удалена: {category}", // LibraryDeletedFolder
    "Активен", // LibraryStatusActive
    "Отключён", // LibraryStatusDisabled
    "Архив", // LibraryStatusArchived
    "Перемещено в корзину", // LibraryRecycledAction
    "Удалено", // LibraryDeletedAction
    "Ошибка удаления", // LibraryDeleteFailed
    "Ошибка отключения", // LibraryDisableFailed
    "Ошибка архивации", // LibraryArchiveFailed
    "Ошибка включения", // LibraryEnableFailed
    "Ошибка восстановления", // LibraryRestoreFailed
    "Отключено", // LibraryActionDisabled
    "Архивировано", // LibraryActionArchived
    "Включено", // LibraryActionEnabled
    "Возвращено из архива", // LibraryActionUnarchived
    "{action}: {name}", // LibraryActionMessage
    "{action} модов: {count}", // LibraryActionCountMessage
    "{action} {category} и модов: {count}", // LibraryCategoryActionCountMessage
    "Обновления добавлены в очередь для модов: {count}", // LibraryQueuedUpdates
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
    "Редактируемая пользовательская заметка", // LibraryEditableUserNote
    "Изменить личную заметку", // LibraryEditPersonalNote
    "+ Добавить заметку", // LibraryAddNote
    "Локально", // LibraryLocal
    "Открыть в проводнике", // LibraryOpenInFileExplorer
    "Источник", // LibrarySource
    "• Последняя синхронизация: {age}", // LibraryLastSynced
    "Синхронизировать", // LibraryResync
    "Отвязать", // LibraryUnlink
    "Страница GameBanana", // LibraryGameBananaPage
    "Свяжите с GameBanana, чтобы включить отслеживание обновлений и синхронизацию метаданных.", // LibraryLinkGameBananaPrompt
    "URL или ID", // LibraryUrlOrId
    "Синхронизировать мод", // LibrarySyncMod
    "Настройки обновлений:", // LibraryUpdatePreferences
    "Синхронизация с GameBanana…", // LibrarySyncingGameBanana

    // Window: Settings
    "Настройки", // SettingsWindowTitle
    "Общие", // SettingsTabGeneral
    "Категории", // SettingsTabCategory
    "Дополнительно", // SettingsTabAdvanced
    "Игра и путь", // SettingsTabGamePath
    "О программе", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "Поведение", // SettingsGeneralBehaviorSection
    "При запуске игры:", // SettingsGeneralBehaviorWhenLaunchingGame
    "После установки мода:", // SettingsGeneralBehaviorAfterInstallingMod
    "При запуске инструмента:", // SettingsGeneralBehaviorWhenLaunchingTool
    "Метаданные сведений о моде:", // SettingsGeneralBehaviorModDetailMetadata
    "Ничего не делать", // SettingsGeneralBehaviorDoNothing
    "Свернуть Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "Выйти из Hestia", // SettingsGeneralBehaviorExitHestia
    "Добавить к выбранному", // SettingsGeneralBehaviorAddToSelection
    "Открыть сведения о моде", // SettingsGeneralBehaviorOpenModDetail
    "Не показывать", // SettingsGeneralBehaviorNeverShow
    "Показывать, если нет описания", // SettingsGeneralBehaviorShowIfNoDescription
    "Всегда показывать", // SettingsGeneralBehaviorAlwaysShow

    // Window: Settings > General > Installed Mods List
    "Список установленных модов", // SettingsGeneralInstalledModsListSection
    "Группировать список:", // SettingsGeneralInstalledModsGroupListBy
    "Раскладка категорий:", // SettingsGeneralInstalledModsCategoryLayout
    "Категория", // SettingsGeneralInstalledModsGroupCategory
    "Статус", // SettingsGeneralInstalledModsGroupStatus
    "Нет", // SettingsGeneralInstalledModsGroupNone
    "Список", // SettingsGeneralInstalledModsLayoutList
    "Папки", // SettingsGeneralInstalledModsLayoutFolders
    "Сначала сортировать по категории", // SettingsGeneralInstalledModsSortByCategoryFirst
    "Сортировка по порядку категорий, не обязательно алфавитному.", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
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
    "Нет и скрыть кнопку «Обновить»", // SettingsGeneralOperationalNoAndHideUpdateButton
    "При установке уже существующего мода:", // SettingsGeneralOperationalWhenInstallingExistingMod
    "Всегда спрашивать", // SettingsGeneralOperationalAlwaysAsk
    "Всегда заменять", // SettingsGeneralOperationalAlwaysReplace
    "Всегда объединять", // SettingsGeneralOperationalAlwaysMerge
    "Всегда оставлять оба", // SettingsGeneralOperationalAlwaysKeepBoth
    "При обновлении модов всегда заменять", // SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods
    "При удалении мода:", // SettingsGeneralOperationalWhenDeletingMod
    "Переместить в корзину", // SettingsGeneralOperationalMoveToRecycleBin
    "Удалить безвозвратно", // SettingsGeneralOperationalDeletePermanently

    // Window: Settings > General > Tasks
    "Задачи", // SettingsGeneralTasksSection
    "Раскладка задач:", // SettingsGeneralTasksLayout
    "Разделы", // SettingsGeneralTasksLayoutSections
    "Вкладки", // SettingsGeneralTasksLayoutTabbed
    "Единый список", // SettingsGeneralTasksLayoutSingleList
    "Очищать завершённые задачи:", // SettingsGeneralTasksClearCompletedTasks
    "Очистить задачи", // SettingsGeneralTasksClearTasks
    "Порядок задач:", // SettingsGeneralTasksOrder
    "Старые → Новые", // SettingsGeneralTasksOldestToNewest
    "Новые → Старые", // SettingsGeneralTasksNewestToOldest

    // Window: Settings > Category
    "Выберите игру для настройки категорий.", // SettingsCategorySelectGame
    "Обзор", // SettingsCategoryBrowseSection
    "Автоматически создавать категории GameBanana для загруженных модов", // SettingsCategoryAutoCreateGameBananaCategories
    "Применяется к {game}.", // SettingsCategoryAppliesToGame
    "Категории", // SettingsCategoryCategoriesSection
    "Выбрать все категории", // SettingsCategorySelectAllCategories
    "Снять выбор со всех категорий", // SettingsCategoryUnselectAllCategories
    "Создать", // SettingsCategoryNew
    "Новая категория (Ctrl+N)", // SettingsCategoryNewTooltip
    "Удалить", // SettingsCategoryDelete
    "Без категории", // SettingsCategoryUncategorized

    // Window: Settings > Game & Path
    "Проблемы с путями?", // SettingsPathScanTitle
    "Hestia может выполнить глубокое сканирование для обнаружения путей XXMI и поддерживаемых игр", // SettingsPathScanDescription
    "Сканировать пути", // SettingsPathScanButtonScan
    "Сканирование…", // SettingsPathScanButtonScanning
    "Сканирует доступные диски в поисках XXMI и исполняемых файлов игр.", // SettingsPathScanButtonTooltip
    "XXMI", // SettingsPathXxmiSection
    "Лаунчер XXMI:", // SettingsPathXxmiLauncher
    "Путь не найден", // SettingsPathPathNotFound
    "Использовать путь модов XXMI по умолчанию для игр", // SettingsPathUseDefaultXxmiModPath
    "Игра", // SettingsPathGameSection
    "EXE-файл игры:", // SettingsPathGameExeFile
    "Папка модов {code}:", // SettingsPathGameModsFolder

    // Window: Settings > Advanced > Appearance
    "Внешний вид", // SettingsAdvancedAppearanceSection
    "Язык:", // SettingsAdvancedAppearanceLanguage
    "Стиль шрифта:", // SettingsAdvancedAppearanceFontStyle
    "Классический", // SettingsAdvancedAppearanceFontClassic
    "Современный", // SettingsAdvancedAppearanceFontModern
    "Использует шрифт Segoe UI", // SettingsAdvancedAppearanceFontClassicTooltip
    "Использует шрифт Selawik", // SettingsAdvancedAppearanceFontModernTooltip

    // Window: Settings > Advanced > Content Restriction
    "Ограничение контента", // SettingsAdvancedContentRestrictionSection
    "Скрывать небезопасный контент:", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "Скрывать NSFW-моды и счётчик", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "Скрывать NSFW-моды, показывать счётчик", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "Показывать изображения с цензурой", // SettingsAdvancedContentRestrictionShowImagesCensored
    "Показывать без ограничений", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Cache and Archive
    "Кэш и архив", // SettingsAdvancedCacheArchiveSection
    "Размер кэша:", // SettingsAdvancedCacheArchiveCacheSize
    "Использование: {gb} ГБ", // SettingsAdvancedCacheArchiveCurrentUsage
    "Очистить кэш", // SettingsAdvancedCacheArchiveClearCache
    "Кэш очищен", // SettingsAdvancedCacheArchiveCacheCleared
    "Не удалось очистить кэш", // SettingsAdvancedCacheArchiveClearCacheFailed
    "Использование архива: {gb} ГБ", // SettingsAdvancedCacheArchiveArchiveUsage
    "Удалить архивные моды", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "Перемещено в корзину", // SettingsAdvancedCacheArchiveRecycled
    "Удалено", // SettingsAdvancedCacheArchiveDeleted
    "Архивных модов: {count}", // SettingsAdvancedCacheArchiveArchivedMods
    "Архив очищен: {count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "Нет архивов для очистки", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "Не удалось очистить архив", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "автор: {authors}", // SettingsAboutBy
    "Версия:", // SettingsAboutVersion
    "Нажмите, чтобы открыть «Что нового».", // SettingsAboutVersionTooltip
    "Автоматически проверять обновления", // SettingsAboutAutomaticallyCheckForUpdate
    "Проверка…", // SettingsAboutUpdateChecking
    "Перезапустить для обновления", // SettingsAboutUpdateRestartToUpdate
    "Проверить обновление", // SettingsAboutUpdateCheckForUpdate
    "Актуально", // SettingsAboutUpdateUpToDate
    "Ошибка проверки", // SettingsAboutUpdateFailedToCheck
    "Требуется ручное обновление", // SettingsAboutUpdateManualRequired
    "Доступно обновление", // SettingsAboutUpdateAvailable
    "Обновление готово", // SettingsAboutUpdateReady
    "Ошибка обновления", // SettingsAboutUpdateFailed
    "Загрузка обновления отменена", // SettingsAboutUpdateDownloadCanceled
    "Дождитесь завершения активных задач перед обновлением", // SettingsAboutUpdateWaitForActiveTasks
    "Не удалось применить обновление", // SettingsAboutUpdateCouldNotApply
    "Hestia установлена в папку, которую этот процесс не может обновить:\n{path}\nПереместите Hestia в другую папку и попробуйте снова или обновите установку из процесса с повышенными правами.", // SettingsAboutUpdateManualInstallFolder
    "Атрибуция", // SettingsAboutAttributionSection
    "Источник данных: GameBanana, API используется с разрешения. Метаданные модов, медиа и данные обзора GameBanana получены из GameBanana.", // SettingsAboutAttributionGameBanana

    // Translation
    "Ошибка перевода", // TranslationFailed
    "Идёт перевод", // TranslationInProgress
];
