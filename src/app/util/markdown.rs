fn normalize_markdown_image_dest(raw: &str) -> String {
    let s = raw.trim();
    let url = if s.starts_with('<') {
        if let Some(end_idx) = s.find('>') {
            s[1..end_idx].trim()
        } else {
            s
        }
    } else {
        s.split(['"', '\'']).next().unwrap_or(s).trim()
    };
    url.replace("%20", " ")
}

fn extract_markdown_image_dests(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    for cap in MARKDOWN_IMAGE_DEST_RE.captures_iter(markdown) {
        if let Some(dest) = cap.get(2) {
            out.push(normalize_markdown_image_dest(dest.as_str()));
        }
    }
    out
}

fn is_gif_dest(dest: &str) -> bool {
    let base = dest.split_once('#').map(|(a, _)| a).unwrap_or(dest);
    let base = base.split_once('?').map(|(a, _)| a).unwrap_or(base);
    base.to_ascii_lowercase().ends_with(".gif")
}

fn file_uri_to_path(dest: &str) -> Option<PathBuf> {
    let raw = if let Some(r) = dest.strip_prefix("file:///") {
        r
    } else if let Some(r) = dest.strip_prefix("file://") {
        r
    } else {
        dest.strip_prefix("file:/")?
    };
    let decoded = percent_decode(raw);
    let mut path_str = decoded.replace('/', "\\");
    if path_str.starts_with('\\') && path_str.len() > 2 && path_str.get(2..3) == Some(":") {
        path_str.remove(0);
    }
    Some(PathBuf::from(path_str))
}

fn gif_preview_out_path(dest: &str, mod_root: Option<&Path>) -> PathBuf {
    if let (Some(root), Some(src_path)) = (mod_root, file_uri_to_path(dest)) {
        let meta_dir = root.join(MOD_META_DIR);
        if src_path.starts_with(&meta_dir) {
            if let Some(stem) = src_path.file_stem().and_then(|s| s.to_str()) {
                return meta_dir.join(format!("{stem}_preview.png"));
            }
        }
    }
    let key = hash64_hex(dest.as_bytes());
    persistence::runtime_temp_cache_dir()
        .join("gif_preview")
        .join(format!("{key}.png"))
}

fn gif_anim_cache_path(dest: &str) -> PathBuf {
    let key = hash64_hex(dest.as_bytes());
    persistence::runtime_temp_cache_dir()
        .join("gif_anim")
        .join(format!("{key}.gif"))
}

fn rewrite_markdown_gif_images(markdown: &str, _mod_root: Option<&Path>) -> String {
    MARKDOWN_IMAGE_DEST_RE
        .replace_all(markdown, |caps: &regex::Captures| {
            let alt = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let dest = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let dest = normalize_markdown_image_dest(dest);
            if !is_gif_dest(&dest) {
                return caps[0].to_string();
            }
            let texture_key = format!("gif-preview-{}", hash64_hex(dest.as_bytes()));
            format!("![{alt}]({texture_key})")
        })
        .to_string()
}

fn extract_markdown_images(markdown: &str) -> Vec<(usize, usize, String)> {
    let mut images = Vec::new();
    let img_re = Regex::new(r"!\[([^\]]*)\]\(gif-preview-([a-f0-9]+)\)").unwrap();
    for cap in img_re.captures_iter(markdown) {
        if let (Some(m), Some(key_match)) = (cap.get(0), cap.get(2)) {
            images.push((
                m.start(),
                m.end(),
                format!("gif-preview-{}", key_match.as_str()),
            ));
        }
    }
    images
}

fn extract_youtube_video_id(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let patterns = [
        r#"(?i)(?:youtube\.com/embed/)([A-Za-z0-9_-]{6,})"#,
        r#"(?i)(?:youtube\.com/watch\?[^"'\s<>]*v=)([A-Za-z0-9_-]{6,})"#,
        r#"(?i)(?:youtu\.be/)([A-Za-z0-9_-]{6,})"#,
    ];
    for pattern in patterns {
        let re = Regex::new(pattern).ok()?;
        if let Some(caps) = re.captures(trimmed) {
            if let Some(video_id) = caps.get(1) {
                return Some(video_id.as_str().to_string());
            }
        }
    }
    None
}

fn youtube_watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn extract_markdown_youtube_embeds(markdown: &str) -> Vec<(usize, usize, String)> {
    let Ok(re) = Regex::new(r"\{\{hestia-youtube:([A-Za-z0-9_-]{6,})\}\}") else {
        return Vec::new();
    };
    re.captures_iter(markdown)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            let video_id = caps.get(1)?.as_str().to_string();
            Some((whole.start(), whole.end(), youtube_watch_url(&video_id)))
        })
        .collect()
}

fn save_mod_image_from_url_bg(
    portable: &persistence::PortablePaths,
    client: &reqwest::blocking::Client,
    mod_root_path: &Path,
    mod_id: u64,
    idx: usize,
    full_url: &str,
) -> Result<String> {
    let meta_dir = mod_root_path.join(crate::model::MOD_META_DIR);
    fs::create_dir_all(&meta_dir)?;
    let path_no_query = full_url.split('?').next().unwrap_or(full_url);
    let ext = Path::new(path_no_query)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jpg");
    let file_name = format!("gb_{mod_id}_{}.{ext}", idx + 1);
    let abs_path = meta_dir.join(&file_name);

    let cache_key = format!("img:{}", hash64_hex(full_url.as_bytes()));
    let bytes = if let Some(cached) = persistence::cache_get(portable, &cache_key)? {
        cached
    } else {
        client.get(full_url).send()?.error_for_status()?.bytes()?.to_vec()
    };
    fs::write(&abs_path, &bytes)?;
    Ok(format!("{MOD_META_DIR}\\{file_name}"))
}

fn cache_description_images_to_mod_bg(
    portable: &persistence::PortablePaths,
    client: &reqwest::blocking::Client,
    mod_root_path: &Path,
    mod_id: u64,
    html_text: Option<&str>,
) -> Result<()> {
    let Some(html_text) = html_text else {
        return Ok(());
    };
    for url in extract_image_urls_from_html_like_text(html_text) {
        let _ = save_mod_description_image_from_url_bg(
            portable,
            client,
            mod_root_path,
            mod_id,
            &url,
        );
    }
    Ok(())
}

fn save_mod_description_image_from_url_bg(
    portable: &persistence::PortablePaths,
    client: &reqwest::blocking::Client,
    mod_root_path: &Path,
    mod_id: u64,
    full_url: &str,
) -> Result<String> {
    let meta_dir = mod_root_path.join(crate::model::MOD_META_DIR);
    fs::create_dir_all(&meta_dir)?;
    let path_no_query = full_url.split('?').next().unwrap_or(full_url);
    let ext = Path::new(path_no_query)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jpg");
    let url_hash = hash64_hex(full_url.as_bytes());
    let file_name = format!("gb_desc_{mod_id}_{url_hash}.{ext}");
    let abs_path = meta_dir.join(&file_name);

    let cache_key = format!("img:{}", hash64_hex(full_url.as_bytes()));
    let bytes = if let Some(cached) = persistence::cache_get(portable, &cache_key)? {
        cached
    } else {
        client.get(full_url).send()?.error_for_status()?.bytes()?.to_vec()
    };
    fs::write(&abs_path, &bytes)?;
    Ok(format!("{MOD_META_DIR}\\{file_name}"))
}

fn persist_source_images_bg(
    portable: &persistence::PortablePaths,
    mod_root_path: &Path,
    profile: &gamebanana::ProfileResponse,
    client: &reqwest::blocking::Client,
) -> Result<Vec<String>> {
    let mut rel_paths = Vec::new();
    if let Some(preview) = &profile.preview_media {
        for (idx, image) in preview.images.iter().enumerate() {
            let full_url = gamebanana::full_image_url(image);
            rel_paths.push(save_mod_image_from_url_bg(
                portable,
                client,
                mod_root_path,
                profile.id,
                idx,
                &full_url,
            )?);
        }
    }

    let _ = cache_description_images_to_mod_bg(
        portable,
        client,
        mod_root_path,
        profile.id,
        profile.html_text.as_deref(),
    );

    Ok(rel_paths)
}

fn extract_image_urls_from_html_like_text(text: &str) -> Vec<String> {
    let is_placeholder = |url: &str| {
        url.eq_ignore_ascii_case(
            "https://images.gamebanana.com/static/img/mascots/detective.png",
        )
    };
    let mut urls = Vec::new();
    if let Ok(img_re) = Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#) {
        for cap in img_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let url = m.as_str().trim();
                if (url.starts_with("http://") || url.starts_with("https://")) && !is_placeholder(url)
                {
                    urls.push(url.to_string());
                }
            }
        }
    }
    if let Ok(md_re) = Regex::new(r"!\[[^\]]*\]\(([^)]+)\)") {
        for cap in md_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let url = m.as_str().trim();
                if (url.starts_with("http://") || url.starts_with("https://")) && !is_placeholder(url)
                {
                    urls.push(url.to_string());
                }
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

fn extract_image_urls_from_profile_json(raw_profile_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw_profile_json) else {
        return Vec::new();
    };
    let Some(html) = value.get("_sText").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    extract_image_urls_from_html_like_text(html)
}

fn mod_primary_description_markdown(mod_entry: &ModEntry, portable: &persistence::PortablePaths) -> String {
    if let Some(html) = mod_entry
        .metadata
        .user
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
    {
        return prepare_markdown_for_display(
            html,
            Some(&mod_entry.root_path),
            Some(parse_gb_id_from_entry(mod_entry)),
            portable,
        );
    }

    if let Some(raw) = mod_entry.source.as_ref().and_then(|s| s.raw_profile_json.as_deref()) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(html) = value.get("_sText").and_then(|v| v.as_str()) {
                return prepare_markdown_for_display(
                    html,
                    None,
                    Some(parse_gb_id_from_entry(mod_entry)),
                    portable,
                );
            }
        }
    }

    if let Some(html) = mod_entry
        .source
        .as_ref()
        .and_then(|s| s.snapshot.as_ref())
        .and_then(|s| s.description.as_deref())
        .filter(|d| !d.trim().is_empty())
    {
        return prepare_markdown_for_display(
            html,
            Some(&mod_entry.root_path),
            Some(parse_gb_id_from_entry(mod_entry)),
            portable,
        );
    }

    "No description".to_string()
}

fn mod_extracted_description_markdown(mod_entry: &ModEntry) -> Option<String> {
    mod_entry
        .metadata
        .extracted
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(ToString::to_string)
}

fn prepare_markdown_for_display(
    html: &str,
    mod_root: Option<&Path>,
    gb_id: Option<u64>,
    portable: &PortablePaths,
) -> String {
    let detective = "https://images.gamebanana.com/static/img/mascots/detective.png";
    let img_re = Regex::new(r#"(?is)<img\b(?P<attrs>[^>]*?)>"#).unwrap();
    let iframe_re = Regex::new(r#"(?is)<iframe\b(?P<attrs>[^>]*?)>(?:\s*</iframe>)?"#).unwrap();
    let src_re = Regex::new(r#"(?i)\bsrc\s*=\s*["']([^"']+)["']"#).unwrap();
    let data_src_re =
        Regex::new(r#"(?i)\b(?:data-src|data-preview)\s*=\s*["']([^"']+)["']"#).unwrap();

    let iframe_sanitized_html = iframe_re.replace_all(html, |caps: &regex::Captures| {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let src = src_re
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or_default();
        if let Some(video_id) = extract_youtube_video_id(src) {
            format!("\n\n{{{{hestia-youtube:{video_id}}}}}\n\n")
        } else {
            String::new()
        }
    });

    let sanitized_html = img_re.replace_all(&iframe_sanitized_html, |caps: &regex::Captures| {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let src = src_re
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or_default();
        let data = data_src_re
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or_default();

        let real_url = if src == detective || (src.is_empty() && !data.is_empty()) {
            data
        } else {
            src
        };
        if real_url.is_empty() || real_url == detective {
            return String::new();
        }
        format!(r#"<img src="{real_url}">"#)
    });

    let markdown = html2md::parse_html(&sanitized_html);
    rewrite_markdown_urls(&markdown, mod_root, gb_id, portable)
}

fn rewrite_markdown_urls(
    markdown: &str,
    mod_root: Option<&Path>,
    gb_id: Option<u64>,
    _portable: &PortablePaths,
) -> String {
    MARKDOWN_IMAGE_DEST_RE
        .replace_all(markdown, |caps: &regex::Captures| {
            let alt = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let dest_raw = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let url = normalize_markdown_image_dest(dest_raw);

            if !url.starts_with("http") {
                return caps[0].to_string();
            }

            if let (Some(root), Some(mid)) = (mod_root, gb_id) {
                if mid > 0 {
                    let meta_dir = root.join(MOD_META_DIR);
                    let url_hash = hash64_hex(url.as_bytes());
                    let prefix = format!("gb_desc_{mid}_{url_hash}.");
                    if let Ok(entries) = fs::read_dir(meta_dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with(&prefix) {
                                let uri = path_to_file_uri(&entry.path());
                                return format!("![{alt}](<{uri}>)");
                            }
                        }
                    }
                }
            }

            if is_gif_dest(&url) {
                return caps[0].to_string();
            }

            let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
            let cache_path = persistence::cache_file_path(&cache_key);
            if cache_path.exists() {
                let uri = path_to_file_uri(&cache_path);
                return format!("![{alt}](<{uri}>)");
            }

            caps[0].to_string()
        })
        .to_string()
}

fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut i = 0;
    let s_bytes = s.as_bytes();
    while i < s_bytes.len() {
        if s_bytes[i] == b'%' && i + 2 < s_bytes.len() {
            if let (Some(h), Some(l)) = (
                (s_bytes[i + 1] as char).to_digit(16),
                (s_bytes[i + 2] as char).to_digit(16),
            ) {
                bytes.push((h << 4 | l) as u8);
                i += 3;
                continue;
            }
        }
        bytes.push(s_bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn path_to_file_uri(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let mut uri = String::new();
    for (i, c) in s.chars().enumerate() {
        if i == 0 && s.get(1..2) == Some(":") {
            uri.push('/');
        }
        match c {
            '\\' => uri.push('/'),
            ' ' => uri.push_str("%20"),
            '#' => uri.push_str("%23"),
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            _ => uri.push(c),
        }
    }
    format!("file:///{uri}")
}
