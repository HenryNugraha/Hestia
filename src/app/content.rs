use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese.

pub(crate) const WHATS_NEW_DATE: L10n = l10n("xx June 2026", "xx Juni 2026", "2026年 6月 xx日");
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        "Added app translation to Bahasa Indonesia & Chinese",
        "Menambahkan terjemahan aplikasi ke Bahasa Indonesia & Mandarin",
        "新增 Bahasa Indonesia 和简体中文翻译",
    ),
];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = true;
pub(crate) const FEEDBACK_SURVEY_LAUNCH_DELAY: u32 = 15;
pub(crate) const FEEDBACK_SURVEY_TITLE: L10n =
    l10n("Quick Feedback", "Masukan Singkat", "快速反馈");
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[ContentSurveyQuestion] = &[
    q(
        "light_theme_demand",
        l10n(
            "Would you like a white/light app theme?",
            "Apakah Anda ingin tema aplikasi putih/terang?",
            "你想要白色/浅色应用主题吗？",
        ),
        &[
            a(1, l10n("Yes", "Ya", "是")),
            a(2, l10n("I don't care", "Tidak masalah", "无所谓")),
        ],
    ),
    q(
        "translation_demand",
        l10n(
            "Would you like a non-English translation?",
            "Apakah Anda ingin terjemahan selain bahasa Inggris?",
            "你想要非英语翻译吗？",
        ),
        &[
            a(1, l10n("Yes", "Ya", "是")),
            a(2, l10n("I don't care", "Tidak masalah", "无所谓")),
        ],
    ),
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: L10n = l10n(
    "What language? Or, anything else?",
    "Bahasa apa? Atau ada hal lain?",
    "想要哪种语言？或者还有其他意见吗？",
);
