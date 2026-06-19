static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

fn set_current_language(language: AppLanguage) {
    CURRENT_LANGUAGE.store(language as u8, Ordering::Relaxed);
}

fn current_language() -> Option<AppLanguage> {
    match CURRENT_LANGUAGE.load(Ordering::Relaxed) {
        0 => Some(AppLanguage::English),
        1 => Some(AppLanguage::Indonesian),
        2 => Some(AppLanguage::ChineseSimplified),
        3 => Some(AppLanguage::Russian),
        _ => None,
    }
}

fn parse_gb_id(input: &str) -> Option<u64> {
    let input = input.trim();
    if let Ok(id) = input.parse::<u64>() {
        return Some(id);
    }
    if let Some(idx) = input.find("/mods/") {
        let rest = &input[idx + 6..];
        let id_part = rest.split('/').next()?;
        return id_part.parse::<u64>().ok();
    }
    None
}

fn sanitize_folder_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "Imported Mod".to_string()
    } else {
        trimmed.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
    }
}

fn bold(text: impl Into<String>, size: Option<f32>) -> RichText {
    let rich_text = RichText::new(text).family(FontFamily::Name(BOLD_FONT_FAMILY.into()));
    let final_size = size.unwrap_or(12.0);
    rich_text.size(if current_language() == Some(AppLanguage::Russian) {
        final_size * 0.75
    } else {
        final_size
    })
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_lookup(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn icon_char(icon: Icon) -> char {
    char::from(icon)
}

fn hash64_hex(data: &[u8]) -> String {
    format!("{:016x}", xxh3_64(data))
}

fn icon_rich(icon: Icon, size: f32, color: Color32) -> RichText {
    RichText::new(icon_char(icon).to_string())
        .family(FontFamily::Name(LUCIDE_FAMILY.into()))
        .size(size)
        .color(color)
}

fn sanitize_log_subject(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
        .collect();
    sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn build_log_text(entries: &[OperationLogEntry]) -> String {
    let use_24h = system_uses_24h_time();
    let mut last_date: Option<String> = None;
    let mut lines = Vec::new();
    for entry in entries.iter().rev() {
        let (date, time) = format_log_timestamp(entry.timestamp, use_24h);
        if last_date.as_deref() != Some(date.as_str()) {
            if last_date.is_some() {
                lines.push(String::new());
            }
            lines.push(date.clone());
            last_date = Some(date);
        }
        let summary = sanitize_log_subject(&entry.summary);
        lines.push(format!("[{}] {}", time, summary));
    }
    lines.join("\n")
}

fn mod_update_state_tooltip(state: ModUpdateState) -> &'static str {
    match state {
        ModUpdateState::Unlinked => "No GameBanana source is linked for this mod.",
        ModUpdateState::UpToDate => {
            "The linked source was checked and no newer version was found."
        }
        ModUpdateState::UpdateAvailable => {
            "A newer version is available from the linked source."
        }
        ModUpdateState::CheckSkipped => {
            "This mod is linked, but update checks are disabled for its current status."
        }
        ModUpdateState::MissingSource => {
            "The linked source or tracked file is no longer available."
        }
        ModUpdateState::ModifiedLocally => {
            "This mod has local changes since it was linked or last updated."
        }
        ModUpdateState::IgnoringUpdateOnce => {
            "The current update is ignored until a newer update appears."
        }
        ModUpdateState::IgnoringUpdateAlways => {
            "Updates for this mod are ignored until this option is turned off."
        }
    }
}

fn format_file_size(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size_f = size as f64;
    if size_f >= GB {
        format!("{:.2} GB", size_f / GB)
    } else if size_f >= MB {
        format!("{:.2} MB", size_f / MB)
    } else if size_f >= KB {
        format!("{:.1} KB", size_f / KB)
    } else {
        format!("{size} B")
    }
}

fn format_compact_count(count: u64) -> String {
    const SUFFIXES: &[(u64, &str)] = &[(1_000_000_000, "b"), (1_000_000, "m"), (1_000, "k")];

    let Some(&(divisor, suffix)) = SUFFIXES.iter().find(|&&(divisor, _)| count >= divisor) else {
        return count.to_string();
    };

    let whole = count / divisor;
    if whole < 10 {
        let tenth = (count % divisor) / (divisor / 10);
        if tenth > 0 {
            return format!("{whole}.{tenth}{suffix}");
        }
    }

    format!("{whole}{suffix}")
}

fn format_count_with_separators(count: u64) -> String {
    let digits = count.to_string();
    let first_group_len = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut formatted = String::with_capacity(digits.len() + (digits.len() - 1) / 3);
    formatted.push_str(&digits[..first_group_len]);
    for group in digits[first_group_len..].as_bytes().chunks(3) {
        formatted.push(',');
        formatted.extend(group.iter().map(|&byte| byte as char));
    }
    formatted
}

#[cfg(test)]
mod formatting_tests {
    use super::{format_compact_count, format_count_with_separators};

    #[test]
    fn formats_compact_counts_without_rounding_up() {
        assert_eq!(format_compact_count(123), "123");
        assert_eq!(format_compact_count(1_234), "1.2k");
        assert_eq!(format_compact_count(12_345), "12k");
        assert_eq!(format_compact_count(123_456), "123k");
        assert_eq!(format_compact_count(1_234_567), "1.2m");
        assert_eq!(format_compact_count(9_999), "9.9k");
    }

    #[test]
    fn formats_exact_counts_with_thousands_separators() {
        assert_eq!(format_count_with_separators(123), "123");
        assert_eq!(format_count_with_separators(1_234), "1,234");
        assert_eq!(format_count_with_separators(1_234_567), "1,234,567");
    }
}

fn icon_text_sized(icon: Icon, label: &str, icon_size: f32, text_size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &icon_char(icon).to_string(),
        0.0,
        TextFormat {
            font_id: egui::FontId::new(icon_size, FontFamily::Name(LUCIDE_FAMILY.into())),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job.append(
        " ",
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(text_size),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job.append(
        label,
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(text_size),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job
}

fn icon_bold_text(icon: Icon, label: &str, icon_size: f32, text_size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &icon_char(icon).to_string(),
        0.0,
        TextFormat {
            font_id: egui::FontId::new(icon_size, FontFamily::Name(LUCIDE_FAMILY.into())),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job.append(
        " ",
        0.0,
        TextFormat {
            font_id: egui::FontId::new(text_size, FontFamily::Name(BOLD_FONT_FAMILY.into())),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job.append(
        label,
        0.0,
        TextFormat {
            font_id: egui::FontId::new(text_size, FontFamily::Name(BOLD_FONT_FAMILY.into())),
            color: Color32::from_rgb(225, 229, 233),
            ..Default::default()
        },
    );
    job
}

fn parse_gb_id_from_entry(mod_entry: &ModEntry) -> u64 {
    mod_entry
        .source
        .as_ref()
        .and_then(|s| s.gamebanana.as_ref())
        .map(|l| l.mod_id)
        .unwrap_or(0)
}
