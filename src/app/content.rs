use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese, Russian.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n(
    "xx xx 2026",
    "xx xx 2026",
    "2026年 xx月 xx日",
    "xx xx 2026",
);
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        "A few more tiny performance optimizations",
        "Sedikit optimasi performa tambahan",
        "又做了一点额外的性能优化",
        "Ещё немного дополнительных оптимизаций производительности",
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
