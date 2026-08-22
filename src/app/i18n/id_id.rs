const ID_ID: [&str; TEXT_KEY_COUNT] = [
    // Window: What's New
    "Yang Baru", // WhatsNewWindowTitle
    "Klik untuk menampilkan survei masukan", // WhatsNewFeedbackSurveyTooltip

    // Window: Feedback Survey
    "Opsional", // FeedbackSurveyOptional
    "Mengirim...", // FeedbackSurveySubmitting
    "Kirim Masukan", // FeedbackSurveySubmitFeedback
    "Tutup", // FeedbackSurveyDismiss
    "Ingatkan nanti", // FeedbackSurveyRemindLater
    "Lewati versi ini", // FeedbackSurveySkipVersion
    "Jangan tanyakan lagi", // FeedbackSurveyNeverAskAgain
    "Detail privasi", // FeedbackSurveyPrivacyDetails
    "Masukan dikirim secara anonim.\nTidak ada cara untuk mengidentifikasi atau menghubungi pengirim.\nKiriman dapat dipublikasikan, tetapi pesan tetap privat.\nHanya payload data berikut yang akan dikirim ke server survei:", // FeedbackSurveyPrivacyCopy
    "• Client: Hash Sha256 dari UUID acak di file hestia.toml\n• URL Server & Database: {server_url}\n• Lokasi Server: Asia Pacific", // FeedbackSurveyPrivacyPayload
    "Lihat hasil survei di sini:", // FeedbackSurveyResultsHeader
    "• Berjalan: ", // FeedbackSurveyResultsOngoing
    "• Sebelumnya: ", // FeedbackSurveyResultsPrevious

    // Window: Log
    "Log", // LogWindowTitle
    "Log disalin", // LogCopied

    // Window: Tasks
    "Unduhan", // TasksWindowTitle
    "Berjalan", // TasksOngoing
    "Berjalan ({count})", // TasksOngoingCount
    "Tidak ada tugas aktif", // TasksNoActiveTasks
    "Selesai", // TasksCompleted
    "Selesai ({count})", // TasksCompletedCount
    "Tidak ada tugas selesai", // TasksNoCompletedTasks
    "Unduhan", // TasksDownloads
    "Unduhan ({count})", // TasksDownloadsCount
    "Pemasangan", // TasksInstalls
    "Pemasangan ({count})", // TasksInstallsCount
    "Gagal", // TasksFailed
    "Gagal ({count})", // TasksFailedCount
    "Tidak ada tugas", // TasksNoTasks
    "Dalam antrean", // TasksStatusQueued
    "Memasang", // TasksStatusInstalling
    "Mengunduh", // TasksStatusDownloading
    "Membatalkan", // TasksStatusCanceling
    "Selesai", // TasksStatusCompleted
    "Gagal", // TasksStatusFailed
    "Dibatalkan", // TasksStatusCanceled
    "Membatalkan…", // TasksCanceling
    "Batal", // TasksCancel
    "Coba Lagi", // TasksRetry
    "Lanjutkan", // TasksResume
    "Memulai unduhan…", // TasksStartingDownload
    "Dalam antrean…", // TasksQueuedProgress
    "Memasang file mod…", // TasksInstallingModFiles
    "Membatalkan tugas…", // TasksCancelingTask

    // Window: Tools
    "Alat", // ToolsWindowTitle
    "Tambahkan pintasan ke alat eksternal untuk game ini, lalu jalankan dari Hestia. Atur opsi peluncuran khusus bila diperlukan, dan sematkan alat yang sering digunakan ke titlebar.", // ToolsDescription
    "Tidak ada game yang dipilih", // ToolsNoGameSelected
    "Jalankan", // ToolsLaunch
    "Atur opsi peluncuran", // ToolsSetLaunchOptions
    "Buka Folder", // ToolsOpenFolder
    "Lepas dari Titlebar", // ToolsUnpinFromTitlebar
    "Sematkan ke Titlebar", // ToolsPinToTitlebar
    "Hapus", // ToolsRemove
    "Tambah Alat", // ToolsAddTool
    "Alat", // ToolsFallbackLabel
    "Tidak ada game yang dipilih untuk menambah alat", // ToolsNoGameSelectedForAdd
    "Alat sudah ditambahkan", // ToolsAlreadyAdded
    "Alat ditambahkan", // ToolsToolAdded
    "Alat dihapus", // ToolsToolRemoved
    "Maksimal 4 alat dapat ditampilkan di titlebar untuk satu game", // ToolsTitlebarLimit
    "Batas alat titlebar tercapai", // ToolsTitlebarLimitReached
    "File alat hilang", // ToolsExecutableMissing
    "alat tidak ditemukan: {path}", // ToolsNotFound
    "Menjalankan alat: {tool}", // ToolsLaunched
    "Tidak dapat menjalankan alat", // ToolsCouldNotLaunch
    "Tidak dapat membuka lokasi", // ToolsCouldNotOpenLocation
    "Opsi peluncuran alat disimpan", // ToolsLaunchOptionsSaved
    "Alat Ditambahkan", // ToolsActionAdded
    "Alat Dihapus", // ToolsActionRemoved
    "Alat Dijalankan", // ToolsActionLaunched
    "Baru", // ToolsNewBadge
    "{mod} menyertakan alat: {tool}", // ToolsInstalledModIncludesTool
    "{mod} menyertakan {count} alat", // ToolsInstalledModIncludesTools
    "Buka Alat", // ToolsOpenToolsAction

    // Window: Tool Launch Options
    "Atur Opsi Peluncuran", // ToolLaunchOptionsWindowTitle
    "Opsi peluncuran (mis. -option value -flag)", // ToolLaunchOptionsHint
    "Simpan", // ToolLaunchOptionsSave
    "Batal", // ToolLaunchOptionsCancel

    // Window: Dialogs
    "Memindai path...", // DialogScanningPaths
    "Pindai Game", // DialogFindingPaths
    "Hestia sedang memindai drive yang dapat diakses untuk mencari instalasi game yang didukung.", // DialogDeepScanningPaths
    "Hasil Pemindaian", // DialogScanResults
    "Lanjutkan", // DialogContinue
    "Hentikan Pemindaian", // DialogStopScan
    "Menghentikan pemindaian…", // DialogStoppingScan
    "Pemindaian dihentikan.", // DialogScanStopped
    "Pemindaian selesai.", // DialogScanCompleted
    "Dihentikan", // DialogStopped
    "Tidak ditemukan", // DialogNotFound
    "Mencari...", // DialogSearching
    "Ditemukan", // DialogFound
    "Pilih...", // DialogChoose
    "Beberapa ditemukan", // DialogMultipleFound
    "Mod Impor", // DialogImportedMod
    "Tidak Ada .ini", // DialogMissingIniTitle
    "Tidak ada file .ini yang dikenali di path induk arsip; arsip mungkin berisi beberapa mod.\nPilih folder yang ingin dipasang:", // DialogMissingIniPrompt
    "Pasang", // DialogInstall
    "Pasang Gabungan", // DialogInstallMerged
    "Pasang folder terpilih ke folder mod yang sama dan anggap sebagai satu mod", // DialogInstallMergedTooltip
    "Pasang Terpisah", // DialogInstallSeparately
    "Pasang folder terpilih ke folder mod masing-masing", // DialogInstallSeparatelyTooltip
    "Pemasangan gagal", // DialogInstallFailed
    "Pemasangan tidak tersedia", // DialogInstallUnavailable
    "Pemasangan gagal untuk {name}: {error}", // DialogInstallFailedFor
    "Pemeriksaan pemasangan gagal untuk {name}: {error}", // DialogInstallInspectionFailed
    "Gagal mengirim pemasangan untuk {name}", // DialogInstallDispatchFailed
    "Gagal memulai pemasangan untuk {name}: {error}", // DialogInstallStartFailed
    "Pilih game terlebih dahulu.", // DialogSelectGameFirst
    "Tidak ada folder yang dipilih", // DialogNoFoldersSelected
    "Pemasangan dibatalkan", // DialogInstallCanceled
    "Pemasangan dibatalkan: {name}", // DialogInstallCanceledMessage
    "Konflik Pemasangan", // DialogInstallationConflict
    "folder ini", // DialogThisFolder
    "Sudah ada di:", // DialogAlreadyExistsIn
    "Ganti", // DialogReplace
    "Gabungkan", // DialogMerge
    "Simpan Keduanya", // DialogKeepBoth
    "Konflik (Ganti)", // DialogConflictReplace
    "Konflik (Gabungkan)", // DialogConflictMerge
    "Konflik (Simpan Keduanya)", // DialogConflictKeepBoth
    "Konflik (Batal)", // DialogConflictCancel
    "Lepas mod untuk memasangnya\n\natau\n\nlepas gambar untuk menambahkannya ke:\n{name}", // DialogDropModsImages
    "Lepas untuk memasang", // DialogDropToInstall
    "Tidak didukung", // DialogUnsupported
    "Tidak didukung: {file}", // DialogUnsupportedFile
    "file", // DialogFile
    "Arsip", // DialogFileFilterArchives
    "Executable", // DialogFileFilterExecutable
    "Semua file", // DialogFileFilterAllFiles
    "Buka detail mod yang belum tertaut terlebih dahulu", // DialogOpenUnlinkedModDetailFirst
    "Memasang: {count} mod", // DialogInstallingCount
    "Tidak dapat membuat folder mod", // DialogCouldNotCreateModsFolder
    "Tidak dapat menonaktifkan mod terpasang", // DialogCouldNotDisableInstalledMod
    "Tidak dapat mempertahankan mod tetap nonaktif", // DialogCouldNotKeepModDisabled
    "Terpasang", // DialogInstalledAction
    "{count} mod terpasang", // DialogInstalledCount
    "Terpasang: {name}", // DialogInstalledName
    "Disinkronkan", // DialogSyncedAction
    "Pembaruan tidak tersedia", // DialogUpdateUnavailable
    "Memperbarui: {title}", // DialogUpdatingTask

    // Main GUI: App Messages
    "Tidak dapat menyimpan setelan", // AppCouldNotSaveSettings
    "Tidak dapat menyimpan data", // AppCouldNotSaveData
    "Peringatan: {detail}", // AppLogWarn
    "Error: {detail}", // AppLogError
    "Gagal menjalankan", // AppLaunchFailed
    "Path peluncuran belum diatur", // AppLaunchPathNotSet
    "Game belum dipilih", // AppGameNotSelected
    "Main (dengan mod)", // AppPlayModded
    "Main (tanpa mod)", // AppPlayVanilla
    "Dengan mod", // AppModded
    "Tanpa mod", // AppVanilla
    "Path {label} belum diatur untuk {game}", // AppLaunchPathNotSetForGame
    "Menjalankan {game}", // AppLaunchedGame
    "Menjalankan {game} ({mode})", // AppLaunchedGameMode
    "Tidak ada survei masukan yang dikonfigurasi untuk versi ini.", // AppNoFeedbackSurveyConfigured
    "Menambahkan gambar dari clipboard...", // AppAddingClipboardImage
    "Tidak dapat menempel gambar", // AppCouldNotPasteImage
    "Tidak dapat melampirkan gambar", // AppCouldNotAttachImages
    "Tidak dapat menyimpan gambar", // AppCouldNotSaveImages
    "Gambar Ditambahkan", // AppImagesAddedAction
    "{count} gambar ditambahkan", // AppImagesAdded
    "Tidak dapat menambahkan gambar", // AppCouldNotAddImages
    "Tonton Pratinjau", // AppWatchPreview
    "Tidak dapat membuka peramban", // AppCouldNotOpenBrowser
    "Tidak dapat memuat ulang mod", // AppCouldNotRefreshMods
    "{count} mod dipindai, tidak ada perubahan", // AppModsScannedNoChanges
    "Dimuat ulang: {count} mod, tidak ada perubahan", // AppReloadedNoChanges
    "{count} mod dipindai", // AppModsScanned
    "Dimuat ulang: {count} mod", // AppReloaded
    "{count} ditambahkan", // AppReloadAdded
    "{count} dihapus", // AppReloadRemoved
    "{count} berubah", // AppReloadChanged
    "Muat ulang: {line}", // AppReloadAction
    "Kategori", // AppCategoryAction
    "Membuat \"{category}\"", // AppCategoryCreated
    "{mod} tidak memiliki kategori GameBanana yang valid; pembuatan kategori dilewati", // AppCategorySkippedNoValidGameBananaCategory
    "Survei", // AppSurveyAction
    "Payload masukan tertunda yang tidak dapat dibaca dibuang: {error}", // AppSurveyDiscardedUnreadablePendingFeedbackPayload
    "Mencoba ulang payload masukan tertunda", // AppSurveyRetryingPendingFeedbackPayload
    "Masukan untuk {version} terkirim", // AppSurveySubmittedFeedback
    "Pengiriman masukan gagal untuk {version}: {error}", // AppSurveyFeedbackSubmitFailed
    "Payload masukan tertunda untuk {version} dibuang", // AppSurveyDiscardedPendingFeedbackPayload
    "Tidak dapat mengirim masukan", // AppCouldNotSubmitFeedback
    "Masukan terkirim", // AppFeedbackSubmitted
    "Unduhan dibatalkan: {title}", // AppDownloadCanceled

    // Main GUI: Chrome
    "\nManajer Mod", // ChromeAppSubtitle
    "Main", // ChromePlay
    "Pasang\nZip/Rar", // ChromeInstallArchive
    "Pasang\nFolder", // ChromeInstallFolder
    "Folder\nMod", // ChromeOpenModsFolder
    "Muat\nUlang", // ChromeReload
    "Game tidak terpasang atau belum diatur.", // ChromeGameNotInstalled
    "Jalankan game dengan mod lewat XXMI", // ChromeLaunchWithModsTooltip
    "Jalankan game tanpa mod", // ChromeLaunchWithoutModsTooltip
    "Main dengan mod", // ChromePlayWithMods
    "Main tanpa mod", // ChromePlayWithoutMods
    "Pasang mod dari arsip zip/rar/7z", // ChromeInstallArchiveTooltip
    "Pasang mod dari folder yang sudah diekstrak", // ChromeInstallFolderTooltip
    "Buka folder mod untuk game yang dipilih", // ChromeOpenModsFolderTooltip
    "Pasang", // ChromeInstall
    "Pasang & Nonaktifkan", // ChromeInstallDisabled
    "Pindai ulang mod terpasang dan periksa pembaruan di GameBanana (Ctrl+R)", // ChromeReloadLibraryTooltip
    "Muat ulang daftar saat ini (Ctrl+R)", // ChromeReloadBrowseTooltip
    "Tutup", // ChromeClose
    "Pulihkan", // ChromeRestore
    "Maksimalkan", // ChromeMaximize
    "Minimalkan", // ChromeMinimize
    "Mod Saya", // ChromeMyMods
    "Jelajah", // ChromeBrowse
    "Alat (Ctrl+T)", // ChromeToolsTooltip
    "Unduhan (Ctrl+J)", // ChromeTasksTooltip
    "Log (Ctrl+L)", // ChromeLogTooltip
    "Setelan (Ctrl+P)", // ChromeSettingsTooltip
    "Belum Ada Game yang Diaktifkan", // ChromeNoGamesDetected
    "Lihat Setelan → Game", // ChromeSeeSettingsGames

    // Main GUI: Browse
    "Temukan mod di GameBanana...", // BrowseSearchHint
    "Mod GameBanana", // BrowseModsTitle
    "Karakter", // BrowseCharacters
    "Populer", // BrowsePopular
    "Terbaru", // BrowseRecentUpdated
    "Paling Sesuai", // BrowseBestMatch
    "{count} mod", // BrowseModsCount
    "Memuat…", // BrowseLoading
    "{count} disembunyikan karena NSFW", // BrowseHiddenNsfwCount
    "{count} mod", // BrowseSelectedCharacterModsCount
    "Tampilkan semua mod", // BrowseShowAllMods
    "Mengambil mod dari GameBanana…", // BrowseFetchingMods
    "Terpasang", // BrowseInstalled
    "Buka di Peramban", // BrowseOpenInBrowser
    "Tidak dapat membuka peramban", // BrowseCouldNotOpenBrowser
    "Memuat lagi…", // BrowseLoadingMore
    "Daftar karakter belum diatur untuk game ini.", // BrowseNoCharacterList
    "Muat ulang karakter", // BrowseRefreshCharacters
    "Bersihkan filter ini", // BrowseClearFilter
    "Dipilih: {name}", // BrowseSelectedCharacter
    "{count} karakter", // BrowseCharacterCount
    "Menunggu", // BrowseWaiting
    "Tidak ada karakter yang dikembalikan oleh GameBanana.", // BrowseNoCharactersReturned
    "Detail Mod", // BrowseModDetail
    "Salin ID GameBanana", // BrowseCopyGameBananaId
    "ID GameBanana disalin", // BrowseGameBananaIdCopied
    "Tidak diketahui", // BrowseUnknown
    "Pembaruan", // BrowseUpdates
    "Mod ini privat.", // BrowsePrivateMod
    "Instalasi otomatis dinonaktifkan. Anda mungkin dapat melihat atau mengunduhnya langsung di GameBanana jika punya izin.", // BrowseAutomaticInstallDisabledAuthorized
    "Mod ini sedang ditahan", // BrowseWithheldMod
    "Ditahan oleh", // BrowseWithheldBy
    "Instalasi otomatis dinonaktifkan sampai penahanan diselesaikan.", // BrowseAutomaticInstallDisabledWithheld
    "Pelanggaran aturan", // BrowseRuleViolation
    "Mod ini sudah tidak ada.", // BrowseDeletedModNoLongerExists
    "Mod ini telah dihapus oleh", // BrowseDeletedBy
    "Mod ini telah dihapus", // BrowseDeleted
    "File", // BrowseFiles
    "File Arsip", // BrowseArchivedFiles
    "Memuat detail mod…", // BrowseLoadingDetails
    "{size} • {date} • {downloads} unduhan", // BrowseFileMetadata
    "Pilih File", // BrowseChooseFiles
    "Mod ini memiliki beberapa file yang tersedia.\nPilih file untuk diunduh dan dipasang:", // BrowseMultipleFilesPrompt
    "Game ini tidak memiliki daftar kategori karakter GameBanana yang diatur.", // BrowseNoConfiguredCharacterCategoryList
    "Karakter tidak tersedia", // BrowseCharactersUnavailable
    "Koneksi gagal", // BrowseConnectionFailed
    "Waktu koneksi habis", // BrowseConnectionTimedOut
    "Jelajah gagal", // BrowseFailed
    "Karakter gagal dimuat", // BrowseCharactersFailed
    "Detail jelajah gagal", // BrowseDetailFailed
    "Tidak dapat memuat pembaruan", // BrowseCouldNotLoadUpdates
    "Terunduh: {title}", // BrowseDownloaded
    "Tidak dapat menyiapkan pemasangan", // BrowseCouldNotPrepareInstall
    "Unduhan gagal", // BrowseDownloadFailed
    "Menyiapkan unduhan: {title}", // BrowseResolvingDownload
    "Tidak ada file yang dapat diunduh", // BrowseNoDownloadableFilesFound
    "Tidak ada file yang dipilih", // BrowseNoFilesSelected
    "Unduhan diantrekan", // BrowseDownloadQueued
    "Refresh halaman jelajah gagal; menggunakan hasil cache: {warning}", // BrowsePageWarning
    "Halaman jelajah gagal: {error}", // BrowsePageFailed
    "Refresh kategori karakter gagal; menggunakan hasil cache: {warning}", // BrowseCharacterCategoriesWarning
    "Kategori karakter gagal: {error}", // BrowseCharacterCategoriesFailed
    "Refresh detail jelajah gagal untuk mod {mod_id}; menggunakan detail cache: {warning}", // BrowseDetailWarning
    "Detail jelajah gagal untuk mod {mod_id}: {error}", // BrowseDetailFailedMessage
    "Refresh pembaruan jelajah gagal untuk mod {mod_id}; menggunakan pembaruan cache: {warning}", // BrowseUpdatesWarning
    "Pembaruan jelajah gagal untuk mod {mod_id}: {error}", // BrowseUpdatesFailedMessage
    "Unduhan gagal untuk {title}: {error}", // BrowseDownloadFailedMessage

    // Main GUI: My Mods
    "Memindai mod terpasang", // LibraryScanningInstalledMods
    "XXMI Launcher diperlukan", // LibraryEnsureXxmiInstalled
    "XXMI Launcher diperlukan sebelum Hestia dapat memasang atau mengunduh mod untuk game ini.", // LibraryInstallXxmiDescription
    "Pindai game yang terpasang atau aktifkan satu secara manual di setelan.", // LibrarySetupDescription
    "Unduh XXMI", // LibraryDownloadXxmi
    "Pindai Game", // LibraryFindGamesAndFixPaths
    "Temukan game yang didukung secara otomatis.", // LibraryPathScanDescription
    "Setelan Game", // LibraryGamesSettings
    "Aktifkan game dan atur path secara manual.", // LibraryGamesSettingsDescription
    "Signature bypasser belum terpasang", // LibraryNteBypasserMissingTitle
    "Mod NTE tidak akan dimuat sampai AyakaNTEModLoader.asi atau UniversalSigBypasser.asi terpasang.", // LibraryNteBypasserMissingDescription
    "AyakaNTEBypasser", // LibraryNteBypasserAyaka
    "UniversalSigBypasser", // LibraryNteBypasserUniversal
    "Hestia tidak dapat mengubah folder game ini", // LibraryProtectedPathTitle
    "Game ini terpasang di lokasi yang dilindungi ({path}), sehingga Windows memblokir Hestia untuk memasang, menonaktifkan, atau mengganti mod di sini. Klik Beri akses agar akun Windows kamu dapat mengubah folder game, ini hanya memerlukan persetujuan administrator satu kali. Menjalankan ulang sebagai admin juga bisa, tetapi harus diulang setiap kali dibuka dan drag-and-drop dari Explorer tidak akan berfungsi selama berjalan sebagai admin.", // LibraryProtectedPathDescription
    "Beri akses", // LibraryGrantAccess
    "Mulai ulang sebagai admin", // LibraryRestartAsAdmin
    "Filter nama mod...", // LibrarySearchHint
    "Mod Terpasang", // LibraryInstalledMods
    "{count} dipilih", // LibrarySelectedCount
    "Pilih semua mod yang terlihat", // LibrarySelectAllVisibleMods
    "{count} mod", // LibraryModsCount
    "1 mod", // LibraryOneMod
    "Kembali", // LibraryBack
    "Kembali ke folder kategori", // LibraryBackToCategoryFolders
    "{active} aktif • {disabled} nonaktif • {archived} arsip", // LibraryCategorySummary
    "Nama A-Z", // LibrarySortNameAsc
    "Nama Z-A", // LibrarySortNameDesc
    "Terbaru → Terlama", // LibrarySortDateDesc
    "Terlama → Terbaru", // LibrarySortDateAsc
    "Ukuran Terkecil → Terbesar", // LibrarySortSizeAsc
    "Ukuran Terbesar → Terkecil", // LibrarySortSizeDesc
    "Urutkan, kelompokkan, dan atur tampilan mod terpasang", // LibrarySortMenuTooltip
    "Urutan Mod", // LibrarySortModsHeading
    "Mengurutkan berdasarkan judul mod, lalu nama folder jika tidak ada", // LibrarySortNameTooltip
    "Menggunakan timestamp pemasangan, konten, atau refresh terbaru yang diketahui", // LibrarySortNewestTooltip
    "Menggunakan timestamp pemasangan, konten, atau refresh terlama yang diketahui terlebih dahulu", // LibrarySortOldestTooltip
    "Mengurutkan berdasarkan total ukuran konten mod", // LibrarySortSizeTooltip
    "Kelompokkan Mod", // LibraryGroupModsHeading
    "Mengelompokkan mod berdasarkan kategori per game", // LibraryGroupCategoryTooltip
    "Mengelompokkan mod ke bagian Aktif, Dinonaktifkan, dan Diarsipkan", // LibraryGroupStatusTooltip
    "Menampilkan satu daftar mod berurutan tanpa grup", // LibraryGroupNoneTooltip
    "Tampilan Kategori", // LibraryCategoryLayoutHeading
    "Tersedia saat dikelompokkan berdasarkan kategori.", // LibraryAvailableWhenGroupedByCategory
    "Menampilkan tile kategori lebih dulu, lalu membuka satu kategori pada satu waktu", // LibraryCategoryFoldersTooltip
    "Menampilkan setiap kategori sebagai bagian dalam daftar mod", // LibraryCategoryListTooltip
    "Urutan Kategori", // LibrarySortCategoriesHeading
    "Manual", // LibraryCategorySortManual
    "Nama A-Z", // LibraryCategorySortByNameAsc
    "Mod Paling Sedikit", // LibraryCategorySortByLeastMods
    "Mod Paling Banyak", // LibraryCategorySortByMostMods
    "Menggunakan urutan kategori manual Anda", // LibraryCategorySortManualTooltip
    "Mengurutkan folder dan bagian kategori berdasarkan nama kategori", // LibraryCategorySortByNameTooltip
    "Menampilkan kategori dengan mod terbanyak lebih dulu", // LibraryCategorySortByMostModsTooltip
    "Menampilkan kategori dengan mod tersedikit lebih dulu", // LibraryCategorySortByLeastModsTooltip
    "Lain-lain", // LibraryMiscellaneousHeading
    "Di dalam grup status, mengikuti urutan kategori sebelum urutan terpilih", // LibrarySortCategoryFirstTooltip
    "Menempatkan mod Aktif lebih dulu, lalu Dinonaktifkan, lalu Diarsipkan sebelum urutan terpilih", // LibrarySortStatusFirstTooltip
    "Tersedia saat dikelompokkan berdasarkan kategori dalam tampilan daftar", // LibraryUncategorizedFirstListOnlyTooltip
    "Alihkan Visibilitas", // LibraryToggleVisibility
    "Status Mod", // LibraryModStateHeading
    "Tampilkan semua status mod", // LibraryShowAllModStates
    "Sembunyikan semua status mod", // LibraryHideAllModStates
    "Mod aktif", // LibraryEnabledMods
    "Mod dinonaktifkan", // LibraryDisabledMods
    "Mod diarsipkan", // LibraryArchivedMods
    "Status Pembaruan", // LibraryUpdateStateHeading
    "Tampilkan semua status pembaruan", // LibraryShowAllUpdateStates
    "Sembunyikan semua status pembaruan", // LibraryHideAllUpdateStates
    "Tidak tertaut", // LibraryUnlinked
    "Sudah terbaru", // LibraryUpToDate
    "Pembaruan tersedia", // LibraryUpdateAvailable
    "Pemeriksaan dilewati", // LibraryCheckSkipped
    "Sumber hilang", // LibraryMissingSource
    "Dimodifikasi lokal", // LibraryModifiedLocally
    "Mengabaikan pembaruan", // LibraryIgnoringUpdate
    "Menampilkan mod yang mengabaikan pembaruan saat ini atau mengabaikan pembaruan sampai dimatikan", // LibraryIgnoringUpdateTooltip
    "Perbarui", // LibraryUpdate
    "Aktifkan", // LibraryEnable
    "Nonaktifkan", // LibraryDisable
    "Arsipkan", // LibraryArchive
    "Lainnya", // LibraryMore
    "(tidak ada)", // LibraryNone
    "Belum ada kategori.\n\n1. Klik kartu mod untuk membuka detailnya.\n2. Klik \"Tanpa kategori\" di bawah nama mod.\n3. Klik \"+ Kategori Baru\" lalu beri nama.", // LibraryNoCategoryHelp
    "Belum ada kategori.", // LibraryNoCategoryYet
    "Kategori Baru", // LibraryNewCategory
    "Buka", // LibraryOpen
    "Penjelajah Berkas", // LibraryFileExplorer
    "Tidak ada sumber GameBanana yang tertaut untuk mod ini.", // LibraryNoGameBananaSource
    "Abaikan sekali saja", // LibraryIgnoreUpdateOnce
    "Mengabaikan pembaruan yang tersedia saat ini. Jika belum ada pembaruan, versi saat ini akan diingat dan pembaruan berikutnya yang terdeteksi akan diabaikan.", // LibraryIgnoreUpdateOnceTooltip
    "Sinkronkan mod ini dengan GameBanana sebelum memakai abaikan sekali", // LibraryIgnoreUpdateOnceDisabledTooltip
    "Sinkronkan setidaknya satu mod terpilih dengan GameBanana sebelum memakai abaikan sekali", // LibraryIgnoreUpdateOnceBulkDisabledTooltip
    "Selalu abaikan", // LibraryIgnoreUpdateAlways
    "Mengatur status pembaruan mod ini menjadi \"Selalu Mengabaikan Pembaruan\" sampai dinonaktifkan", // LibraryIgnoreUpdateAlwaysTooltip
    "Tandai sebagai tidak dimodifikasi", // LibraryMarkAsNotModified
    "Memperlakukan file mod ini saat ini sebagai tidak dimodifikasi. Acuan pemasangan asli tetap disimpan, dan mengedit file setelah titik ini akan kembali ditandai sebagai dimodifikasi. Pembaruan akan menimpa perubahan ini.", // LibraryMarkAsNotModifiedTooltip
    "Pulihkan status modifikasi", // LibraryRestoreModificationStatus
    "Mengembalikan status Dimodifikasi dengan membandingkan kembali file mod ini terhadap acuan pemasangan asli", // LibraryRestoreModificationStatusTooltip
    "Dimodifikasi", // LibraryModified
    "\n(Dimodifikasi)", // LibraryModifiedSuffix
    "…dan {count} lainnya", // LibraryAndMore
    "Dimodifikasi & Abaikan Sekali", // LibraryModifiedIgnoringOnce
    "Dimodifikasi & Selalu Abaikan", // LibraryModifiedIgnoringAlways
    "Dimodifikasi & Pembaruan Tersedia", // LibraryModifiedUpdateAvailable
    "Abaikan Sekali", // LibraryIgnoringOnce
    "Selalu Abaikan", // LibraryIgnoringAlways
    "Hilang", // LibraryMissing
    "Dilewati", // LibrarySkipped
    "Kosong", // LibraryEmpty
    "Memindahkan", // LibraryMoving
    "Pindah ke sini", // LibraryMoveHere
    "Buka {item}", // LibraryOpenItem
    "Lepas ke kategori", // LibraryDropOnCategory
    "Urutkan ulang folder", // LibraryReorderFolder
    "Kategori", // LibraryCategoriesHeading
    "{folders} folder / {uncategorized} mod tanpa kategori", // LibraryFoldersUncategorizedSummary
    "Drop mengubah urutan menjadi Manual", // LibraryDropSwitchesToManualOrder
    "Ubah nama", // LibraryRename
    "Ubah nama (F2)", // LibraryRenameShortcut
    "Folder saja, pindahkan mod keluar", // LibraryFolderOnlyMoveModsOutside
    "Mod di dalam saja, pertahankan folder", // LibraryFolderModsInsideKeepFolder
    "Hanya menghapus mod yang ditampilkan. {count} mod yang tidak ditampilkan tetap di folder ini.", // LibraryFolderModsInsideKeepFolderHiddenTooltip
    "Folder dan mod di dalamnya", // LibraryFolderAndModsInside
    "{count} mod di folder ini tidak ditampilkan. Bersihkan pencarian/filter untuk menghapus semua isinya.", // LibraryFolderAndModsInsideHiddenTooltip
    "Folder dihapus: {category}", // LibraryDeletedFolder
    "Buat Kategori", // LibraryContextCreateCategory
    "Pilih Semua", // LibraryContextSelectAll
    "Hapus Pilihan", // LibraryContextClearSelection
    "Urutkan", // LibraryContextSortBy
    "Kelompokkan", // LibraryContextGroupBy
    "Buka Folder Mod", // LibraryContextOpenModsFolder
    "Dari Arsip .zip/.rar", // LibraryContextInstallArchive
    "Dari Folder", // LibraryContextInstallFolder
    "Kategori dibuat: {category}", // LibraryCreatedFolder
    "Aktif", // LibraryStatusActive
    "Nonaktif", // LibraryStatusDisabled
    "Arsip", // LibraryStatusArchived
    "Baru", // RelativeTimeNow
    "Hari ini", // RelativeTimeToday
    "{count}mnt", // RelativeTimeMinutes
    "{count}j", // RelativeTimeHours
    "{count}hr", // RelativeTimeDays
    "Dipindahkan ke Tempat Sampah", // LibraryRecycledAction
    "Dihapus", // LibraryDeletedAction
    "Gagal menghapus", // LibraryDeleteFailed
    "Gagal menonaktifkan", // LibraryDisableFailed
    "Gagal mengarsipkan", // LibraryArchiveFailed
    "Gagal mengaktifkan", // LibraryEnableFailed
    "Gagal memulihkan", // LibraryRestoreFailed
    "Dinonaktifkan", // LibraryActionDisabled
    "Diarsipkan", // LibraryActionArchived
    "Diaktifkan", // LibraryActionEnabled
    "Dikeluarkan dari arsip", // LibraryActionUnarchived
    "{action}: {name}", // LibraryActionMessage
    "{action} {count} mod", // LibraryActionCountMessage
    "{action} {category} dan {count} mod", // LibraryCategoryActionCountMessage
    "Pembaruan diantrekan untuk {count} mod", // LibraryQueuedUpdates
    "Mod saat ini terkunci, kemungkinan oleh game.", // LibraryModsLockedProbablyByGame
    "Melewati mod yang sedang terkunci, kemungkinan oleh game.", // LibrarySkippedLockedModsProbablyByGame
    "Gagal mengubah nama", // LibraryRenameFailed
    "Ubah nama", // LibraryActionRenamed
    "Ubah nama menjadi: {name}", // LibraryRenamedTo
    "Catatan Pribadi", // LibraryPersonalNote
    "Catatan pribadi disimpan", // LibrarySavedPersonalNote
    "Catatan pribadi dihapus", // LibraryPersonalNoteRemoved
    "Tidak dapat menyimpan catatan pribadi", // LibraryCouldNotSavePersonalNote
    "Hapus gambar", // LibraryRemoveImage
    "Klik di sini untuk", // LibraryClickHereTo
    "menambahkan gambar sendiri.", // LibraryManuallyAddImages
    "Anda juga bisa menarik gambar ke sini,", // LibraryDropImagesHere
    "atau tempel dari clipboard (CTRL + V).", // LibraryPasteFromClipboard
    "Menambahkan gambar...", // LibraryAddingImages
    "Tambah gambar", // LibraryAddImages
    "Gambar", // LibraryImagesFileDialog
    "Menambahkan {count} gambar", // LibraryAddingImagesCount
    "Tidak dapat menambahkan gambar", // LibraryCouldNotAddImages
    "Gambar dihapus", // LibraryImageRemoved
    "Tidak dapat menghapus gambar", // LibraryCouldNotRemoveImage
    "Membutuhkan RabbitFX", // LibraryRequiresRabbitFx
    "Deskripsi", // LibraryMetaSourceDescription
    "Deskripsi dari halaman GameBanana", // LibraryMetaSourceDescriptionGbTooltip
    "Deskripsi mod", // LibraryMetaSourceDescriptionTooltip
    "Tombol", // LibraryMetaSourceHotkeys
    "Bersumber dari file .ini mod", // LibraryMetaSourceHotkeysTooltip
    "Mod ini tidak memiliki hotkey", // LibraryMetaSourceHotkeysUnavailable
    "Daftar", // LibraryHotkeysViewList
    "Mentahan", // LibraryHotkeysViewRaw
    "Beralih ke tampilan mentah", // LibraryHotkeysSwitchToRaw
    "Beralih ke tampilan daftar", // LibraryHotkeysSwitchToList
    "Tidak ada tombol pengalih di mod ini.", // LibraryHotkeysNoToggleKeys
    "Hanya-baca", // LibraryHotkeysWriteBlockedLabel
    "Hanya-baca selama gim berjalan.\nUntuk mengeditnya secara langsung, aktifkan \"Izinkan Hestia mengubah konfigurasi d3dx.ini\" di\nSetelan > Umum > Operasional > Eksperimental, lalu mulai ulang gim.", // LibraryHotkeysWriteBlockedHint
    "Gim sedang berjalan!", // LibraryHotkeysRunningToast
    "Buatkan catatan pribadi", // LibraryAddPersonalNote
    "Simpan catatan pribadi", // LibrarySavePersonalNote
    "Catatan pengguna yang bisa diedit", // LibraryEditableUserNote
    "Edit catatan pribadi", // LibraryEditPersonalNote
    "+ Buat Catatan", // LibraryAddNote
    "Lokal", // LibraryLocal
    "Buka di Penjelajah Berkas", // LibraryOpenInFileExplorer
    "Sumber", // LibrarySource
    "• Terakhir disinkronkan: {age}", // LibraryLastSynced
    "Perbarui", // LibraryResync
    "Putus", // LibraryUnlink
    "Lihat di GameBanana", // LibraryGameBananaPage
    "Tautkan ke GameBanana agar bisa melacak pembaruan dan sinkronisasi metadata.", // LibraryLinkGameBananaPrompt
    "URL atau ID GameBanana", // LibraryUrlOrId
    "Sinkronkan Mod", // LibrarySyncMod
    "Preferensi Pembaruan:", // LibraryUpdatePreferences
    "Menyinkronkan dengan GameBanana…", // LibrarySyncingGameBanana

    // Window: Profiles
    "Profil", // ProfilesTitle
    "Profil", // ProfilesLabel
    "Standar", // ProfilesDefault
    "Profil baru", // ProfilesNew
    "Profil baru", // ProfilesCreateEmpty
    "Buat kumpulan mod dan kategori yang terpisah.", // ProfilesNewDescription
    "Duplikat profil", // ProfilesDuplicateCurrent
    "Buat salinan mod dan kategori dari profil saat ini.", // ProfilesDuplicateDescription
    "Buka folder profil", // ProfilesOpenFolder
    "Buka folder tempat profil gim ini disimpan", // ProfilesOpenFolderTooltip
    "Ubah nama profil", // ProfilesRename
    "Pilih nama baru untuk profil ini.", // ProfilesRenameDescription
    "Hapus profil", // ProfilesDelete
    "Nama profil", // ProfilesName
    "Masukkan nama profil.", // ProfilesNameEmpty
    "Profil bernama \"{name}\" sudah ada.", // ProfilesNameTaken
    "Ganti profil", // ProfilesSwitch
    "Mengganti profil…", // ProfilesSwitching
    "Membuat profil…", // ProfilesCreating
    "Menduplikat profil…", // ProfilesDuplicating
    "Menghapus profil…", // ProfilesDeleting
    "Mengarsipkan profil saat ini…", // ProfilesArchivingCurrent
    "Mengekstrak profil yang dipilih…", // ProfilesExtractingSelected
    "Menyalin profil yang dipilih…", // ProfilesCopyingSelected
    "Mengaktifkan profil yang dipilih…", // ProfilesActivatingSelected
    "Profil yang tidak aktif dikompresi untuk menghemat ruang disk.", // ProfilesInactiveCompressedNote
    "Status", // ProfilesStatusLabel
    "Profil aktif", // ProfilesStatusActive
    "Menunggu untuk dikompresi", // ProfilesStatusQueued
    "Sedang dikompresi", // ProfilesStatusRunning
    "Sudah dikompresi", // ProfilesStatusComplete
    "Kompresi gagal", // ProfilesStatusFailed
    "Belum dikompresi", // ProfilesStatusUnavailable
    "Ukuran arsip", // ProfilesArchiveSize
    "Ukuran arsip sebelumnya", // ProfilesPreviousArchiveSize
    "Belum ada arsip", // ProfilesNoArchiveYet
    "Ukuran", // ProfilesUncompressedSize
    "{size} (belum dikompresi)", // ProfilesUncompressedSizeValue
    "Operasi profil gagal", // ProfilesOperationFailed
    "File di {folder} sedang dibuka oleh {apps}, tutup dulu lalu coba lagi.", // ProfilesFilesInUse
    "File di {folder} sedang dibuka oleh program lain, tutup dulu lalu coba lagi.", // ProfilesFilesInUseUnknown
    " dan {count} lainnya", // ProfilesFilesInUseMore
    "Aksi dijeda:", // ProfilesActionsPausedLabel
    "tugas lain sedang berjalan", // ProfilesActionsPausedFallback
    "operasi profil sedang berjalan", // ProfilesActionsPausedProfileOperation
    "memperbarui daftar mod", // ProfilesActionsPausedRefreshingLibrary
    "memasang mod", // ProfilesActionsPausedInstallingMods
    "memeriksa pembaruan", // ProfilesActionsPausedCheckingUpdates
    "memperbarui Hestia", // ProfilesActionsPausedUpdatingHestia
    "game sedang berjalan", // ProfilesActionsPausedGameRunning
    "mod terkunci", // ProfilesActionsPausedModsLocked
    "alat profil sedang berjalan", // ProfilesActionsPausedProfileToolRunning
    "Pilih game untuk mengelola profil.", // ProfilesSelectGame
    "Diperlukan setidaknya satu profil.", // ProfilesAtLeastOneRequired
    "Ganti ke profil lain sebelum menghapus profil ini.", // ProfilesSwitchBeforeDelete
    "Hapus profil \"{name}\"?", // ProfilesDeleteConfirmation
    "Mod dalam profil ini akan dihapus secara PERMANEN. Data yang terhapus tidak dapat dikembalikan.", // ProfilesDeleteConfirmationDetails
    "Profil dibuat: {name}", // ProfilesCreated
    "Profil diduplikatkan: {name}", // ProfilesDuplicated
    "Nama profil diubah: {name}", // ProfilesRenamed
    "Profil dihapus: {name}", // ProfilesDeleted
    "Profil diaktifkan: {name}", // ProfilesActivated
    "Operasi profil dibatalkan", // ProfilesCanceled
    "Profil dipulihkan", // ProfilesActionRecovered
    "Memulihkan {count} profil yang ada di penyimpanan tetapi hilang dari daftar profil.", // ProfilesRecovered

    // Window: Settings
    "Setelan", // SettingsWindowTitle
    "Umum", // SettingsTabGeneral
    "Kategori", // SettingsTabCategory
    "Lanjutan", // SettingsTabAdvanced
    "Game", // SettingsTabGames
    "Tentang", // SettingsTabAbout

    // Window: Settings > General > Behavior
    "Perilaku", // SettingsGeneralBehaviorSection
    "Saat menjalankan game:", // SettingsGeneralBehaviorWhenLaunchingGame
    "Setelah memasang mod:", // SettingsGeneralBehaviorAfterInstallingMod
    "Saat menjalankan alat:", // SettingsGeneralBehaviorWhenLaunchingTool
    "Jangan lakukan apa-apa", // SettingsGeneralBehaviorDoNothing
    "Minimalkan Hestia", // SettingsGeneralBehaviorMinimizeHestia
    "Keluar dari Hestia", // SettingsGeneralBehaviorExitHestia
    "Tambahkan ke pilihan", // SettingsGeneralBehaviorAddToSelection
    "Buka detail mod", // SettingsGeneralBehaviorOpenModDetail

    // Window: Settings > General > Installed Mods List
    "Daftar Mod Terpasang", // SettingsGeneralInstalledModsListSection
    "Bagi berdasarkan:", // SettingsGeneralInstalledModsGroupListBy
    "Tampilan kategori:", // SettingsGeneralInstalledModsCategoryLayout
    "Kategori", // SettingsGeneralInstalledModsGroupCategory
    "Status", // SettingsGeneralInstalledModsGroupStatus
    "Tidak ada", // SettingsGeneralInstalledModsGroupNone
    "Daftar", // SettingsGeneralInstalledModsLayoutList
    "Folder", // SettingsGeneralInstalledModsLayoutFolders
    "Urutkan kategori lebih dulu", // SettingsGeneralInstalledModsSortByCategoryFirst
    "Mengurutkan berdasarkan urutan kategori, tidak selalu alfabetis", // SettingsGeneralInstalledModsSortByCategoryFirstTooltip
    "Urutkan status lebih dulu", // SettingsGeneralInstalledModsSortByStatusFirst
    "Mengurutkan mod Aktif terlebih dahulu, lalu Dinonaktifkan, lalu Diarsipkan", // SettingsGeneralInstalledModsSortByStatusFirstTooltip
    "Tampilkan status mod di kartu", // SettingsGeneralInstalledModsShowModStatusOnCard
    "Tampilkan kategori di kartu", // SettingsGeneralInstalledModsShowCategoryOnCard
    "Status mod tetap ditampilkan lewat titik status berwarna", // SettingsGeneralInstalledModsShowCategoryOnCardTooltip
    "Tampilkan mod yang dinonaktifkan", // SettingsGeneralInstalledModsShowDisabledMods
    "Tampilkan mod yang diarsipkan", // SettingsGeneralInstalledModsShowArchivedMods
    "Tampilkan mod tanpa kategori lebih dulu", // SettingsGeneralInstalledModsShowUncategorizedModsFirst
    "Tampilkan folder kategori kosong", // SettingsGeneralInstalledModsShowEmptyCategoryFolders

    // Window: Settings > General > Operational
    "Operasional", // SettingsGeneralOperationalSection
    "Mod yang diperiksa pembaruannya:", // SettingsGeneralOperationalModsToCheckForUpdates
    "Perbarui mod secara otomatis:", // SettingsGeneralOperationalAutomaticallyUpdateMods
    "Aktif", // SettingsGeneralOperationalStatusActive
    "Nonaktif", // SettingsGeneralOperationalStatusDisabled
    "Arsip", // SettingsGeneralOperationalStatusArchived
    "Perbarui juga mod yang sudah dimodifikasi:", // SettingsGeneralOperationalAlsoUpdateModifiedMods
    "Ya", // SettingsGeneralOperationalYes
    "Tidak, tapi tampilkan tombol Update", // SettingsGeneralOperationalNoButShowUpdateButton
    "Tidak, dan sembunyikan tombol Update", // SettingsGeneralOperationalNoAndHideUpdateButton
    "Saat memasang mod yang sudah ada:", // SettingsGeneralOperationalWhenInstallingExistingMod
    "Selalu tanyakan", // SettingsGeneralOperationalAlwaysAsk
    "Selalu timpa", // SettingsGeneralOperationalAlwaysReplace
    "Selalu gabungkan", // SettingsGeneralOperationalAlwaysMerge
    "Selalu simpan terpisah", // SettingsGeneralOperationalAlwaysKeepBoth
    "Selalu timpa saat memperbarui mod", // SettingsGeneralOperationalAlwaysReplaceOnUpdatingMods
    "Saat menghapus mod:", // SettingsGeneralOperationalWhenDeletingMod
    "Pindahkan ke Tempat Sampah", // SettingsGeneralOperationalMoveToRecycleBin
    "Hapus permanen", // SettingsGeneralOperationalDeletePermanently
    "── EKSPERIMENTAL / Hanya XXMI ──", // SettingsGeneralOperationalXxmiExperimentalSection
    "Simpan pengaturan kustomisasi mod", // SettingsGeneralOperationalPreserveModSettings
    "Menjaga kustomisasi mod dalam game yang tersimpan (pengaturan persisten XXMI) saat mengganti nama, menonaktifkan, mengarsipkan, memperbarui, atau menghapus mod, serta saat berganti profil", // SettingsGeneralOperationalPreserveModSettingsTooltip
    "Izinkan Hestia mengubah konfigurasi d3dx.ini", // SettingsGeneralOperationalSendReloadHotkey
    "Setelah Hestia mengubah mod XXMI yang aktif saat game berjalan, kirim hotkey muat ulang XXMI agar game langsung memuatnya. Juga membuat daftar Hotkey menampilkan nilai langsung tiap mod di dalam game dan mengubahnya secara tepat, termasuk perubahan yang kamu buat lewat tombol mod itu sendiri", // SettingsGeneralOperationalSendReloadHotkeyTooltip
    "Hestia perlu mengubah {code} untuk membaca data kustomisasi mod dengan andal", // SettingsGeneralOperationalD3dxBulletAutosave
    "Hestia perlu mengubah {code} untuk memicu muat ulang XXMI dan hotkey mod", // SettingsGeneralOperationalD3dxBulletForeground
    "Mengubah ini memerlukan restart game secara manual", // SettingsGeneralOperationalD3dxBulletRestart
    "Hotkey mod yang ditekan di Hestia akan berpengaruh di dalam game", // SettingsGeneralOperationalD3dxBulletHotkeys
    "Kemampuan terbatas saat game berjalan", // SettingsGeneralOperationalPreserveLimited
    "Berfungsi penuh selama bermain", // SettingsGeneralOperationalPreserveFull
    "Muat ulang XXMI otomatis saat:", // SettingsGeneralOperationalReloadHotkeyTrigger
    "Mengaktifkan mod", // SettingsGeneralOperationalReloadTriggerEnablingMods
    "Menonaktifkan mod", // SettingsGeneralOperationalReloadTriggerDisablingMods
    "Memasang mod", // SettingsGeneralOperationalReloadTriggerInstallingMods
    "Menghapus mod", // SettingsGeneralOperationalReloadTriggerDeletingMods
    "Memperbarui mod", // SettingsGeneralOperationalReloadTriggerUpdatingMods
    "Mengganti nama mod", // SettingsGeneralOperationalReloadTriggerRenamingMods
    "Mengarsipkan mod", // SettingsGeneralOperationalReloadTriggerArchivingMods
    "Memulihkan mod", // SettingsGeneralOperationalReloadTriggerRestoringMods
    "Menyesuaikan mod", // SettingsGeneralOperationalReloadTriggerCustomizingMods
    "Beralih profil", // SettingsGeneralOperationalReloadTriggerProfileSwitch
    "Ubah d3dx.ini?", // SettingsGeneralOperationalD3dxConflictTitle
    "Untuk memuat ulang mod dan membaca kustomisasinya saat gim berjalan, Hestia menambahkan dua baris di bawah [System]:\n\t➔ additional_foreground_window = Hestia\n\t➔ settings_auto_save_interval = 1", // SettingsGeneralOperationalD3dxConflictIntro
    "d3dx.ini kamu sudah menyetel:", // SettingsGeneralOperationalD3dxConflictExisting
    "Ganti? Hestia akan mengomentari nilai yang bentrok (tetap ada, tapi nonaktif) dan menyimpan pengaturannya sendiri dalam blok bertanda di bawah [System]. Mematikan opsi ini akan mengembalikan nilai aslimu.", // SettingsGeneralOperationalD3dxConflictReplaceDetails
    "Tidak ada bagian lain di d3dx.ini yang diubah. Cadangan disimpan sebelum setiap perubahan.", // SettingsGeneralOperationalD3dxConflictNoOtherConfig

    // Window: Settings > General > Tasks
    "Unduhan", // SettingsGeneralTasksSection
    "Tampilan:", // SettingsGeneralTasksLayout
    "Bagian", // SettingsGeneralTasksLayoutSections
    "Tab", // SettingsGeneralTasksLayoutTabbed
    "Daftar", // SettingsGeneralTasksLayoutSingleList
    "Bersihkan yang beres:", // SettingsGeneralTasksClearCompletedTasks
    "Bersihkan", // SettingsGeneralTasksClearTasks
    "Urutan:", // SettingsGeneralTasksOrder
    "Terlama → Terbaru", // SettingsGeneralTasksOldestToNewest
    "Terbaru → Terlama", // SettingsGeneralTasksNewestToOldest

    // Window: Settings > Category
    "Pilih game untuk mengatur kategori.", // SettingsCategorySelectGame
    "Jelajah", // SettingsCategoryBrowseSection
    "Buat otomatis kategori GameBanana untuk mod yang diunduh", // SettingsCategoryAutoCreateGameBananaCategories
    "Berlaku untuk {game}.", // SettingsCategoryAppliesToGame
    "Kategori", // SettingsCategoryCategoriesSection
    "Pilih semua kategori", // SettingsCategorySelectAllCategories
    "Batalkan pilihan semua kategori", // SettingsCategoryUnselectAllCategories
    "Baru", // SettingsCategoryNew
    "Kategori baru (Ctrl+N)", // SettingsCategoryNewTooltip
    "Hapus", // SettingsCategoryDelete
    "Tanpa kategori", // SettingsCategoryUncategorized

    // Window: Settings > Games
    "Pindai Game", // SettingsGamesScanTitle
    "Hestia dapat melakukan pencarian mendalam untuk menemukan game yang didukung", // SettingsGamesScanDescription
    "Pindai", // SettingsGamesScanButtonScan
    "Memindai...", // SettingsGamesScanButtonScanning
    "XXMI", // SettingsGamesXxmiSection
    "XXMI Launcher:", // SettingsGamesXxmiLauncher
    "Path tidak ditemukan", // SettingsGamesPathNotFound
    "Path dilindungi", // SettingsGamesProtectedPath
    "Gunakan path mod XXMI default untuk game", // SettingsGamesUseDefaultXxmiModPath
    "Game", // SettingsGamesGamesSection
    "File EXE game:", // SettingsGamesGameExeFile
    "Folder Mod {code}:", // SettingsGamesGameModsFolder
    "Folder mod (~mods):", // SettingsGamesUnrealModFolder

    // Window: Settings > Advanced > Appearance
    "Tampilan", // SettingsAdvancedAppearanceSection
    "Bahasa:", // SettingsAdvancedAppearanceLanguage
    "Gaya Font:", // SettingsAdvancedAppearanceFontStyle
    "Klasik", // SettingsAdvancedAppearanceFontClassic
    "Modern", // SettingsAdvancedAppearanceFontModern
    "Elegan", // SettingsAdvancedAppearanceFontElegant
    "Tradisional", // SettingsAdvancedAppearanceFontTraditional
    "Menggunakan font UI sistem", // SettingsAdvancedAppearanceFontClassicTooltip
    "Menggunakan font Selawik", // SettingsAdvancedAppearanceFontModernTooltip
    "Menggunakan Diphylleia dengan Gabriela untuk teks tebal", // SettingsAdvancedAppearanceFontElegantTooltip
    "Menggunakan New Tegomin dengan Coustard untuk teks tebal", // SettingsAdvancedAppearanceFontTraditionalTooltip
    "Selalu terjemahkan detail mod", // SettingsAdvancedAppearanceAlwaysTranslateModDetails
    "Jika diaktifkan, deskripsi dan metadata mod akan diterjemahkan secara otomatis ke bahasa yang dipilih saat melihat detail", // SettingsAdvancedAppearanceAlwaysTranslateModDetailsTooltip

    // Window: Settings > Advanced > Content Restriction
    "Pembatasan Konten", // SettingsAdvancedContentRestrictionSection
    "Sembunyikan konten tidak aman:", // SettingsAdvancedContentRestrictionHideUnsafeContents
    "Sembunyikan mod NSFW dan sembunyikan jumlahnya", // SettingsAdvancedContentRestrictionHideNsfwHideCounter
    "Sembunyikan mod NSFW dan tampilkan jumlahnya", // SettingsAdvancedContentRestrictionHideNsfwShowCounter
    "Tampilkan gambar ditutupi sensor", // SettingsAdvancedContentRestrictionShowImagesCensored
    "Tampilkan semuanya", // SettingsAdvancedContentRestrictionShowUnrestricted

    // Window: Settings > Advanced > Proxy
    "Proksi", // SettingsAdvancedProxySection
    "Alamat proksi:", // SettingsAdvancedProxyAddress
    "Protokol opsional; alamat tanpa protokol dideteksi otomatis. Gunakan socks5h:// atau socks4a:// untuk DNS melalui proksi.", // SettingsAdvancedProxyHelp
    "Proksi dengan autentikasi belum didukung.", // SettingsAdvancedProxyCredentialsUnsupported
    "Masukkan alamat proksi yang valid.", // SettingsAdvancedProxyAddressInvalid
    "Proksi putus", // SettingsAdvancedProxyDisabled
    "Proksi terhubung", // SettingsAdvancedProxyEnabled
    "Tidak dapat terhubung ke proksi", // SettingsAdvancedProxyConnectionFailed
    "Saat aplikasi dibuka, Hestia memverifikasi \nkoneksi proksi sebelum memulai operasi. \nJika gagal, Hestia melanjutkan tanpa proksi.", // SettingsAdvancedProxyStartupBehavior

    // Window: Settings > Advanced > Cache and Archive
    "Cache dan Arsip", // SettingsAdvancedCacheArchiveSection
    "Ukuran cache:", // SettingsAdvancedCacheArchiveCacheSize
    "Penggunaan saat ini: {gb} GB", // SettingsAdvancedCacheArchiveCurrentUsage
    "Bersihkan Cache", // SettingsAdvancedCacheArchiveClearCache
    "Cache dibersihkan", // SettingsAdvancedCacheArchiveCacheCleared
    "Tidak dapat membersihkan cache", // SettingsAdvancedCacheArchiveClearCacheFailed
    "Penggunaan arsip: {gb} GB", // SettingsAdvancedCacheArchiveArchiveUsage
    "Hapus Mod yang Diarsipkan", // SettingsAdvancedCacheArchiveDeleteArchivedMods
    "Dipindahkan ke Tempat Sampah", // SettingsAdvancedCacheArchiveRecycled
    "Dihapus", // SettingsAdvancedCacheArchiveDeleted
    "{count} mod yang diarsipkan", // SettingsAdvancedCacheArchiveArchivedMods
    "Arsip dibersihkan: {count}", // SettingsAdvancedCacheArchiveArchivesCleared
    "Tidak ada arsip untuk dibersihkan", // SettingsAdvancedCacheArchiveNoArchivesToClear
    "Tidak dapat membersihkan arsip", // SettingsAdvancedCacheArchiveClearArchivesFailed

    // Window: Settings > About
    "oleh {authors}", // SettingsAboutBy
    "Versi:", // SettingsAboutVersion
    "Klik untuk menampilkan What's New", // SettingsAboutVersionTooltip
    "Periksa pembaruan secara otomatis", // SettingsAboutAutomaticallyCheckForUpdate
    "Memeriksa...", // SettingsAboutUpdateChecking
    "Mulai ulang untuk memperbarui", // SettingsAboutUpdateRestartToUpdate
    "Periksa Pembaruan", // SettingsAboutUpdateCheckForUpdate
    "Sudah Terbaru", // SettingsAboutUpdateUpToDate
    "Gagal Memeriksa", // SettingsAboutUpdateFailedToCheck
    "Pembaruan Manual Diperlukan", // SettingsAboutUpdateManualRequired
    "Pembaruan Tersedia", // SettingsAboutUpdateAvailable
    "Pembaruan siap", // SettingsAboutUpdateReady
    "Pembaruan gagal", // SettingsAboutUpdateFailed
    "Unduhan pembaruan dibatalkan", // SettingsAboutUpdateDownloadCanceled
    "Tunggu tugas aktif selesai sebelum memperbarui", // SettingsAboutUpdateWaitForActiveTasks
    "Tidak dapat menerapkan pembaruan", // SettingsAboutUpdateCouldNotApply
    "Hestia terpasang di folder yang tidak dapat diperbarui oleh proses ini:\n{path}\nPindahkan Hestia ke folder lain lalu coba lagi, atau perbarui instalasi ini dari proses dengan hak akses lebih tinggi.", // SettingsAboutUpdateManualInstallFolder
    "Atribusi", // SettingsAboutAttributionSection
    "Sumber data: GameBanana, API digunakan dengan izin. Metadata mod, media, dan data jelajah GameBanana bersumber dari GameBanana.", // SettingsAboutAttributionGameBanana

    // Translation strings
    "Terjemahkan (F7)", // TranslationToggleShortcut
    "Terjemahkan ulang", // TranslationRetranslate
    "Terjemahan gagal", // TranslationFailed
    "Sedang menerjemahkan", // TranslationInProgress

    // Settings: Advanced renderer
    "Renderer", // SettingsAdvancedRendererSection
    "API grafis:", // SettingsAdvancedRendererGraphicsApi
    "Otomatis (disarankan)", // SettingsAdvancedRendererAuto
    "Renderer aktif: {backend}", // SettingsAdvancedRendererActive
    "Perubahan berlaku setelah Hestia dimulai ulang.", // SettingsAdvancedRendererRestartHint
    "Mulai Ulang", // SettingsAdvancedRendererRestart

    // Fullview image overlay
    "Salin gambar", // OverlayCopyImage
    "Gambar sebelumnya", // OverlayPreviousImage
    "Gambar berikutnya", // OverlayNextImage
    "Gambar disalin ke papan klip", // OverlayImageCopied
    "Tidak dapat menyalin gambar", // OverlayCouldNotCopyImage
    "Hapus pengaturan mod", // LibraryHotkeyClearCustomization
    "Tautkan Mod", // LibraryLinkMod
    "Memeriksa…", // LibraryChecking
    "Atur ulang ukuran dan posisi", // ResetWindowLayout
    "Filter karakter", // BrowseFilterCharactersHint
    "Belum ada sumber GameBanana yang ditautkan untuk mod ini. Klik untuk menautkannya.", // LibraryUnlinkedClickToLink
    "Nama Z-A", // LibraryCategorySortByNameDesc
    "Tanpa kategori: {status}", // LibraryUncategorizedStatusHeader
    "Urutkan & Susun", // LibrarySortMenuTitle
];
