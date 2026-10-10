static RENDER_EMBED_IMAGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\((gif-preview-[a-f0-9]+|rail:[a-f0-9]+)\)").unwrap());
static YOUTUBE_EMBED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{hestia-youtube:([A-Za-z0-9_-]{6,})\}\}").unwrap());
static YOUTUBE_URL_RES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r#"(?i)(?:youtube\.com/embed/)([A-Za-z0-9_-]{6,})"#,
        r#"(?i)(?:youtube\.com/watch\?[^"'\s<>]*v=)([A-Za-z0-9_-]{6,})"#,
        r#"(?i)(?:youtu\.be/)([A-Za-z0-9_-]{6,})"#,
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});
static HTML_IMAGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)<img\b(?P<attrs>[^>]*?)>"#).unwrap());
static HTML_IFRAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)<iframe\b(?P<attrs>[^>]*?)>(?:\s*</iframe>)?"#).unwrap());
static HTML_SRC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bsrc\s*=\s*["']([^"']+)["']"#).unwrap());
static HTML_DATA_SRC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\b(?:data-src|data-preview)\s*=\s*["']([^"']+)["']"#).unwrap());

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

fn gif_anim_cache_path(dest: &str) -> PathBuf {
    let key = hash64_hex(dest.as_bytes());
    persistence::runtime_temp_cache_dir()
        .join("gif_anim")
        .join(format!("{key}.gif"))
}

fn gif_preview_out_path(dest: &str, mod_root: Option<&Path>) -> PathBuf {
    let key = hash64_hex(dest.as_bytes());
    let base = if let Some(root) = mod_root {
        root.join(MOD_META_DIR).join("description_gif_previews")
    } else {
        persistence::runtime_temp_cache_dir().join("gif_previews")
    };
    base.join(format!("{key}.png"))
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
    for cap in RENDER_EMBED_IMAGE_RE.captures_iter(markdown) {
        if let (Some(m), Some(key_match)) = (cap.get(0), cap.get(2)) {
            images.push((m.start(), m.end(), key_match.as_str().to_string()));
        }
    }
    images
}

fn extract_youtube_video_id(url: &str) -> Option<String> {
    let trimmed = url.trim();
    for re in YOUTUBE_URL_RES.iter() {
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
    YOUTUBE_EMBED_RE
        .captures_iter(markdown)
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
        if is_gif_dest(url) || image_bytes_are_decodable(&cached) {
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
    if !is_gif_dest(url) && !image_bytes_are_decodable(&bytes) {
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
    mut on_first_preview: impl FnMut(&str),
) -> Result<Vec<String>> {
    let mut rel_paths = Vec::new();
    if let Some(preview) = &profile.preview_media {
        for (idx, image) in preview.images.iter().enumerate() {
            let full_url = gamebanana::full_image_url(image);
            let rel_path = save_mod_image_from_url_bg(
                portable,
                client,
                mod_root_path,
                profile.id,
                idx,
                &full_url,
            )?;
            if idx == 0 {
                on_first_preview(&rel_path);
            }
            rel_paths.push(rel_path);
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

static HTML_IMG_SRC_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#).unwrap());
static MARKDOWN_IMAGE_LINK_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[[^\]]*\]\(([^)]+)\)").unwrap());

fn extract_image_urls_from_html_like_text(text: &str) -> Vec<String> {
    let is_placeholder = |url: &str| {
        url.eq_ignore_ascii_case("https://images.gamebanana.com/static/img/mascots/detective.png")
    };
    let mut urls = Vec::new();
    for cap in HTML_IMG_SRC_URL_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let url = m.as_str().trim();
            if (url.starts_with("http://") || url.starts_with("https://")) && !is_placeholder(url) {
                urls.push(url.to_string());
            }
        }
    }
    for cap in MARKDOWN_IMAGE_LINK_URL_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let url = m.as_str().trim();
            if (url.starts_with("http://") || url.starts_with("https://")) && !is_placeholder(url) {
                urls.push(url.to_string());
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

/// The Library detail pane keeps one small cache for the selected mod. HTML to Markdown
/// conversion is pure and relatively expensive, while URL rewriting below it depends on files
/// arriving in the mod metadata directory and the shared image cache. Keep those two stages
/// separate so a prepared description can be reused without making a missing image permanent.
#[derive(Clone, Default)]
struct LibraryDetailContentCache {
    mod_id: String,
    prepared_markdown: Vec<LibraryDetailPreparedMarkdown>,
    raw_profile: Option<LibraryDetailRawProfile>,
    #[cfg(test)]
    prepared_builds: usize,
    #[cfg(test)]
    raw_profile_parses: usize,
}

#[derive(Clone, PartialEq, Eq)]
struct LibraryDetailPreparedMarkdown {
    key: LibraryDetailMarkdownKey,
    markdown: String,
}

#[derive(Clone, PartialEq, Eq)]
struct LibraryDetailMarkdownKey {
    content_hash: u64,
    content_len: usize,
    mod_root: Option<PathBuf>,
    gb_id: Option<u64>,
}

#[derive(Clone)]
struct LibraryDetailRawProfile {
    content_hash: u64,
    content_len: usize,
    description: Option<String>,
    preview_captions: Vec<Option<String>>,
}

impl LibraryDetailContentCache {
    const MAX_PREPARED_VARIANTS: usize = 8;

    fn reset_for_mod(&mut self, mod_id: &str) {
        if self.mod_id == mod_id {
            return;
        }
        self.mod_id.clear();
        self.mod_id.push_str(mod_id);
        self.prepared_markdown.clear();
        self.raw_profile = None;
    }

    fn prepared_markdown(
        &mut self,
        mod_id: &str,
        html: &str,
        mod_root: Option<&Path>,
        gb_id: Option<u64>,
    ) -> String {
        self.reset_for_mod(mod_id);
        let key = LibraryDetailMarkdownKey {
            content_hash: xxh3_64(html.as_bytes()),
            content_len: html.len(),
            mod_root: mod_root.map(Path::to_path_buf),
            gb_id,
        };
        if let Some(cached) = self
            .prepared_markdown
            .iter()
            .find(|cached| cached.key == key)
        {
            return cached.markdown.clone();
        }

        let markdown = prepare_markdown_content(html);
        #[cfg(test)]
        {
            self.prepared_builds += 1;
        }
        if self.prepared_markdown.len() >= Self::MAX_PREPARED_VARIANTS {
            self.prepared_markdown.remove(0);
        }
        self.prepared_markdown.push(LibraryDetailPreparedMarkdown {
            key,
            markdown: markdown.clone(),
        });
        markdown
    }

    fn raw_profile_description(&mut self, mod_id: &str, raw_profile_json: &str) -> Option<String> {
        self.ensure_raw_profile(mod_id, raw_profile_json);
        self.raw_profile
            .as_ref()
            .and_then(|profile| profile.description.clone())
    }

    fn raw_profile_preview_captions(
        &mut self,
        mod_id: &str,
        raw_profile_json: &str,
    ) -> Vec<Option<String>> {
        self.ensure_raw_profile(mod_id, raw_profile_json);
        self.raw_profile
            .as_ref()
            .map(|profile| profile.preview_captions.clone())
            .unwrap_or_default()
    }

    fn ensure_raw_profile(&mut self, mod_id: &str, raw_profile_json: &str) {
        self.reset_for_mod(mod_id);
        let content_hash = xxh3_64(raw_profile_json.as_bytes());
        let content_len = raw_profile_json.len();
        if self.raw_profile.as_ref().is_some_and(|profile| {
            profile.content_hash == content_hash && profile.content_len == content_len
        }) {
            return;
        }

        let (description, preview_captions) = serde_json::from_str::<serde_json::Value>(
            raw_profile_json,
        )
        .ok()
        .map(|value| {
            let description = value
                .get("_sText")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            let preview_captions = value
                .get("_aPreviewMedia")
                .and_then(|media| media.get("_aImages"))
                .and_then(|images| images.as_array())
                .map(|images| {
                    images
                        .iter()
                        .map(|image| {
                            image
                                .get("_sCaption")
                                .and_then(|caption| caption.as_str())
                                .map(ToString::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (description, preview_captions)
        })
        .unwrap_or_default();

        #[cfg(test)]
        {
            self.raw_profile_parses += 1;
        }
        self.raw_profile = Some(LibraryDetailRawProfile {
            content_hash,
            content_len,
            description,
            preview_captions,
        });
    }
}

fn cached_library_prepared_markdown(
    cache: &mut Option<LibraryDetailContentCache>,
    mod_id: &str,
    html: &str,
    mod_root: Option<&Path>,
    gb_id: Option<u64>,
) -> String {
    cache
        .get_or_insert_with(LibraryDetailContentCache::default)
        .prepared_markdown(mod_id, html, mod_root, gb_id)
}

/// Convert HTML to the Markdown form consumed by egui_commonmark. This deliberately excludes
/// filesystem-dependent URL resolution; callers that render the result must still run
/// `rewrite_markdown_urls` / `cached_rewrite_markdown_for_render` afterwards.
fn prepare_markdown_content(html: &str) -> String {
    let detective = "https://images.gamebanana.com/static/img/mascots/detective.png";
    let iframe_sanitized_html = HTML_IFRAME_RE.replace_all(html, |caps: &regex::Captures| {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let src = HTML_SRC_RE
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

    let sanitized_html =
        HTML_IMAGE_RE.replace_all(&iframe_sanitized_html, |caps: &regex::Captures| {
            let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let src = HTML_SRC_RE
                .captures(attrs)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str())
                .unwrap_or_default();
            let data = HTML_DATA_SRC_RE
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

    let mut handlers: HashMap<String, Box<dyn html2md::TagHandlerFactory>> = HashMap::new();
    handlers.insert("a".to_string(), Box::new(VerbatimLinkHandler::default));
    html2md::parse_html_custom(&sanitized_html, &handlers)
}

/// html2md's own link handler percent-decodes link targets, which turns `%26` into `&`, `%23`
/// into `#` and `%29` into `)` and breaks real links. Same handler, minus the decoding.
#[derive(Default)]
struct VerbatimLinkHandler {
    start_pos: usize,
    url: String,
    emit_unchanged: bool,
}

impl html2md::TagHandler for VerbatimLinkHandler {
    fn handle(&mut self, tag: &html2md::Handle, printer: &mut html2md::StructuredPrinter) {
        // Markdown has no named anchors, so those stay raw HTML.
        if html2md::common::get_tag_attr(tag, "name").is_some() {
            html2md::TagHandler::handle(&mut html2md::dummy::IdentityHandler, tag, printer);
            self.emit_unchanged = true;
        }
        self.start_pos = printer.data.len();
        let href = html2md::common::get_tag_attr(tag, "href").unwrap_or_default();
        let href = href.trim_matches(|c: char| c.is_ascii_whitespace());
        self.url = if href.contains(|c: char| c.is_ascii_whitespace()) {
            format!("<{href}>")
        } else {
            href.to_string()
        };
    }

    fn after_handle(&mut self, printer: &mut html2md::StructuredPrinter) {
        if !self.emit_unchanged {
            printer.insert_str(self.start_pos, "[");
            printer.append_str(&format!("]({})", self.url));
        }
    }
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
    let markdown = prepare_markdown_content(html);
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

            if is_gif_dest(&url) {
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
                                if path.is_file() {
                                    let uri = path_to_file_uri(&path);
                                    return format!("![{alt}](<{uri}>)");
                                }
                            }
                        }
                    }
                }
            }

            let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
            let cache_path = persistence::cache_file_path(&cache_key);
            if cache_path.is_file() {
                let uri = path_to_file_uri(&cache_path);
                return format!("![{alt}](<{uri}>)");
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

    path.is_file()
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
    _portable: &PortablePaths,
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
            if cache_path.is_file() {
                return format!("![{alt}]({texture_key})");
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
        persistence::cache_put(
            &portable,
            &cache_key,
            "test-image",
            &tiny_png_bytes(),
            1024 * 1024,
        )
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
        persistence::cache_put(
            &portable,
            &cache_key,
            "test-image",
            &tiny_png_bytes(),
            1024 * 1024,
        )
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
    fn prepare_keeps_persisted_static_description_image_without_decoding() {
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

        assert!(markdown.contains(&path_to_file_uri(&image_path)));
        assert!(image_path.exists());
    }

    #[test]
    fn prepare_keeps_persisted_gif_on_remote_gif_path_without_reading_file() {
        let portable = dummy_portable_paths();
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let meta_dir = temp.path().join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).expect("meta dir should be created");
        let url = "https://images.gamebanana.com/example/desc.gif";
        let url_hash = hash64_hex(url.as_bytes());
        let image_path = meta_dir.join(format!("gb_desc_42_{url_hash}.gif"));
        fs::write(&image_path, b"not a gif").expect("test gif placeholder should be written");

        let markdown = prepare_markdown_for_display(
            &format!(r#"<p>body</p><img src="{url}">"#),
            Some(temp.path()),
            Some(42),
            &portable,
        );

        assert!(markdown.contains(url));
        assert!(!markdown.contains(&path_to_file_uri(&image_path)));
        assert!(image_path.exists());
    }

    #[test]
    fn render_rewrite_uses_cached_remote_static_image_without_decoding() {
        let portable = dummy_portable_paths();
        let url = "https://images.gamebanana.com/example/cached-invalid.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);
        persistence::cache_put(
            &portable,
            &cache_key,
            "test-image",
            b"not an image",
            1024 * 1024,
        )
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
    fn library_detail_prepared_content_reuses_unchanged_input_and_invalidates_inputs() {
        let mut cache = LibraryDetailContentCache::default();
        let html = "<p>same content</p>";
        let root = PathBuf::from(r"C:\mods\example");

        let first = cache.prepared_markdown("mod-1", html, Some(&root), Some(42));
        let second = cache.prepared_markdown("mod-1", html, Some(&root), Some(42));
        assert_eq!(first, second);
        assert_eq!(cache.prepared_builds, 1);

        cache.prepared_markdown("mod-1", "<p>changed content</p>", Some(&root), Some(42));
        assert_eq!(cache.prepared_builds, 2);

        let changed_root = PathBuf::from(r"C:\mods\moved");
        cache.prepared_markdown("mod-1", "<p>changed content</p>", Some(&changed_root), Some(42));
        assert_eq!(cache.prepared_builds, 3);
    }

    #[test]
    fn library_detail_raw_profile_json_is_parsed_once_until_content_changes() {
        let mut cache = LibraryDetailContentCache::default();
        let raw = r#"{"_sText":"<p>description</p>","_aPreviewMedia":{"_aImages":[{"_sCaption":"first"}]}}"#;

        assert_eq!(
            cache.raw_profile_description("mod-1", raw).as_deref(),
            Some("<p>description</p>")
        );
        assert_eq!(
            cache.raw_profile_preview_captions("mod-1", raw),
            vec![Some("first".to_string())]
        );
        assert_eq!(cache.raw_profile_parses, 1);

        cache.raw_profile_description("mod-1", r#"{"_sText":"<p>changed</p>"}"#);
        assert_eq!(cache.raw_profile_parses, 2);
    }

    #[test]
    fn library_detail_cached_prepare_keeps_dynamic_local_image_resolution() {
        let portable = dummy_portable_paths();
        let root = tempfile::tempdir().expect("temp mod root should be created");
        let url = "https://images.gamebanana.com/example/library-detail-dynamic-local-image.png";
        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
        let _ = persistence::cache_remove(&portable, &cache_key);
        let html = format!(r#"<p>body</p><img src="{url}">"#);
        let mut cache = LibraryDetailContentCache::default();
        let prepared = cache.prepared_markdown("mod-1", &html, Some(root.path()), Some(42));

        let before = rewrite_markdown_urls(&prepared, Some(root.path()), Some(42), &portable);
        let original_before = prepare_markdown_for_display(
            &html,
            Some(root.path()),
            Some(42),
            &portable,
        );
        assert_eq!(before, original_before);

        let meta_dir = root.path().join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).expect("metadata directory should be created");
        let local_path = meta_dir.join(format!("gb_desc_42_{}.png", hash64_hex(url.as_bytes())));
        fs::write(&local_path, b"persisted image").expect("local image should be written");
        let after_appears = rewrite_markdown_urls(&prepared, Some(root.path()), Some(42), &portable);
        let original_after_appears = prepare_markdown_for_display(
            &html,
            Some(root.path()),
            Some(42),
            &portable,
        );
        assert_eq!(after_appears, original_after_appears);
        assert!(after_appears.contains(&path_to_file_uri(&local_path)));

        fs::remove_file(&local_path).expect("local image should be removed");
        let after_disappears = rewrite_markdown_urls(&prepared, Some(root.path()), Some(42), &portable);
        let original_after_disappears = prepare_markdown_for_display(
            &html,
            Some(root.path()),
            Some(42),
            &portable,
        );
        assert_eq!(after_disappears, original_after_disappears);
        assert_eq!(cache.prepared_builds, 1);
    }

    #[test]
    #[ignore = "focused release-path evidence; run with --ignored --nocapture"]
    fn library_detail_cache_release_path_evidence() {
        use std::{hint::black_box, time::Instant};

        let portable = dummy_portable_paths();
        let html = format!(
            "<article><h2>Large profile</h2>{}</article>",
            "<p>Repeated profile prose with a link and an image marker.</p>".repeat(256)
        );
        let iterations = 24;
        let _ = prepare_markdown_for_display(&html, None, Some(42), &portable);
        let mut cache = LibraryDetailContentCache::default();
        let prepared = cache.prepared_markdown("mod-1", &html, None, Some(42));
        let _ = rewrite_markdown_urls(&prepared, None, Some(42), &portable);

        let baseline_output = prepare_markdown_for_display(&html, None, Some(42), &portable);
        let cached_output = rewrite_markdown_urls(
            &cache.prepared_markdown("mod-1", &html, None, Some(42)),
            None,
            Some(42),
            &portable,
        );
        assert_eq!(baseline_output, cached_output);

        let baseline_start = Instant::now();
        let mut baseline_bytes = 0usize;
        for _ in 0..iterations {
            baseline_bytes = baseline_bytes.wrapping_add(
                black_box(prepare_markdown_for_display(&html, None, Some(42), &portable)).len(),
            );
        }
        let baseline_elapsed = baseline_start.elapsed();

        let cached_start = Instant::now();
        let mut cached_bytes = 0usize;
        for _ in 0..iterations {
            let prepared = cache.prepared_markdown("mod-1", &html, None, Some(42));
            cached_bytes = cached_bytes.wrapping_add(
                black_box(rewrite_markdown_urls(&prepared, None, Some(42), &portable)).len(),
            );
        }
        let cached_elapsed = cached_start.elapsed();

        println!(
            "library detail prepare evidence: baseline={baseline_elapsed:?} cached={cached_elapsed:?} baseline_bytes={baseline_bytes} cached_bytes={cached_bytes} prepared_builds={}",
            cache.prepared_builds
        );
        assert_eq!(cache.prepared_builds, 1);
        assert_eq!(baseline_bytes, cached_bytes);
    }
}

#[cfg(test)]
mod markdown_link_tests {
    use super::*;

    #[test]
    fn prepare_keeps_link_targets_as_written() {
        let cases = [
            (
                r#"<p><a href="https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fdl%3Fid%3D1%26key%3Dabc&amp;sa=D">mirror</a></p>"#,
                "[mirror](https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fdl%3Fid%3D1%26key%3Dabc&sa=D)",
            ),
            (
                r#"<p><a href="https://example.com/search?tag=%23genshin&amp;page=2">hashtag</a></p>"#,
                "[hashtag](https://example.com/search?tag=%23genshin&page=2)",
            ),
            (
                r#"<p><a href="https://example.com/q?x=a+b&amp;y=100%25">percent</a></p>"#,
                "[percent](https://example.com/q?x=a+b&y=100%25)",
            ),
            (
                r#"<p><a href="https://example.com/a%29b">unbalanced</a> after</p>"#,
                "[unbalanced](https://example.com/a%29b) after",
            ),
            (
                r#"<p><a href="https://example.com/dl/%FF%FEfile.zip">legacy link</a></p>"#,
                "[legacy link](https://example.com/dl/%FF%FEfile.zip)",
            ),
            (
                r#"<p><a href="https://example.com/%3Cx%3E%20y">angle</a></p>"#,
                "[angle](https://example.com/%3Cx%3E%20y)",
            ),
            (
                r#"<p>Get <a href="https://example.com/files/my%20mod%20v2.zip">the file</a></p>"#,
                "Get [the file](https://example.com/files/my%20mod%20v2.zip)",
            ),
            (
                r#"<p><a href=' https://example.com/a%20b '>padded</a></p>"#,
                "[padded](https://example.com/a%20b)",
            ),
            (
                r#"<p><A HREF=https://example.com/a%20b>loud</A></p>"#,
                "[loud](https://example.com/a%20b)",
            ),
            (
                r#"<p><a href="https://example.com/my file.zip">spaced</a></p>"#,
                "[spaced](<https://example.com/my file.zip>)",
            ),
        ];
        for (html, expected) in cases {
            assert_eq!(prepare_markdown_content(html).trim(), expected, "input: {html}");
        }
    }

    #[test]
    fn prepare_passes_html_only_links_through_unchanged() {
        let markdown = prepare_markdown_content(
            r#"<p>x<a name="top" href="https://example.com/a%20b"></a> see<sup><a href="https://example.com/c%26d">1</a></sup></p>"#,
        );
        assert!(markdown.contains(r#"href="https://example.com/a%20b""#), "{markdown}");
        assert!(markdown.contains(r#"href="https://example.com/c%26d""#), "{markdown}");
    }
}
