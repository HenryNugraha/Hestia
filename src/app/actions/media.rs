impl HestiaApp {
    fn is_static_image_path(path: &Path) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .map(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "bmp"
                )
            })
            .unwrap_or(false)
    }

    fn is_jpeg_path(path: &Path) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
            .unwrap_or(false)
    }

    fn is_unlinked_mod_entry(mod_entry: &ModEntry) -> bool {
        mod_entry
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .is_none()
    }

    fn selected_unlinked_mod_context(&self) -> Option<(String, String)> {
        if self.current_view != ViewMode::Library || !self.mod_detail_open {
            return None;
        }
        let selected = self.selected_mod()?;
        if !Self::is_unlinked_mod_entry(selected) {
            return None;
        }
        Some((
            selected.id.clone(),
            selected
                .metadata
                .user
                .title
                .clone()
                .unwrap_or_else(|| selected.folder_name.clone()),
        ))
    }

    fn sync_mod_cover_to_first_screenshot(mod_entry: &mut ModEntry) {
        mod_entry.metadata.user.cover_image = mod_entry.metadata.user.screenshots.first().cloned();
    }

    fn my_mod_screenshot_texture_key(mod_id: &str, rel_path: &str) -> String {
        format!("my-mod-shot-{mod_id}-{}", hash64_hex(rel_path.as_bytes()))
    }

    fn clear_mod_card_texture(&mut self, mod_id: &str) {
        self.remove_tracked_texture(TextureKind::ModThumb, mod_id);
        self.remove_tracked_texture(TextureKind::ModFull, mod_id);
        self.pending_mod_image_requests.remove(mod_id);
        self.pending_mod_image_queue
            .retain(|req| req.texture_key != mod_id);
        self.pending_texture_uploads.retain(|item| match item {
            PendingTextureUpload::ModThumb { texture_key, .. }
            | PendingTextureUpload::ModFull { texture_key, .. } => texture_key != mod_id,
            _ => true,
        });
    }

    fn clear_mod_screenshot_texture(&mut self, mod_id: &str, rel_path: &str) {
        let texture_key = Self::my_mod_screenshot_texture_key(mod_id, rel_path);
        self.remove_tracked_texture(TextureKind::ModThumb, &texture_key);
        self.remove_tracked_texture(TextureKind::ModFull, &texture_key);
        self.pending_mod_image_requests.remove(&texture_key);
        self.pending_mod_image_queue
            .retain(|req| req.texture_key != texture_key);
        self.pending_texture_uploads.retain(|item| match item {
            PendingTextureUpload::ModThumb { texture_key: key, .. }
            | PendingTextureUpload::ModFull { texture_key: key, .. } => key != &texture_key,
            _ => true,
        });
        self.my_mod_overlay_images
            .retain(|item| item.texture_key != texture_key);
        if self
            .browse_state
            .screenshot_overlay
            .as_ref()
            .is_some_and(|overlay| overlay.texture_key == texture_key)
        {
            self.browse_state.screenshot_overlay = None;
        }
    }

    fn encode_dynamic_image_as_jpeg(image: image::DynamicImage, quality: u8) -> Result<Vec<u8>> {
        let rgb = image.to_rgb8();
        let mut out = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        encoder.encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )?;
        Ok(out)
    }

    fn save_manual_mod_image_from_path(mod_root: &Path, source_path: &Path) -> Result<String> {
        if !Self::is_static_image_path(source_path) {
            bail!("unsupported image file: {}", source_path.display());
        }

        let bytes = fs::read(source_path)
            .map_err(|err| anyhow!("failed to read image {}: {err}", source_path.display()))?;
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes.as_slice()))
            .with_guessed_format()
            .map_err(|err| anyhow!("failed to detect image format: {err}"))?;
        let is_jpeg = matches!(reader.format(), Some(image::ImageFormat::Jpeg));
        let decoded = reader
            .decode()
            .map_err(|err| anyhow!("failed to decode image {}: {err}", source_path.display()))?;

        let encoded = if Self::is_jpeg_path(source_path) && is_jpeg {
            bytes
        } else {
            Self::encode_dynamic_image_as_jpeg(decoded, 90)?
        };

        Self::save_manual_mod_image_bytes(mod_root, &encoded)
    }

    fn save_manual_mod_image_bytes(mod_root: &Path, encoded_jpeg: &[u8]) -> Result<String> {
        let meta_dir = mod_root.join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir)?;
        for _ in 0..8 {
            let file_name = format!("manual_{}.jpg", Uuid::new_v4().simple());
            let abs_path = meta_dir.join(&file_name);
            if abs_path.exists() {
                continue;
            }
            persistence::write_atomic_bytes(&abs_path, encoded_jpeg)?;
            return Ok(format!("{MOD_META_DIR}\\{file_name}"));
        }
        bail!("failed to allocate a unique manual image name")
    }

    fn import_manual_images_from_paths(mod_root: &Path, paths: Vec<PathBuf>) -> Result<Vec<String>> {
        let mut imported = Vec::new();
        for path in paths {
            match Self::save_manual_mod_image_from_path(mod_root, &path) {
                Ok(rel) => imported.push(rel),
                Err(err) => {
                    for rel in &imported {
                        let abs_path = mod_root.join(rel);
                        if abs_path.exists() {
                            let _ = fs::remove_file(abs_path);
                        }
                    }
                    return Err(err);
                }
            }
        }
        Ok(imported)
    }

    fn enqueue_add_images_to_unlinked_mod(&mut self, mod_id: &str, paths: Vec<PathBuf>) -> Result<()> {
        let (root_path, folder_name) = {
            let mod_entry = self
                .state
                .mods
                .iter()
                .find(|item| item.id == mod_id)
                .ok_or_else(|| anyhow!("mod not found"))?;
            if !Self::is_unlinked_mod_entry(mod_entry) {
                bail!("manual images are only supported for unlinked mods");
            }
            (mod_entry.root_path.clone(), mod_entry.folder_name.clone())
        };

        if paths.is_empty() {
            return Ok(());
        }

        let mod_id = mod_id.to_string();
        let tx = self.manual_image_event_tx.clone();
        let handle = self.runtime_services.handle();
        self.manual_image_imports_pending = self.manual_image_imports_pending.saturating_add(1);
        self.runtime_services.spawn(async move {
            let folder_name_for_error = folder_name.clone();
            let result = handle
                .spawn_blocking(move || Self::import_manual_images_from_paths(&root_path, paths))
                .await;
            let event = match result {
                Ok(Ok(rel_paths)) => ManualImageEvent::Added {
                    mod_id,
                    folder_name,
                    rel_paths,
                },
                Ok(Err(err)) => ManualImageEvent::Failed {
                    folder_name: folder_name_for_error,
                    error: format!("{err:#}"),
                },
                Err(err) => ManualImageEvent::Failed {
                    folder_name: folder_name_for_error,
                    error: format!("image import worker failed: {err}"),
                },
            };
            let _ = tx.send(event);
        });
        Ok(())
    }

    fn enqueue_clipboard_image_to_selected_unlinked_mod(&mut self) -> Result<()> {
        let (mod_id, _) = self
            .selected_unlinked_mod_context()
            .ok_or_else(|| anyhow!("open an unlinked mod detail first"))?;

        let (root_path, folder_name) = {
            let mod_entry = self
                .state
                .mods
                .iter()
                .find(|item| item.id == mod_id)
                .ok_or_else(|| anyhow!("mod not found"))?;
            (mod_entry.root_path.clone(), mod_entry.folder_name.clone())
        };
        let tx = self.manual_image_event_tx.clone();
        let handle = self.runtime_services.handle();
        self.manual_image_imports_pending = self.manual_image_imports_pending.saturating_add(1);
        self.runtime_services.spawn(async move {
            let folder_name_for_error = folder_name.clone();
            let result = handle
                .spawn_blocking(move || {
                    let mut clipboard = arboard::Clipboard::new()
                        .map_err(|err| anyhow!("failed to open clipboard: {err}"))?;
                    let image = clipboard
                        .get_image()
                        .map_err(|err| anyhow!("clipboard does not contain an image: {err}"))?;
                    let width = u32::try_from(image.width)
                        .map_err(|err| anyhow!("clipboard image width is too large: {err}"))?;
                    let height = u32::try_from(image.height)
                        .map_err(|err| anyhow!("clipboard image height is too large: {err}"))?;
                    let rgba = image.bytes.into_owned();
                    let rgba = image::RgbaImage::from_raw(width, height, rgba)
                        .ok_or_else(|| anyhow!("clipboard image data is invalid"))?;
                    let encoded = Self::encode_dynamic_image_as_jpeg(
                        image::DynamicImage::ImageRgba8(rgba),
                        90,
                    )?;
                    Self::save_manual_mod_image_bytes(&root_path, &encoded).map(|rel| vec![rel])
                })
                .await;
            let event = match result {
                Ok(Ok(rel_paths)) => ManualImageEvent::Added {
                    mod_id,
                    folder_name,
                    rel_paths,
                },
                Ok(Err(err)) => ManualImageEvent::Failed {
                    folder_name: folder_name_for_error,
                    error: format!("{err:#}"),
                },
                Err(err) => ManualImageEvent::Failed {
                    folder_name: folder_name_for_error,
                    error: format!("clipboard image worker failed: {err}"),
                },
            };
            let _ = tx.send(event);
        });
        Ok(())
    }

    fn consume_manual_image_events(&mut self) {
        while let Ok(event) = self.manual_image_event_rx.try_recv() {
            self.manual_image_imports_pending =
                self.manual_image_imports_pending.saturating_sub(1);
            match event {
                ManualImageEvent::Added {
                    mod_id,
                    folder_name,
                    rel_paths,
                } => {
                    if rel_paths.is_empty() {
                        continue;
                    }
                    let count = rel_paths.len();
                    let cover_changed = {
                        let Some(mod_entry) = self.state.mods.iter_mut().find(|item| item.id == mod_id) else {
                            self.report_warn(
                                format!(
                                    "manual images imported for missing mod {mod_id}: {}",
                                    rel_paths.join(", ")
                                ),
                                Some("Could not attach images"),
                            );
                            continue;
                        };
                        let old_cover = mod_entry.metadata.user.cover_image.clone();
                        mod_entry.metadata.user.screenshots.extend(rel_paths);
                        Self::sync_mod_cover_to_first_screenshot(mod_entry);
                        let cover_changed = old_cover != mod_entry.metadata.user.cover_image;
                        if let Err(err) = xxmi::save_mod_metadata(mod_entry) {
                            self.report_error(err, Some("Could not save images"));
                            continue;
                        }
                        cover_changed
                    };
                    if cover_changed {
                        self.clear_mod_card_texture(&mod_id);
                    }
                    self.save_state();
                    self.log_action("Images Added", &folder_name);
                    self.set_message_ok(format!("Added {count} image(s)"));
                }
                ManualImageEvent::Failed { folder_name, error } => {
                    self.report_error_message(
                        format!("manual image import failed for {folder_name}: {error}"),
                        Some("Could not add images"),
                    );
                }
            }
        }
    }

    fn delete_unlinked_mod_image(&mut self, mod_id: &str, rel_path: &str) -> Result<()> {
        let (abs_path, cover_changed) = {
            let mod_entry = self
                .state
                .mods
                .iter_mut()
                .find(|item| item.id == mod_id)
                .ok_or_else(|| anyhow!("mod not found"))?;
            if !Self::is_unlinked_mod_entry(mod_entry) {
                bail!("manual images are only supported for unlinked mods");
            }
            let before = mod_entry.metadata.user.screenshots.len();
            mod_entry
                .metadata
                .user
                .screenshots
                .retain(|item| item != rel_path);
            if mod_entry.metadata.user.screenshots.len() == before {
                bail!("image is not listed on this mod");
            }

            let old_cover = mod_entry.metadata.user.cover_image.clone();
            let abs_path = mod_entry.root_path.join(rel_path);
            Self::sync_mod_cover_to_first_screenshot(mod_entry);
            let cover_changed = old_cover != mod_entry.metadata.user.cover_image;
            xxmi::save_mod_metadata(mod_entry)?;
            (abs_path, cover_changed)
        };

        if abs_path
            .components()
            .any(|part| part.as_os_str() == std::ffi::OsStr::new(MOD_META_DIR))
            && abs_path.exists()
        {
            let _ = fs::remove_file(&abs_path);
        }

        self.clear_mod_screenshot_texture(mod_id, rel_path);
        if cover_changed {
            self.clear_mod_card_texture(mod_id);
        }
        self.save_state();
        Ok(())
    }

    fn enqueue_cover_preload(&mut self) {
        let Some(selected_game_id) = self
            .state
            .games
            .get(self.selected_game)
            .map(|game| game.definition.id.clone())
        else {
            return;
        };
        for game in &self.state.games {
            let game_id = &game.definition.id;
            if game_id == &selected_game_id {
                continue;
            }
            if self.game_cover_textures.contains_key(game_id) {
                continue;
            }
            if self.pending_cover_requests.contains(game_id) {
                continue;
            }
            if self
                .cover_request_tx
                .send(CoverRequest {
                    game_id: game_id.clone(),
                })
                .is_ok()
            {
                self.pending_cover_requests.insert(game_id.clone());
            }
        }
    }

    fn enqueue_icon_preload(&mut self) {
        let Some(selected_game_id) = self
            .state
            .games
            .get(self.selected_game)
            .map(|game| game.definition.id.clone())
        else {
            return;
        };
        for game in &self.state.games {
            let game_id = &game.definition.id;
            if game_id == &selected_game_id {
                continue;
            }
            if self.game_icon_textures.contains_key(game_id) {
                continue;
            }
            if self.pending_icon_requests.contains(game_id) {
                continue;
            }
            if self
                .icon_request_tx
                .send(IconRequest {
                    game_id: game_id.clone(),
                })
                .is_ok()
            {
                self.pending_icon_requests.insert(game_id.clone());
            }
        }
    }

    fn request_icon_texture(&mut self, game_id: &str) {
        if self.game_icon_textures.contains_key(game_id) {
            return;
        }
        if self.pending_icon_requests.contains(game_id) {
            return;
        }
        if self
            .icon_request_tx
            .send(IconRequest {
                game_id: game_id.to_string(),
            })
            .is_ok()
        {
            self.pending_icon_requests.insert(game_id.to_string());
        }
    }

    fn consume_cover_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.cover_result_rx.try_recv() {
            self.pending_cover_requests.remove(&result.game_id);
            let texture = ctx.load_texture(
                format!("game-cover-{}", result.game_id),
                result.image,
                egui::TextureOptions::LINEAR,
            );
            self.game_cover_textures
                .insert(result.game_id, texture);
        }
    }

    fn consume_icon_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.icon_result_rx.try_recv() {
            self.pending_icon_requests.remove(&result.game_id);
            let texture = ctx.load_texture(
                format!("game-icon-{}", result.game_id),
                result.image,
                egui::TextureOptions::LINEAR,
            );
            self.game_icon_textures.insert(result.game_id, texture);
        }
    }

    fn resolve_mod_thumb_path(mod_root: &Path, profile: ThumbnailProfile) -> PathBuf {
        let file_name = match profile {
            ThumbnailProfile::Card => "card_thumb.png",
            ThumbnailProfile::Rail => "rail_thumb.png",
        };
        mod_root.join(MOD_META_DIR).join(file_name)
    }

    fn current_card_thumb_meta(mod_entry: &ModEntry) -> (CardThumbMeta, Option<PathBuf>, Option<String>) {
        if let Some(rel) = mod_entry
            .metadata
            .user
            .cover_image
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            let abs = mod_entry.root_path.join(rel);
            let (mtime, size) = fs::metadata(&abs)
                .ok()
                .map(|m| {
                    let mt = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64);
                    (mt, Some(m.len()))
                })
                .unwrap_or((None, None));
            return (
                CardThumbMeta {
                    kind: "cover_file".to_string(),
                    id: rel.to_string(),
                    mtime,
                    size,
                },
                Some(abs),
                None,
            );
        }
        if let Some(url) = mod_entry
            .source
            .as_ref()
            .and_then(|s| s.snapshot.as_ref())
            .and_then(|s| s.preview_urls.first())
            .cloned()
        {
            return (
                CardThumbMeta {
                    kind: "gb_url".to_string(),
                    id: url.clone(),
                    mtime: None,
                    size: None,
                },
                None,
                Some(url),
            );
        }
        (
            CardThumbMeta {
                kind: "none".to_string(),
                id: String::new(),
                mtime: None,
                size: None,
            },
            None,
            None,
        )
    }

    fn is_mod_thumb_valid(mod_entry: &ModEntry, expected: &CardThumbMeta, profile: ThumbnailProfile) -> bool {
        let user = &mod_entry.metadata.user;
        let (source_kind, source_id, source_mtime, source_size) = match profile {
            ThumbnailProfile::Card => (
                user.card_thumb_source_kind.as_deref(),
                user.card_thumb_source_id.as_deref(),
                user.card_thumb_source_mtime,
                user.card_thumb_source_size,
            ),
            ThumbnailProfile::Rail => (
                user.rail_thumb_source_kind.as_deref(),
                user.rail_thumb_source_id.as_deref(),
                user.rail_thumb_source_mtime,
                user.rail_thumb_source_size,
            ),
        };
        let matches_meta = source_kind == Some(expected.kind.as_str())
            && source_id == Some(expected.id.as_str())
            && source_mtime == expected.mtime
            && source_size == expected.size;
        if !matches_meta {
            return false;
        }
        Self::resolve_mod_thumb_path(&mod_entry.root_path, profile).exists()
    }

    fn update_mod_thumb_meta(mod_entry: &mut ModEntry, expected: &CardThumbMeta, profile: ThumbnailProfile) {
        match profile {
            ThumbnailProfile::Card => {
                mod_entry.metadata.user.card_thumb_source_kind = Some(expected.kind.clone());
                mod_entry.metadata.user.card_thumb_source_id = Some(expected.id.clone());
                mod_entry.metadata.user.card_thumb_source_mtime = expected.mtime;
                mod_entry.metadata.user.card_thumb_source_size = expected.size;
                mod_entry.metadata.user.card_thumb_generated_at = Some(Utc::now());
            }
            ThumbnailProfile::Rail => {
                mod_entry.metadata.user.rail_thumb_source_kind = Some(expected.kind.clone());
                mod_entry.metadata.user.rail_thumb_source_id = Some(expected.id.clone());
                mod_entry.metadata.user.rail_thumb_source_mtime = expected.mtime;
                mod_entry.metadata.user.rail_thumb_source_size = expected.size;
                mod_entry.metadata.user.rail_thumb_generated_at = Some(Utc::now());
            }
        }
        let _ = xxmi::save_mod_metadata(mod_entry);
    }

    fn enqueue_local_mod_image_request(&mut self, req: LocalModImageRequest) {
        if let Some(existing) = self
            .pending_mod_image_queue
            .iter_mut()
            .find(|q| q.texture_key == req.texture_key)
        {
            existing.priority = existing.priority.min(req.priority);
            if matches!(existing.mode, LocalModImageMode::CardThumbOnly | LocalModImageMode::ThumbOnly)
                && req.mode == LocalModImageMode::FullOnly
            {
                existing.mode = LocalModImageMode::FullOnly;
                existing.payload = req.payload;
            }
            return;
        }
        self.pending_mod_image_queue.push(req);
    }

    fn queue_mod_card_thumb_load_with_priority(&mut self, mod_id: &str, priority: u32) {
        if self.mod_cover_textures.contains_key(mod_id) {
            return;
        }
        let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id).cloned() else {
            return;
        };
        let (expected_meta, source_path, source_url) = Self::current_card_thumb_meta(&mod_entry);
        let thumb_path = Self::resolve_mod_thumb_path(&mod_entry.root_path, ThumbnailProfile::Card);
        let force_regen = !Self::is_mod_thumb_valid(&mod_entry, &expected_meta, ThumbnailProfile::Card);
        let payload = LocalModImagePayload::CardThumb {
            mod_root: mod_entry.root_path.clone(),
            source_path,
            source_url,
            expected_meta,
            force_regen,
        };
        if self.pending_mod_image_requests.contains(mod_id)
            && !self
                .pending_mod_image_queue
                .iter()
                .any(|q| q.texture_key == mod_id)
        {
            return;
        }
        self.pending_mod_image_requests.insert(mod_id.to_string());
        self.enqueue_local_mod_image_request(LocalModImageRequest {
            texture_key: mod_id.to_string(),
            mode: LocalModImageMode::CardThumbOnly,
            priority,
            generation: 0,
            payload,
        });
        if !force_regen && thumb_path.exists() {
            return;
        }
    }

    fn queue_mod_image_thumb_load(
        &mut self,
        texture_key: String,
        path: PathBuf,
        priority: u32,
        thumb_profile: ThumbnailProfile,
    ) {
        if self.mod_cover_textures.contains_key(&texture_key) {
            return;
        }
        if self.pending_mod_image_requests.contains(&texture_key) {
            // allow mode upgrade via enqueue method
        } else {
            self.pending_mod_image_requests.insert(texture_key.clone());
        }
        self.enqueue_local_mod_image_request(LocalModImageRequest {
            texture_key,
            mode: LocalModImageMode::ThumbOnly,
            priority,
            generation: 0,
            payload: LocalModImagePayload::Path { path, thumb_profile },
        });
    }

    fn queue_mod_image_full_load(&mut self, texture_key: String, path: PathBuf, priority: u32) {
        if self.mod_full_textures.contains_key(&texture_key) {
            return;
        }
        if self.pending_mod_image_requests.contains(&texture_key) {
            // full will be retried on next frame if currently occupied by thumb request
        } else {
            self.pending_mod_image_requests.insert(texture_key.clone());
        }
        self.enqueue_local_mod_image_request(LocalModImageRequest {
            texture_key,
            mode: LocalModImageMode::FullOnly,
            priority,
            generation: 0,
            payload: LocalModImagePayload::Path {
                path,
                thumb_profile: ThumbnailProfile::Rail,
            },
        });
    }

    fn prewarm_markdown_images(&mut self, _markdown: &str) {
        // No need to prewarm for texture keys - they're already loaded or loading
    }

    fn render_youtube_card(&mut self, ui: &mut Ui, url: &str) {
        let url = url.to_string();
        let button_response = egui::Frame::new()
            .fill(Color32::from_rgb(40, 42, 46))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(80, 82, 86)))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(texture) = &self.youtube_icon_texture {
                        ui.add(egui::Image::new(texture).max_height(20.0));
                    }
                    ui.label(RichText::new("Watch Preview").size(14.0));
                });
            });

        if button_response
            .response
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            if let Err(err) = open_external_url(&url) {
                self.report_error(err, Some("Could not open browser"));
            }
        }
    }

    fn render_markdown_with_inline_images(&mut self, ui: &mut Ui, markdown: &str) {
        let images = extract_markdown_images(markdown);
        let youtube_embeds = extract_markdown_youtube_embeds(markdown);

        if images.is_empty() && youtube_embeds.is_empty() {
            CommonMarkViewer::new().show(ui, &mut self.browse_commonmark_cache, markdown);
            return;
        }

        let mut embeds: Vec<(usize, usize, InlineMarkdownEmbed)> = images
            .into_iter()
            .map(|(start, end, texture_key)| {
                (start, end, InlineMarkdownEmbed::Image { texture_key })
            })
            .collect();
        embeds.extend(youtube_embeds.into_iter().map(|(start, end, url)| {
            (start, end, InlineMarkdownEmbed::Youtube { url })
        }));
        embeds.sort_by_key(|(start, _, _)| *start);

        let mut last_end = 0;
        
        for (start, end, embed) in embeds {
            if start > last_end {
                let text_chunk = &markdown[last_end..start];
                if !text_chunk.trim().is_empty() {
                    CommonMarkViewer::new().show(ui, &mut self.browse_commonmark_cache, text_chunk);
                }
            }
            
            match embed {
                InlineMarkdownEmbed::Image { texture_key } => {
                    if let Some(texture) = self.browse_image_textures.get(&texture_key) {
                        ui.add_space(8.0);
                        render_inline_markdown_image(ui, texture);
                        ui.add_space(8.0);
                    }
                }
                InlineMarkdownEmbed::Youtube { url } => {
                    ui.add_space(8.0);
                    self.render_youtube_card(ui, &url);
                    ui.add_space(8.0);
                }
            }
            
            last_end = end;
        }
        
        if last_end < markdown.len() {
            let text_chunk = &markdown[last_end..];
            if !text_chunk.trim().is_empty() {
                CommonMarkViewer::new().show(ui, &mut self.browse_commonmark_cache, text_chunk);
            }
        }
    }

    fn process_local_mod_image_queue(&mut self) {
        if self.pending_mod_image_queue.is_empty() {
            return;
        }

        // CONTEXTUAL THROTTLING: Suspend background work to prioritize current user focus
        let mut allowed_mod_id = String::new();
        let mut focus_mode = false;

        if let Some(mod_id) = self.selected_mod_id.as_ref() {
            if self.mod_detail_open || self.browse_state.screenshot_overlay.is_some() {
                // Only allow if the overlay/detail actually belongs to this local mod
                let belongs = self.browse_state.screenshot_overlay.as_ref().map_or(true, |o| {
                    o.texture_key == *mod_id || o.texture_key.starts_with(&format!("my-mod-shot-{mod_id}-"))
                });
                if belongs {
                    focus_mode = true;
                    allowed_mod_id = mod_id.clone();
                }
            }
        }

        // If a browse overlay is active, suspend all local mod background work to free resources
        if !focus_mode && self.browse_state.screenshot_overlay.is_some() {
            focus_mode = true;
            allowed_mod_id = "__NONE__".to_string(); // Effectively disallow all local mod images
        }

        self.pending_mod_image_queue.sort_by_key(|req| {
            let class = match req.mode {
                LocalModImageMode::CardThumbOnly | LocalModImageMode::ThumbOnly => 0_u8,
                LocalModImageMode::FullOnly => 1,
            };
            (class, req.priority)
        });

        let mut eligible = Vec::new();
        let current_gen = self.image_generation.load(Ordering::Relaxed);
        let mut i = 0;
        while i < self.pending_mod_image_queue.len() && eligible.len() < LOCAL_IMAGE_DISPATCH_BATCH {
            let req = &self.pending_mod_image_queue[i];
            let is_mod_task = req.texture_key == allowed_mod_id 
                || req.texture_key.starts_with(&format!("my-mod-shot-{}-", allowed_mod_id))
                || req.texture_key.starts_with("file:///")
                || req.texture_key.starts_with("http");
            let is_background_thumb = req.mode == LocalModImageMode::CardThumbOnly;
            let is_overlay_task = self.browse_state.screenshot_overlay.as_ref().is_some_and(|o| o.texture_key == req.texture_key);
            if !focus_mode || is_mod_task || is_background_thumb || is_overlay_task {
                let mut req = self.pending_mod_image_queue.remove(i);
                req.generation = current_gen;
                eligible.push(req);
            } else {
                i += 1;
            }
        }

        for req in eligible {
            if self.mod_image_request_tx.send(req).is_err() {
                break;
            }
        }
    }

    fn consume_mod_image_results(&mut self) {
        while let Ok(result) = self.mod_image_result_rx.try_recv() {
            if result.done {
                self.pending_mod_image_requests.remove(&result.texture_key);
            }
            if result.thumb_generated {
                if let Some(meta) = result.thumb_meta.as_ref() {
                    if let Some(mod_entry) = self
                        .state
                        .mods
                        .iter_mut()
                        .find(|m| m.id == result.texture_key)
                    {
                        Self::update_mod_thumb_meta(mod_entry, meta, ThumbnailProfile::Card);
                    }
                }
            }
            if let Some(image_thumb) = result.image_thumb {
                self.pending_texture_uploads.push_back(PendingTextureUpload::ModThumb {
                    texture_key: result.texture_key.clone(),
                    image: image_thumb,
                });
            }
            if let Some(image_full) = result.image_full {
                self.pending_texture_uploads.push_back(PendingTextureUpload::ModFull {
                    texture_key: result.texture_key,
                    image: image_full,
                });
            }
        }
    }

    fn consume_gif_preview_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.gif_preview_event_rx.try_recv() {
            match event {
                GifPreviewEvent::Ready { out_png, gif_dest, image } => {
                    let out_key = out_png.to_string_lossy().to_string();
                    self.pending_gif_previews.remove(&out_key);

                    // Register as a texture handle so egui can find it by key immediately.
                    let texture_key = format!("gif-preview-{}", hash64_hex(gif_dest.as_bytes()));
                    let texture = ctx.load_texture(
                        &texture_key,
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::BrowseFull, texture_key.clone(), 3, texture);

                    // Queue full animation decode for this GIF (if not already queued)
                    if !self.pending_gif_animations.contains(&texture_key) {
                        self.pending_gif_animations.insert(texture_key.clone());
                        if let Some(path) = file_uri_to_path(&gif_dest) {
                            let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromFile {
                                src_path: path,
                                texture_key: texture_key.clone(),
                            });
                        } else if gif_dest.starts_with("http://") || gif_dest.starts_with("https://") {
                            let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromUrl {
                                url: gif_dest.clone(),
                                texture_key: texture_key.clone(),
                            });
                        }
                    }

                    self.browse_commonmark_cache = CommonMarkCache::default();
                }
                GifPreviewEvent::Failed { out_png } => {
                    let out_key = out_png.to_string_lossy().to_string();
                    self.pending_gif_previews.remove(&out_key);
                    // self.log_action("GIF Preview Failed", &format!("{}: {}", out_png.display(), error));
                    // self.push_toast_err(format!("Failed to generate GIF preview for {}: {}", out_png.display(), error));
                }
            }
            ctx.request_repaint();
        }
    }

    fn consume_gif_animation_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.gif_animation_event_rx.try_recv() {
            match event {
                GifAnimationEvent::Ready { texture_key, animation } => {
                    self.pending_gif_animations.remove(&texture_key);
                    
                    // Get current time for animation timing
                    let now = ctx.input(|i| i.time);
                    
                    // Load the first frame as an immediate texture
                    if let Some(first_frame) = animation.frames.first() {
                        let texture = ctx.load_texture(
                            &texture_key,
                            first_frame.image.clone(),
                            egui::TextureOptions::LINEAR,
                        );
                        self.insert_tracked_texture(TextureKind::BrowseFull, texture_key.clone(), 3, texture);
                    }

                    // Store animation state
                    let state = AnimatedGifState {
                        animation,
                        current_frame: 0,
                        frame_start_time: now,
                    };
                    self.animated_gif_state.insert(texture_key.clone(), state);

                    self.browse_commonmark_cache = CommonMarkCache::default();
                    ctx.request_repaint();
                }
                GifAnimationEvent::Failed { texture_key, error } => {
                    self.pending_gif_animations.remove(&texture_key);
                    self.report_warn(
                        format!("failed to decode GIF animation for {texture_key}: {error}"),
                        None,
                    );
                    ctx.request_repaint();
                }
            }
        }
    }

    fn update_gif_animations(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let mut texture_updates = Vec::new();

        for (texture_key, state) in self.animated_gif_state.iter_mut() {
            let elapsed_ms = ((now - state.frame_start_time) * 1000.0) as u32;
            
            // Calculate total animation duration
            let total_duration_ms: u32 = state.animation.frames.iter().map(|f| f.delay_ms).sum();
            if total_duration_ms == 0 {
                continue; // Skip animations with no valid timing
            }
            
            // Normalize elapsed time to animation loop
            let loop_elapsed = elapsed_ms % total_duration_ms;
            
            // Find which frame we should be on by accumulating delays
            let mut time_accum = 0u32;
            let mut new_frame = 0;
            for (i, frame) in state.animation.frames.iter().enumerate() {
                time_accum += frame.delay_ms;
                if loop_elapsed < time_accum {
                    new_frame = i;
                    break;
                }
            }

            // Only queue texture update if frame actually changed
            if new_frame != state.current_frame {
                state.current_frame = new_frame;
                texture_updates.push((texture_key.clone(), new_frame));
            }
        }

        // Apply texture updates (separate loop to avoid borrow conflicts)
        for (texture_key, frame_index) in texture_updates {
            if let Some(state) = self.animated_gif_state.get(&texture_key) {
                if let Some(frame) = state.animation.frames.get(frame_index) {
                    let texture = ctx.load_texture(
                        &texture_key,
                        frame.image.clone(),
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::BrowseFull, texture_key, 3, texture);
                    ctx.request_repaint();
                }
            }
        }

        // Request continuous repaints while animations are active
        if !self.animated_gif_state.is_empty() {
            ctx.request_repaint();
        }
    }

    fn queue_gif_previews_for_markdown(&mut self, ctx: &egui::Context, markdown: &str, mod_root: Option<&Path>) {
        let dests = extract_markdown_image_dests(markdown);
        for dest in dests {
            if !is_gif_dest(&dest) {
                continue;
            }
            let out_png = gif_preview_out_path(&dest, mod_root);
            let texture_key = format!("gif-preview-{}", hash64_hex(dest.as_bytes()));

            if out_png.exists() {
                // If preview exists on disk but isn't in memory as a texture, load it immediately.
                if !self.browse_image_textures.contains_key(&texture_key) {
                    if let Ok(bytes) = std::fs::read(&out_png) {
                        if let Some(image) = load_cover_color_image(&bytes) {

                            let texture = ctx.load_texture(
                                &texture_key,
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            self.insert_tracked_texture(TextureKind::BrowseFull, texture_key.clone(), 3, texture);
                            self.browse_commonmark_cache = CommonMarkCache::default();

                            // Queue full animation decode for this GIF (if not already queued)
                            if !self.pending_gif_animations.contains(&texture_key) {
                                self.pending_gif_animations.insert(texture_key.clone());
                                if let Some(path) = file_uri_to_path(&dest) {
                                    let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromFile {
                                        src_path: path,
                                        texture_key: texture_key.clone(),
                                    });
                                } else if dest.starts_with("http://") || dest.starts_with("https://") {
                                    let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromUrl {
                                        url: dest.clone(),
                                        texture_key: texture_key.clone(),
                                    });
                                }
                            }
                        }
                    }
                } else {
                    // Still queue animation decode if not already animating or queued
                    if !self.animated_gif_state.contains_key(&texture_key) && !self.pending_gif_animations.contains(&texture_key) {
                        self.pending_gif_animations.insert(texture_key.clone());
                        if let Some(path) = file_uri_to_path(&dest) {
                            let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromFile {
                                src_path: path,
                                texture_key: texture_key.clone(),
                            });
                        } else if dest.starts_with("http://") || dest.starts_with("https://") {
                            let _ = self.gif_animation_request_tx.send(GifAnimationRequest::FromUrl {
                                url: dest.clone(),
                                texture_key: texture_key.clone(),
                            });
                        }
                    }
                }
                continue;
            }
            let out_key = out_png.to_string_lossy().to_string();
            if self.pending_gif_previews.contains(&out_key) {
                continue;
            }

            self.pending_gif_previews.insert(out_key);

            if let Some(path) = file_uri_to_path(&dest) {
                let _ = self.gif_preview_request_tx.send(GifPreviewRequest::FromFile {
                    src_path: path,
                    out_png,
                    gif_dest: dest,
                });
            } else if dest.starts_with("http://") || dest.starts_with("https://") {
                let _ = self.gif_preview_request_tx.send(GifPreviewRequest::FromUrl {
                    url: dest.clone(),
                    out_png,
                    gif_dest: dest,
                });
            }
        }
    }

    fn process_pending_texture_uploads(&mut self, ctx: &egui::Context) {
        if self.pending_texture_uploads.len() > 1 {
            let mut uploads: Vec<_> = self.pending_texture_uploads.drain(..).collect();
            uploads.sort_by_key(PendingTextureUpload::priority_class);
            self.pending_texture_uploads = uploads.into();
        }
        let thumb_count = self
            .pending_texture_uploads
            .iter()
            .filter(|item| item.is_thumb())
            .count();
        let full_count = self.pending_texture_uploads.len().saturating_sub(thumb_count);
        let budget = if full_count == 0 {
            thumb_count.clamp(TEXTURE_UPLOADS_PER_FRAME, 192)
        } else {
            (TEXTURE_UPLOADS_PER_FRAME + thumb_count.min(48)).min(96)
        };
        for _ in 0..budget {
            let Some(item) = self.pending_texture_uploads.pop_front() else {
                break;
            };
            match item {
                PendingTextureUpload::ModThumb { texture_key, image } => {
                    let texture = ctx.load_texture(
                        format!("mod-thumb-{}", texture_key),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::ModThumb, texture_key, 2, texture);
                }
                PendingTextureUpload::ModFull { texture_key, image } => {
                    let texture = ctx.load_texture(
                        texture_key.clone(),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::ModFull, texture_key, 3, texture);
                }
                PendingTextureUpload::BrowseThumb { texture_key, image } => {
                    let texture = ctx.load_texture(
                        format!("browse-image-thumb-{}", texture_key),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::BrowseThumb, texture_key, 2, texture);
                }
                PendingTextureUpload::BrowseFull { texture_key, image } => {
                    let texture = ctx.load_texture(
                        texture_key.clone(),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::BrowseFull, texture_key, 3, texture);
                }
            }
        }
        self.evict_textures_to_budget(ctx.input(|i| i.time));
    }

}

fn render_inline_markdown_image(ui: &mut Ui, texture: &egui::TextureHandle) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }

    let max_width = ui.available_width().max(1.0);
    let scale = (max_width / size.x).min(1.0);
    ui.add(egui::Image::new(texture).fit_to_exact_size(size * scale));
}
