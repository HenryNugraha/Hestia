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

fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Name("Bold".into()))
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

fn status_label(status: &ModStatus) -> &'static str {
    match status {
        ModStatus::Active => "Active",
        ModStatus::Disabled => "Disabled",
        ModStatus::Archived => "Archived",
    }
}

fn mod_update_state_badge(state: ModUpdateState) -> (&'static str, Color32) {
    match state {
        ModUpdateState::Unlinked => ("Unlinked", Color32::from_gray(140)),
        ModUpdateState::UpToDate => ("Up to Date", Color32::from_rgb(140, 174, 138)),
        ModUpdateState::UpdateAvailable => ("Update Available", Color32::from_rgb(144, 188, 150)),
        ModUpdateState::MissingSource => ("Missing Source", Color32::from_rgb(196, 166, 126)),
        ModUpdateState::ModifiedLocally => ("Modified Locally", Color32::from_rgb(179, 133, 133)),
        ModUpdateState::IgnoringUpdateOnce => ("Ignoring Update Once", Color32::from_rgb(181, 153, 196)),
        ModUpdateState::IgnoringUpdateAlways => ("Ignoring Update Always", Color32::from_rgb(181, 153, 196)),
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

fn parse_gb_id_from_entry(mod_entry: &ModEntry) -> u64 {
    mod_entry
        .source
        .as_ref()
        .and_then(|s| s.gamebanana.as_ref())
        .map(|l| l.mod_id)
        .unwrap_or(0)
}
