fn system_uses_24h_time() -> bool {
    #[cfg(windows)]
    {
        let mut buffer = [0u16; 80];
        let len = unsafe {
            GetLocaleInfoEx(
                PCWSTR::null(),
                LOCALE_STIMEFORMAT,
                Some(&mut buffer),
            )
        };
        if len <= 0 {
            return true;
        }
        let value = String::from_utf16_lossy(
            &buffer[..(len as usize).saturating_sub(1)]
        );
        value.contains('H')
    }

    #[cfg(not(windows))]
    return true;
}

fn format_log_timestamp(timestamp: DateTime<Utc>, use_24h: bool) -> (String, String) {
    let local = timestamp.with_timezone(&Local);
    let date = local.format("%e %B %Y").to_string().trim_start().to_string();

    let time = if use_24h {
        local.format("%H:%M").to_string()
    } else {
        local.format("%I:%M %p").to_string()
    };

    (date, time)
}

fn mod_age_label(updated_at: DateTime<Utc>, text: TextCatalog) -> String {
    relative_time_label_at(updated_at, Local::now(), false, text)
}

fn relative_time_label_at(
    updated_at: DateTime<Utc>,
    local_now: DateTime<Local>,
    compact_today: bool,
    text: TextCatalog,
) -> String {
    let local_then = updated_at.with_timezone(&Local);
    let delta = local_now.signed_duration_since(local_then);

    let minutes = delta.num_minutes().max(0);
    let hours = delta.num_hours().max(0);
    let days = delta.num_days().max(0);

    if days <= 0 {
        if compact_today {
            text.relative_time_today().to_string()
        } else if hours <= 0 {
            if minutes <= 0 {
                text.relative_time_now().to_string()
            } else {
                text.relative_time_minutes(minutes)
            }
        } else {
            text.relative_time_hours(hours)
        }
    } else {
        text.relative_time_days(days)
    }
}

fn timestamp_to_utc(timestamp: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
}

fn format_exact_local_timestamp(timestamp: i64) -> String {
    let utc = timestamp_to_utc(timestamp);
    utc.with_timezone(&Local)
        .format("%d-%b-%Y %H:%M")
        .to_string()
}
