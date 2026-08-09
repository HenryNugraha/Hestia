use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese, Russian.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n(
    "9 August 2026",
    "9 Agustus 2026",
    "2026年 8月 9日",
    "9 августа 2026",
);
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[l10n(
    concat!(
        "Added mod profiles feature\n",
        "▸ Categories and tools are included in the profile\n",
        "▸ Inactive profiles will be compressed to save storage space",
    ),
    concat!(
        "Menambahkan fitur profil mod\n",
        "▸ Kategori dan alat termasuk dalam profil\n",
        "▸ Profil yang tidak aktif akan dikompres untuk menghemat penyimpanan\n",
    ),
    concat!(
        "新增配置文件功能\n",
        "▸ 分类和工具已包含在配置文件中\n",
        "▸ 不活动的配置文件将被压缩以节省空间\n",
    ),
    concat!(
        "Добавлена функция профилей\n",
        "▸ Категории и инструменты включены в профиль\n",
        "▸ Неактивные профили будут сжаты для экономии места",
    ),
)];

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
            a(
                1,
                l10n(
                    "Yes",
                    "Iya",
                    "是的",
                    "Да",
                ),
            ),
            a(
                2,
                l10n(
                    "No",
                    "Tidak",
                    "不",
                    "Нет",
                ),
            ),
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
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: L10n = l10n(
    "Anything else? Feature requests, issues, or suggestions are welcome!",
    "Ada lagi? Permintaan fitur, masalah, atau saran boleh ditulis di sini!",
    "还有其他想说的吗？欢迎提出功能需求、问题或建议！",
    "Есть что добавить? Пишите о пожеланиях, проблемах или предложениях!",
);
