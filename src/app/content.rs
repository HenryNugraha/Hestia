use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese, Russian.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n(
    "20 August 2026",
    "20 Agustus 2026",
    "2026年 8月 20日",
    "20 августа 2026",
);
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        concat!(
            "Added a Hotkeys metadata section for mods\n",
            "▸ Open a mod's details, click Description, then select Hotkeys from the dropdown\n",
            "▸ If the game is not running, hotkeys are clickable and will take effect the next time the game is launched\n",
            "▸ While the game is running, hotkey clicking is disabled unless you allow Hestia to modify d3dx.ini\n",
            "▸ See Settings > General > Operational > Experimental",
        ),
        concat!(
            "Menambahkan halaman baru untuk melihat hotkey mod\n",
            "▸ Akses melalui detail Mod, klik Deskripsi, lalu pilih Hotkey dari dropdown\n",
            "▸ Jika game mati, hotkey dapat diklik dan akan berlaku saat game dijalankan berikutnya\n",
            "▸ Saat sedang bermain, hotkey tidak dapat diklik, kecuali jika Hestia diizinkan mengubah d3dx.ini\n",
            "▸ Lihat Pengaturan > Umum > Operasional > Eksperimental",
        ),
        concat!(
            "新增 Mod 快捷键元数据部分\n",
            "▸ 打开 Mod 详情，点击描述，然后从下拉菜单选择快捷键\n",
            "▸ 如果游戏未运行，快捷键可点击，并会在下次启动游戏时生效\n",
            "▸ 游戏运行时，快捷键点击会被禁用，除非你允许 Hestia 修改 d3dx.ini\n",
            "▸ 请查看 设置 > 常规 > 操作 > 实验性",
        ),
        concat!(
            "Добавлен раздел метаданных с горячими клавишами модов\n",
            "▸ Откройте сведения о моде, нажмите «Описание», затем выберите «Горячие клавиши» в раскрывающемся списке\n",
            "▸ Если игра не запущена, горячие клавиши доступны для нажатия и сработают при следующем запуске игры\n",
            "▸ Пока игра запущена, нажатие горячих клавиш отключено, если вы не разрешили Hestia изменять d3dx.ini\n",
            "▸ См. Опции > Общие > Операции > Экспериментально",
        ),
    ),
    l10n(
        concat!(
            "Added support for preserving mod in-game settings when enabling/disabling mods or switching profiles\n",
            "▸ Works with limitations while the game is running\n",
            "▸ Allow Hestia to modify d3dx.ini to remove this limitation\n",
            "▸ See Settings > General > Operational > Experimental",
        ),
        concat!(
            "Menambahkan dukungan untuk menyimpan pengaturan mod ketika mengganti mod dan profil\n",
            "▸ Fitur ini mungkin terbatas saat game sedang berjalan\n",
            "▸ Izinkan Hestia mengedit d3dx.ini agar lebih maksimal\n",
            "▸ Lihat Pengaturan > Umum > Operasional > Eksperimental",
        ),
        concat!(
            "新增在启用/禁用 Mod 或切换配置文件时保留 Mod 游戏内设置的支持\n",
            "▸ 游戏运行时功能会受限\n",
            "▸ 允许 Hestia 修改 d3dx.ini 可解除此限制\n",
            "▸ 请查看 设置 > 常规 > 操作 > 实验性",
        ),
        concat!(
            "Добавлена поддержка сохранения внутриигровых настроек модов при включении/отключении модов или переключении профилей\n",
            "▸ Пока игра запущена, работает с ограничениями\n",
            "▸ Разрешите Hestia изменять d3dx.ini, чтобы убрать это ограничение\n",
            "▸ См. Опции > Общие > Операции > Экспериментально",
        ),
    ),
    l10n(
        "Changed the Settings hotkey from F10 to CTRL+P",
        "Hotkey Setelan diubah dari F10 menjadi CTRL+P",
        "设置快捷键已从 F10 改为 CTRL+P",
        "Горячая клавиша «Опции» изменена с F10 на CTRL+P",
    ),
    l10n(
        "Various visual and interface improvements",
        "Beberapa peningkatan pada tampilan dan menu",
        "多项视觉和界面改进",
        "Разные улучшения внешнего вида и интерфейса",
    ),
];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = true;
pub(crate) const FEEDBACK_SURVEY_LAUNCH_DELAY: u32 = 32;
pub(crate) const FEEDBACK_SURVEY_TITLE: L10n = l10n(
    "Quick Feedback",
    "Survey Singkat",
    "小调查",
    "Быстрый отзыв",
);
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[ContentSurveyQuestion] = &[
    q(
        "profile_feature",
        l10n(
            "Do you find the mod profiles feature useful?",
            "Apakah fitur profil mod berguna bagi Anda?",
            "你觉得 mod 配置文件功能有用吗？",
            "Вы находите функцию профилей полезной?",
        ),
        &[
            a(1, l10n("Yes", "Iya", "是的", "Да")),
            a(2, l10n("No", "Tidak", "不", "Нет")),
            a(
                3,
                l10n(
                    "Never used it",
                    "Tidak pernah pakai",
                    "从未使用过",
                    "Не использовал",
                ),
            ),
        ],
    ),
    q(
        "hotkeys_introduction",
        l10n(
            "If you've used the new Hotkeys feature, has it worked correctly for you?",
            "Kalau kamu sudah mencoba fitur Hotkeys baru, apakah berfungsi dengan benar?",
            "如果你使用过新的快捷键功能，它是否正常工作？",
            "Если вы уже пользовались новой функцией горячих клавиш, она работает корректно?",
        ),
        &[
            a(
                1,
                l10n(
                    "Works well",
                    "Berfungsi baik",
                    "运行良好",
                    "Работает хорошо",
                ),
            ),
            a(
                2,
                l10n("Has issues", "Ada masalah", "存在问题", "Есть проблемы"),
            ),
        ],
    ),
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: L10n = l10n(
    "Anything else?\nFeature requests, issues, or suggestions are welcome!",
    "Ada lagi?\nPermintaan fitur, masalah, atau saran boleh ditulis di sini!",
    "还有其他想说的吗？\n欢迎提出功能需求、问题或建议！",
    "Есть что добавить?\nПишите о пожеланиях, проблемах или предложениях!",
);
