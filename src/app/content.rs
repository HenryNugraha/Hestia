use crate::model::{ContentSurveyQuestion, L10n, a, l10n, q};

// l10n order: English, Bahasa Indonesia, Simplified Chinese.
// Add empty string "" to skip a language.

pub(crate) const WHATS_NEW_DATE: L10n = l10n("xx June 2026", "xx Juni 2026", "2026年 6月 xx日");
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[L10n] = &[
    l10n(
        concat!(
            "Added app translation to Bahasa Indonesia & Chinese\n",
            "▸ Settings > Advanced > Appearance > Languages"
        ),
        concat!(
            "Menambahkan terjemahan aplikasi ke Bahasa Indonesia & Mandarin\n",
            "▸ Setelan > Lanjutan > Tampilan > Bahasa"
        ),
        concat!(
            "新增印度尼西亚语和简体中文翻译\n",
            "▸ 设置 > 高级 > 外观 > 语言"
        ),
    ),
    l10n(
        concat!(
            "Added experimental translate button for mod title and description\n",
            "▸ Server is unstable, can be VERY SLOW"
        ),
        concat!(
            "Menambahkan tombol terjemah untuk judul dan deskripsi mod\n",
            "▸ Server tidak stabil, bisa sangat LAMBAT"
        ),
        concat!(
            "为 mod 标题和描述添加了实验性翻译按钮\n",
            "▸ 服务器不稳定，速度可能会非常慢"
        ),
    ),
    l10n(
        "Added support for resuming downloads",
        "Menambahkan dukungan untuk melanjutkan unduhan",
        "新增支持断点续传功能",
    ),
];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = true;
pub(crate) const FEEDBACK_SURVEY_LAUNCH_DELAY: u32 = 15;
pub(crate) const FEEDBACK_SURVEY_TITLE: L10n =
    l10n("Quick Feedback", "Survey Singkat", "小调查");
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[ContentSurveyQuestion] = &[
    q(
        "language_indonesia_quality",
        l10n(
            "If using INDONESIAN localization: How do you like it?",
            "Jika pakai BAHASA INDONESIA: Bagaimana menurutmu?",
            "如果你用印度尼西亚语：你觉得怎么样？",
        ),
        &[
            a(1, l10n("Great", "Bagus", "很好")),
            a(2, l10n("Okay", "Biasa", "还行")),
            a(3, l10n("Poor", "Buruk", "不好")),
            a(4, l10n("Not using it", "Tidak pakai", "没在用")),
        ],
    ),
    q(
        "language_chinese_quality",
        l10n(
            "If using CHINESE localization: How do you like it?",
            "Jika pakai MANDARIN: Bagaimana menurutmu?",
            "如果你使用中文版：你觉得怎么样？",
        ),
        &[
            a(1, l10n("Great", "Bagus", "很好")),
            a(2, l10n("Okay", "Biasa", "还行")),
            a(3, l10n("Poor", "Buruk", "不好")),
            a(4, l10n("Not using it", "Tidak pakai", "没在用")),
        ],
    ),
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: L10n = l10n(
    "Feature requests? Issues? Let me know!",
    "Permintaan fitur? Masalah? Infokan aja!",
    "有功能建议或遇到了问题？告诉我！（最好用英文）",
);
