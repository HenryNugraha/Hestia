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

fn markdown_static_image_texture_key(dest: &str) -> String {
    format!(
        "{}:{}",
        ThumbnailProfile::Rail.suffix(),
        hash64_hex(dest.as_bytes())
    )
}

fn extract_markdown_images(markdown: &str) -> Vec<(usize, usize, String)> {
    let mut images = Vec::new();
    let img_re = Regex::new(r"!\[([^\]]*)\]\((gif-preview-[a-f0-9]+|rail:[a-f0-9]+)\)").unwrap();
    for cap in img_re.captures_iter(markdown) {
        if let (Some(m), Some(key_match)) = (cap.get(0), cap.get(2)) {
            images.push((m.start(), m.end(), key_match.as_str().to_string()));
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
    let bytes = fetch_valid_image_bytes_bg(portable, client, full_url, &cache_key)?;
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
        let _ =
            save_mod_description_image_from_url_bg(portable, client, mod_root_path, mod_id, &url);
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
    let bytes = fetch_valid_image_bytes_bg(portable, client, full_url, &cache_key)?;
    fs::write(&abs_path, &bytes)?;
    Ok(format!("{MOD_META_DIR}\\{file_name}"))
}

fn fetch_valid_image_bytes_bg(
    portable: &persistence::PortablePaths,
    client: &reqwest::blocking::Client,
    url: &str,
    cache_key: &str,
) -> Result<Vec<u8>> {
    if let Some(cached) = persistence::cache_get(portable, cache_key)? {
        if image_bytes_are_decodable(&cached) {
            return Ok(cached);
        }
        let _ = persistence::cache_remove(portable, cache_key);
    }

    let bytes = client
        .get(url)
        .send()?
        .error_for_status()?
        .bytes()?
        .to_vec();
    if !image_bytes_are_decodable(&bytes) {
        bail!("downloaded image bytes could not be decoded: {url}");
    }
    Ok(bytes)
}

fn image_bytes_are_decodable(bytes: &[u8]) -> bool {
    decode_limited_dynamic_image(bytes).is_ok()
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
        url.eq_ignore_ascii_case("https://images.gamebanana.com/static/img/mascots/detective.png")
    };
    let mut urls = Vec::new();
    if let Ok(img_re) = Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#) {
        for cap in img_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let url = m.as_str().trim();
                if (url.starts_with("http://") || url.starts_with("https://"))
                    && !is_placeholder(url)
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
                if (url.starts_with("http://") || url.starts_with("https://"))
                    && !is_placeholder(url)
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

fn mod_primary_description_markdown(
    mod_entry: &ModEntry,
    portable: &persistence::PortablePaths,
) -> String {
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

    if let Some(raw) = mod_entry
        .source
        .as_ref()
        .and_then(|s| s.raw_profile_json.as_deref())
    {
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
                                let path = entry.path();
                                match fs::read(&path) {
                                    Ok(bytes) if image_bytes_are_decodable(&bytes) => {
                                        let uri = path_to_file_uri(&path);
                                        return format!("![{alt}](<{uri}>)");
                                    }
                                    _ => {
                                        let _ = fs::remove_file(path);
                                    }
                                }
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
                match fs::read(&cache_path) {
                    Ok(bytes) if image_bytes_are_decodable(&bytes) => {
                        let uri = path_to_file_uri(&cache_path);
                        return format!("![{alt}](<{uri}>)");
                    }
                    _ => {
                        let _ = fs::remove_file(&cache_path);
                    }
                }
            }

            caps[0].to_string()
        })
        .to_string()
}

fn is_hestia_controlled_image_path(path: &Path, mod_root: Option<&Path>) -> bool {
    if path.starts_with(persistence::runtime_temp_cache_dir()) {
        return true;
    }

    mod_root
        .map(|root| path.starts_with(root.join(MOD_META_DIR)))
        .unwrap_or(false)
}

fn rewrite_local_markdown_image_for_render(
    alt: &str,
    dest: &str,
    mod_root: Option<&Path>,
) -> Option<String> {
    if dest.starts_with("gif-preview-") {
        return Some(format!("![{alt}]({dest})"));
    }
    let _ = mod_root;
    None
}

fn markdown_image_dest_allowed_for_render(dest: &str, mod_root: Option<&Path>) -> bool {
    let dest = normalize_markdown_image_dest(dest);
    if dest.starts_with("gif-preview-") || dest.starts_with("rail:") {
        return true;
    }

    let Some(path) = file_uri_to_path(&dest) else {
        return false;
    };
    if !is_hestia_controlled_image_path(&path, mod_root) {
        return false;
    }

    fs::read(&path).is_ok_and(|bytes| image_bytes_are_decodable(&bytes))
}

fn strip_untrusted_markdown_images(markdown: &str, mod_root: Option<&Path>) -> String {
    let mut ranges = Vec::new();
    for (event, range) in pulldown_cmark::Parser::new(markdown).into_offset_iter() {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                if !markdown_image_dest_allowed_for_render(&dest_url, mod_root) {
                    ranges.push(range);
                }
            }
            pulldown_cmark::Event::Html(html) | pulldown_cmark::Event::InlineHtml(html) => {
                if html.to_ascii_lowercase().contains("<img") {
                    ranges.push(range);
                }
            }
            _ => {}
        }
    }

    if ranges.is_empty() {
        return markdown.to_string();
    }

    ranges.sort_by_key(|range| range.start);
    let mut out = String::with_capacity(markdown.len());
    let mut cursor = 0usize;
    for range in ranges {
        let start = range.start.min(markdown.len());
        let end = range.end.min(markdown.len());
        if start < cursor || start > end {
            continue;
        }
        out.push_str(&markdown[cursor..start]);
        cursor = end;
    }
    out.push_str(&markdown[cursor..]);
    out
}

fn append_markdown_image_dependency(sig: &mut String, dest: &str, mod_root: Option<&Path>) {
    let dest = normalize_markdown_image_dest(dest);
    let lower_dest = dest.to_ascii_lowercase();
    if dest.starts_with("gif-preview-") {
        sig.push_str("|gif:");
        sig.push_str(&dest);
        return;
    }
    if dest.starts_with("rail:") {
        sig.push_str("|texture:");
        sig.push_str(&dest);
        return;
    }

    let path = if lower_dest.starts_with("http://") || lower_dest.starts_with("https://") {
        if is_gif_dest(&dest) {
            sig.push_str("|remote-gif:");
            sig.push_str(&hash64_hex(dest.as_bytes()));
            return;
        }
        Some(persistence::cache_file_path(&format!(
            "img:{}",
            hash64_hex(dest.as_bytes())
        )))
    } else {
        file_uri_to_path(&dest).filter(|path| is_hestia_controlled_image_path(path, mod_root))
    };

    let Some(path) = path else {
        sig.push_str("|blocked:");
        sig.push_str(&hash64_hex(dest.as_bytes()));
        return;
    };
    sig.push('|');
    sig.push_str(&path.to_string_lossy());
    match fs::metadata(&path) {
        Ok(meta) => {
            sig.push(':');
            sig.push_str(&meta.len().to_string());
            if let Ok(modified) = meta.modified()
                && let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                sig.push(':');
                sig.push_str(&duration.as_nanos().to_string());
            }
        }
        Err(_) => sig.push_str(":missing"),
    }
}

fn markdown_image_dependency_signature(markdown: &str, mod_root: Option<&Path>) -> String {
    let mut sig = String::new();
    for cap in MARKDOWN_IMAGE_DEST_RE.captures_iter(markdown) {
        if let Some(dest) = cap.get(2) {
            append_markdown_image_dependency(&mut sig, dest.as_str(), mod_root);
        }
    }
    for (event, _) in pulldown_cmark::Parser::new(markdown).into_offset_iter() {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event {
            append_markdown_image_dependency(&mut sig, &dest_url, mod_root);
        }
    }
    sig
}

fn rewrite_markdown_remote_images_for_render(
    markdown: &str,
    portable: &PortablePaths,
    mod_root: Option<&Path>,
) -> String {
    let markdown = MARKDOWN_IMAGE_DEST_RE
        .replace_all(markdown, |caps: &regex::Captures| {
            let alt = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let dest_raw = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let url = normalize_markdown_image_dest(dest_raw);
            let lower_url = url.to_ascii_lowercase();

            if !lower_url.starts_with("http://") && !lower_url.starts_with("https://") {
                return rewrite_local_markdown_image_for_render(alt, &url, mod_root)
                    .unwrap_or_default();
            }

            if is_gif_dest(&url) {
                return String::new();
            }

            let texture_key = markdown_static_image_texture_key(&url);
            let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
            let cache_path = persistence::cache_file_path(&cache_key);
            if cache_path.exists() {
                match fs::read(&cache_path) {
                    Ok(bytes) if image_bytes_are_decodable(&bytes) => {
                        return format!("![{alt}]({texture_key})");
                    }
                    _ => {
                        let _ = persistence::cache_remove(portable, &cache_key);
                    }
                }
            }

            String::new()
        })
        .to_string();
    strip_untrusted_markdown_images(&markdown, mod_root)
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

#[cfg(test)]
mod markdown_render_image_tests {
    use super::*;

    fn dummy_portable_paths() -> PortablePaths {
        PortablePaths {
            state_archive: PathBuf::from("test-state.toml"),
            state_source: None,
            history_db: PathBuf::from("test-history.dat"),
        }
    }

    fn tiny_png_bytes() -> Vec<u8> {
        use image::ImageEncoder;

        let mut out = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .expect("test png should encode");
        out
    }

    #[test]
    fn render_rewrite_suppresses_uncached_remote_static_images() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/static.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);

        let markdown = format!("before ![alt]({url}) after");
        let rendered = rewrite_markdown_remote_images_for_render(&markdown, &portable, None);

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn render_rewrite_suppresses_raw_remote_gif_images() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/anim.gif";

        let markdown = format!("before ![alt]({url}) after");
        let rendered = rewrite_markdown_remote_images_for_render(&markdown, &portable, None);

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn prepare_preserves_uncached_remote_static_images_for_prewarm() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/prewarm.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);

        let markdown = prepare_markdown_for_display(
            &format!(r#"<p>body</p><img src="{url}">"#),
            None,
            None,
            &portable,
        );

        assert!(markdown.contains(url));
    }

    #[test]
    fn render_rewrite_uses_valid_cached_remote_static_images() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/cached.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);
        persistence::cache_put(&portable, &cache_key, "test-image", &tiny_png_bytes(), 0)
            .expect("test cache write should succeed");

        let markdown = format!("![alt]({url})");
        let rendered = rewrite_markdown_remote_images_for_render(&markdown, &portable, None);

        assert_eq!(
            rendered,
            format!("![alt]({})", markdown_static_image_texture_key(url))
        );

        let _ = persistence::cache_remove(&portable, &cache_key);
    }

    #[test]
    fn render_dependency_signature_changes_when_remote_cache_appears() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/cache-appears.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);
        let markdown = format!("![alt]({url})");

        let missing = markdown_image_dependency_signature(&markdown, None);
        persistence::cache_put(&portable, &cache_key, "test-image", &tiny_png_bytes(), 0)
            .expect("test cache write should succeed");
        let present = markdown_image_dependency_signature(&markdown, None);

        assert_ne!(missing, present);

        let _ = persistence::cache_remove(&portable, &cache_key);
    }

    #[test]
    fn render_rewrite_suppresses_data_image_dest() {
        let portable = dummy_portable_paths();
        let markdown = "before ![alt](data:image/png;base64,AAAA) after";
        let rendered = rewrite_markdown_remote_images_for_render(markdown, &portable, None);

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn render_rewrite_suppresses_arbitrary_file_uri() {
        let portable = dummy_portable_paths();
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let image_path = temp.path().join("external.png");
        fs::write(&image_path, tiny_png_bytes()).expect("test image should be written");

        let markdown = format!("before ![alt](<{}>) after", path_to_file_uri(&image_path));
        let rendered = rewrite_markdown_remote_images_for_render(&markdown, &portable, None);

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn render_rewrite_suppresses_reference_style_remote_image() {
        let portable = dummy_portable_paths();
        let markdown =
            "before ![alt][gb] after\n\n[gb]: https://images.gamebanana.com/example/ref.png";
        let rendered = rewrite_markdown_remote_images_for_render(markdown, &portable, None);

        assert_eq!(
            rendered,
            "before  after\n\n[gb]: https://images.gamebanana.com/example/ref.png"
        );
    }

    #[test]
    fn render_rewrite_suppresses_shortcut_reference_image() {
        let portable = dummy_portable_paths();
        let markdown = "before ![gb] after\n\n[gb]: https://images.gamebanana.com/example/ref.png";
        let rendered = rewrite_markdown_remote_images_for_render(markdown, &portable, None);

        assert_eq!(
            rendered,
            "before  after\n\n[gb]: https://images.gamebanana.com/example/ref.png"
        );
    }

    #[test]
    fn render_rewrite_suppresses_raw_html_img() {
        let portable = dummy_portable_paths();
        let markdown = r#"before <img src="https://images.gamebanana.com/example/raw.png"> after"#;
        let rendered = rewrite_markdown_remote_images_for_render(markdown, &portable, None);

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn render_rewrite_suppresses_valid_hestia_static_file_uri() {
        let portable = dummy_portable_paths();
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let meta_dir = temp.path().join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).expect("meta dir should be created");
        let image_path = meta_dir.join("local.png");
        fs::write(&image_path, tiny_png_bytes()).expect("test image should be written");

        let markdown = format!("![alt](<{}>)", path_to_file_uri(&image_path));
        let rendered =
            rewrite_markdown_remote_images_for_render(&markdown, &portable, Some(temp.path()));

        assert_eq!(rendered, "");
    }

    #[test]
    fn render_rewrite_suppresses_hestia_named_file_outside_mod_root() {
        let portable = dummy_portable_paths();
        let trusted_root = tempfile::tempdir().expect("trusted temp dir should be created");
        let untrusted_root = tempfile::tempdir().expect("untrusted temp dir should be created");
        let meta_dir = untrusted_root.path().join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).expect("meta dir should be created");
        let image_path = meta_dir.join("local.png");
        fs::write(&image_path, tiny_png_bytes()).expect("test image should be written");

        let markdown = format!("before ![alt](<{}>) after", path_to_file_uri(&image_path));
        let rendered = rewrite_markdown_remote_images_for_render(
            &markdown,
            &portable,
            Some(trusted_root.path()),
        );

        assert_eq!(rendered, "before  after");
    }

    #[test]
    fn render_rewrite_keeps_gif_preview_texture_key() {
        let portable = dummy_portable_paths();
        let markdown = "before ![alt](gif-preview-abcdef1234) after";
        let rendered = rewrite_markdown_remote_images_for_render(markdown, &portable, None);

        assert_eq!(rendered, markdown);
    }

    #[test]
    fn prepare_removes_invalid_persisted_description_image_file() {
        let portable = dummy_portable_paths();
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let meta_dir = temp.path().join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).expect("meta dir should be created");
        let url = "https://images.gamebanana.com/example/desc.png";
        let url_hash = hash64_hex(url.as_bytes());
        let image_path = meta_dir.join(format!("gb_desc_42_{url_hash}.png"));
        fs::write(&image_path, b"not an image").expect("invalid test image should be written");

        let markdown = prepare_markdown_for_display(
            &format!(r#"<p>body</p><img src="{url}">"#),
            Some(temp.path()),
            Some(42),
            &portable,
        );

        assert!(markdown.contains(url));
        assert!(!image_path.exists());
    }
}
