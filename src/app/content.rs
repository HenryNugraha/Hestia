use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese, Russian.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n(
    "10 October 2026",
    "10 Oktober 2026",
    "2026年 10月 10日",
    "10 октября 2026"
);
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        concat!(
            "Added a new overlay feature on supported games\n",
            "▸ Press ALT+H to bring the overlay up\n",
            "▸ Navigate characters & mods with ASWD\n",
            "▸ Space or SHIFT+Space to enable/disable mods",
        ),
        concat!(
            "Menambahkan fitur overlay baru pada game yang didukung\n",
            "▸ Tekan ALT+H untuk menampilkan overlay\n",
            "▸ Navigasi pilihan karakter & mod dengan ASWD\n",
            "▸ Tekan Space atau SHIFT+Space untuk mengaktifkan/menonaktifkan mod",
        ),
        concat!(
            "在受支持的游戏中新增游戏内浮层功能\n",
            "▸ 按 ALT+H 打开浮层\n",
            "▸ 使用 ASWD 在角色和模组列表中移动\n",
            "▸ 按 Space 或 SHIFT+Space 启用/禁用模组",
        ),
        concat!(
            "В поддерживаемых играх появился внутриигровой оверлей\n",
            "▸ Нажмите ALT+H, чтобы открыть оверлей\n",
            "▸ Перемещайтесь по спискам персонажей и модов с помощью ASWD\n",
            "▸ Нажимайте Space или SHIFT+Space, чтобы включать и отключать моды",
        ),
    ),
    l10n(
        "Now supports adjusting interface size",
        "Sekarang bisa memperbesar/memperkecil tampilan aplikasi",
        "现在可以放大或缩小应用界面",
        "Теперь размер интерфейса можно менять",
    ),
    l10n(
        concat!(
            "Added an option to always install a mod as Enabled or Disabled\n",
            "▸ Plus an Auto option with smart detection",
        ),
        concat!(
            "Menambahkan opsi untuk selalu memasang mod dalam keadaan Aktif atau Nonaktif\n",
            "▸ Opsi Otomatis: aktif/nonaktif sesuai kategori",
        ),
        concat!(
            "新增选项，可始终将模组安装为启用或禁用状态\n",
            "▸ “自动”选项根据分类决定启用或禁用模组",
        ),
        concat!(
            "Добавлена возможность всегда устанавливать мод включённым или отключённым\n",
            "▸ Вариант «Авто» выбирает состояние мода в зависимости от категории",
        ),
    ),
    l10n(
        "Censored mods are now blurred",
        "Mod kini disensor dengan blur",
        "需要遮蔽的模组图片现在会模糊显示",
        "Цензурированные изображения модов теперь размыты",
    ),
];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = true;
pub(crate) const FEEDBACK_SURVEY_LAUNCH_DELAY: u32 = 16;
pub(crate) const FEEDBACK_SURVEY_TITLE: L10n = l10n(
    "Quick Feedback",
    "Survey Singkat",
    "快速反馈",
    "Быстрый отзыв",
);
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[ContentSurveyQuestion] = &[
    q(
        "overlay_feature",
        l10n(
            "How easy is it to use the new in-game overlay?",
            "Seberapa mudah menggunakan overlay baru di dalam game?",
            "使用新的游戏内浮层有多容易？",
            "Насколько легко пользоваться новым внутриигровым оверлеем?",
        ),
        &[
            a(1, l10n("Easy", "Mudah", "简单", "Легко")),
            a(2, l10n("Moderate", "Sedang", "一般", "Средне")),
            a(3, l10n("Difficult", "Sulit", "困难", "Сложно")),
            a(
                4,
                l10n(
                    "Haven't tried it yet",
                    "Belum mencobanya",
                    "还没试过",
                    "Ещё не пробовал(а)",
                ),
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
