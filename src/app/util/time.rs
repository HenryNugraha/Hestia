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
        let value = String::from_utf16_lossy(&buffer[..(len as usize).saturating_sub(1)]);
        value.contains('H')
    }

    #[cfg(not(windows))]
    {
        true
    }
}

fn format_log_timestamp(timestamp: DateTime<Utc>, use_24h: bool) -> (String, String) {     let local = timestamp.with_timezone(&Local);     let date = local.format("%e %B %Y").to_string().trim_start().to_string();     let time = if use_24h {         local.format("%H:%M").to_string()     } else {         local.format("%I:%M %p").to_string()     };     (date, time) } 

fn mod_age_label(updated_at: DateTime<Utc>) -> String {     relative_time_label(updated_at, false) }  fn relative_time_label(updated_at: DateTime<Utc>, compact_today: bool) -> String {     let local_now = Local::now();     let local_then = updated_at.with_timezone(&Local);     let delta = local_now.signed_duration_since(local_then);     let minutes = delta.num_minutes().max(0);     let hours = delta.num_hours().max(0);     let days = delta.num_days().max(0);     if days <= 0 {         if compact_today {             "Today".to_string()         } else if hours <= 0 {             if minutes <= 0 {                 "Now".to_string()             } else {                 format!("{minutes}m")             }         } else {             format!("{hours}h")         }     } else {         format!("{days}d")     } }  fn timestamp_to_utc(timestamp: i64) -> DateTime<Utc> {     DateTime::<Utc>::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now) } 

fn format_exact_local_timestamp(timestamp: i64) -> String {     let utc = timestamp_to_utc(timestamp);     utc.with_timezone(&Local)         .format("%d-%b-%Y %H:%M")         .to_string() }  async fn download_to_bytes_async(     client: &ClientWithMiddleware,     url: &str,     cancel: &Arc<AtomicBool>,     progress: Arc<RwLock<DownloadProgress>>, ) -> Result<Vec<u8>> {     if cancel.load(Ordering::Relaxed) {         bail!(importing::CANCELLED_ERROR);     }     let response = client         .get(url)         .send()         .await?         .error_for_status()?;      if let Some(len) = response.content_length() {         if let Ok(mut guard) = progress.write() {             guard.total = Some(len);         }     }      let mut bytes = if let Some(total) = response.content_length() {         Vec::with_capacity(total as usize)     } else {         Vec::new()     };     let mut stream = response.bytes_stream();     while let Some(chunk) = stream.next().await {         if cancel.load(Ordering::Relaxed) {             bail!(importing::CANCELLED_ERROR);         }         let chunk = chunk?;         let read = chunk.len();         if read == 0 {             continue;         }         bytes.extend_from_slice(&chunk);          if let Ok(mut guard) = progress.write() {             guard.downloaded += read as u64;             guard.bytes_since_last += read as u64;             let now = std::time::Instant::now();             let elapsed = now.duration_since(guard.last_update);             if elapsed.as_millis() >= 500 {                 guard.speed = guard.bytes_since_last as f64 / elapsed.as_secs_f64();                 guard.bytes_since_last = 0;                 guard.last_update = now;             }         }     }     Ok(bytes) } 
