use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese, Russian.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n(
    "26 July 2026",
    "26 Juli 2026",
    "2026年 7月 26日",
    "26 июль 2026",
);
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        "Various interface fixes and improvements",
        "Berbagai perbaikan dan peningkatan tampilan",
        "多项界面修复与改进",
        "Ряд исправлений и улучшений интерфейса",
    ),
    l10n(
        "Further refined selecting files for mod auto-update",
        "Penyempurnaan lebih lanjut pada pemilihan file untuk pembaruan otomatis mod",
        "进一步改进了模组自动更新时的文件选择",
        "Доработан выбор файлов для автообновления модов",
    ),
    l10n(
        "Fixed filtering specific character's mods not showing all mods",
        "Memperbaiki filter mod berdasarkan karakter tertentu yang tidak menampilkan semua mod",
        "修复了按特定角色过滤模组时未显示全部模组的问题",
        "Исправлена фильтрация модов по персонажу: теперь отображаются все моды",
    )
];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = false;
pub(crate) const FEEDBACK_SURVEY_LAUNCH_DELAY: u32 = 15;
pub(crate) const FEEDBACK_SURVEY_TITLE: L10n = l10n(
    "Quick Feedback",
    "Survey Singkat",
    "小调查",
    "Быстрый отзыв",
);
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[ContentSurveyQuestion] = &[
    q(
        "translate_tool",
        l10n(
            "Have you used the translate button on mods?",
            "Apakah kamu pernah menggunakan tombol terjemahkan pada mod?",
            "你使用过模组上的翻译按钮吗？",
            "Вы использовали кнопку перевода для модов?",
        ),
        &[
            a(
                1,
                l10n(
                    "Good and fast",
                    "Bagus dan cepat",
                    "效果好且速度快",
                    "Хорошо и быстро",
                ),
            ),
            a(
                2,
                l10n(
                    "Good but slow",
                    "Bagus, tapi lambat",
                    "效果好，但速度慢",
                    "Хорошо, но медленно",
                ),
            ),
            a(
                3,
                l10n(
                    "Poor but fast",
                    "Buruk, tapi cepat",
                    "效果差，但速度快",
                    "Плохо, но быстро",
                ),
            ),
            a(
                4,
                l10n(
                    "Poor and slow",
                    "Buruk dan lambat",
                    "效果差且速度慢",
                    "Плохо и медленно",
                ),
            ),
            a(
                5,
                l10n(
                    "Don't use it / didn't know about it",
                    "Tidak pakai / tidak tahu fitur ini",
                    "不用 / 不知道有这个功能",
                    "Не пользуюсь / не знаю про эту функцию",
                ),
            ),
        ],
    ),
    q(
        "language_russian_quality",
        l10n(
            "If using RUSSIAN localization: How do you like it?",
            "Jika pakai BAHASA RUSIA: Bagaimana menurutmu?",
            "如果你用俄语：你觉得怎么样？",
            "Если используете РУССКУЮ локализацию: как она вам?",
        ),
        &[
            a(1, l10n("Great", "Bagus", "很好", "Отлично")),
            a(2, l10n("Okay", "Biasa", "还行", "Нормально")),
            a(3, l10n("Poor", "Buruk", "不好", "Плохо")),
            a(4, l10n("Not using it", "Tidak pakai", "没在用", "Не использую")),
        ],
    ),
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: L10n = l10n(
    "Anything else? Feature requests, issues, or suggestions are welcome!",
    "Ada lagi? Permintaan fitur, masalah, atau saran boleh ditulis di sini!",
    "还有其他想说的吗？欢迎提出功能需求、问题或建议！",
    "Есть что добавить? Пишите о пожеланиях, проблемах или предложениях!",
);
