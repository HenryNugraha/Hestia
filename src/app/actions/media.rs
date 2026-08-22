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
        self.pending_image_loads.remove(mod_id);
        self.inflight_full_image_requests.remove(mod_id);
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
        self.pending_image_loads.remove(&texture_key);
        self.inflight_full_image_requests.remove(&texture_key);
        self.pending_mod_image_queue
            .retain(|req| req.texture_key != texture_key);
        self.pending_texture_uploads.retain(|item| match item {
            PendingTextureUpload::ModThumb {
                texture_key: key, ..
            }
            | PendingTextureUpload::ModFull {
                texture_key: key, ..
            } => key != &texture_key,
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
        let mut reader = reader;
        reader.limits(image_decode_limits());
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

    fn import_manual_images_from_paths(
        mod_root: &Path,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<String>> {
        let mut imported = Vec::with_capacity(paths.len());
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

    /// Best-effort sibling of `import_manual_images_from_paths` for images a
    /// mod shipped with: one unreadable file skips that file instead of
    /// failing and rolling back the whole adoption.
    fn import_bundled_images_best_effort(mod_root: &Path, paths: Vec<PathBuf>) -> Vec<String> {
        let mut imported = Vec::new();
        for path in paths {
            match Self::save_manual_mod_image_from_path(mod_root, &path) {
                Ok(rel) => imported.push(rel),
                Err(err) => {
                    tracing::warn!("skipping bundled preview image {}: {err:#}", path.display());
                }
            }
        }
        imported
    }

    /// After an external (non-Browse) install, adopt any preview image the mod
    /// shipped with (preview.jpg, .JASM_Cover.jpg, a loose 1.png next to the
    /// ini) as if the user had supplied it manually. Linked mods keep their
    /// GameBanana images; a mod that already shows a screenshot is left alone.
    fn enqueue_adopt_bundled_preview_images(&mut self, mod_id: &str) {
        let Some(mod_entry) = self.state.mods.iter().find(|item| item.id == mod_id) else {
            return;
        };
        if !Self::is_unlinked_mod_entry(mod_entry) {
            return;
        }
        let root_path = mod_entry.root_path.clone();
        let folder_name = mod_entry.folder_name.clone();
        let existing_screenshots = mod_entry.metadata.user.screenshots.clone();
        let mod_id = mod_id.to_string();
        let tx = self.manual_image_event_tx.clone();
        let handle = self.runtime_services.handle();
        self.manual_image_imports_pending = self.manual_image_imports_pending.saturating_add(1);
        self.runtime_services.spawn(async move {
            let result = handle.spawn_blocking(move || {
                // A Replace install can leave screenshot entries whose files
                // died with the old folder; only a screenshot that still
                // exists on disk blocks adoption.
                let has_live_screenshot = existing_screenshots
                    .iter()
                    .any(|rel| root_path.join(rel).exists());
                if has_live_screenshot {
                    return Vec::new();
                }
                let discovered = importing::discover_bundled_preview_images(&root_path);
                Self::import_bundled_images_best_effort(&root_path, discovered)
            });
            let rel_paths = result.await.unwrap_or_default();
            // Always send, even empty: the pending counter above must come
            // back down exactly once per enqueue.
            let _ = tx.send(ManualImageEvent::BundledAdopted {
                mod_id,
                folder_name,
                rel_paths,
            });
        });
    }

    fn enqueue_add_images_to_unlinked_mod(
        &mut self,
        mod_id: &str,
        paths: Vec<PathBuf>,
    ) -> Result<()> {
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

    fn overlay_image_copy_source(&self, texture_key: &str) -> Option<OverlayImageCopySource> {
        if let Some(mod_id) = self.selected_mod_id.clone()
            && let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id)
            && texture_key.starts_with(&format!("my-mod-shot-{}-", mod_entry.id))
            && let Some(rel) = mod_entry.metadata.user.screenshots.iter().find(|rel| {
                Self::my_mod_screenshot_texture_key(&mod_entry.id, rel) == texture_key
            })
        {
            return Some(OverlayImageCopySource::File(mod_entry.root_path.join(rel)));
        }

        if let Some(mod_id) = self.browse_state.selected_mod_id
            && let Some(detail) = self.browse_state.details.get(&mod_id)
            && let Some(preview) = &detail.profile.preview_media
            && let Some(url) = preview
                .images
                .iter()
                .map(gamebanana::full_image_url)
                .find(|url| hash64_hex(url.as_bytes()) == texture_key)
        {
            return Some(OverlayImageCopySource::Url(url));
        }

        self.my_mod_overlay_images.iter().find_map(|item| {
            (item.texture_key == texture_key)
                .then(|| item.url.clone())
                .flatten()
                .map(OverlayImageCopySource::Url)
        })
    }

    fn copy_overlay_image_to_clipboard(&mut self, texture_key: &str) {
        let Some(source) = self.overlay_image_copy_source(texture_key) else {
            self.report_warn(
                format!("no copy source found for overlay image {texture_key}"),
                Some(self.text().could_not_copy_image()),
            );
            return;
        };
        let tx = self.overlay_copy_event_tx.clone();
        let portable = self.portable.clone();
        let client = self.runtime_services.http_client();
        let handle = self.runtime_services.handle();
        self.runtime_services.spawn(async move {
            let result: Result<()> = async {
                let bytes = match source {
                    OverlayImageCopySource::File(path) => {
                        tokio::fs::read(&path).await.map_err(|err| {
                            anyhow!("failed to read image file {}: {err}", path.display())
                        })?
                    }
                    OverlayImageCopySource::Url(url) => {
                        let cache_key = format!("img:{}", hash64_hex(url.as_bytes()));
                        let portable_for_get = portable.clone();
                        let cached = handle
                            .spawn_blocking(move || {
                                persistence::cache_get(&portable_for_get, &cache_key)
                            })
                            .await
                            .map_err(|err| anyhow!("image cache read worker failed: {err}"))?
                            // A cache read failure is only a miss here; fall back
                            // to downloading the image again.
                            .unwrap_or(None);
                        match cached {
                            Some(bytes) => bytes,
                            None => client
                                .get(&url)
                                .send()
                                .await?
                                .error_for_status()?
                                .bytes()
                                .await?
                                .to_vec(),
                        }
                    }
                };
                handle
                    .spawn_blocking(move || -> Result<()> {
                        let image = decode_limited_dynamic_image(&bytes)?.into_rgba8();
                        let width = image.width() as usize;
                        let height = image.height() as usize;
                        let mut clipboard = arboard::Clipboard::new()
                            .map_err(|err| anyhow!("failed to open clipboard: {err}"))?;
                        clipboard
                            .set_image(arboard::ImageData {
                                width,
                                height,
                                bytes: image.into_raw().into(),
                            })
                            .map_err(|err| {
                                anyhow!("failed to write image to clipboard: {err}")
                            })?;
                        Ok(())
                    })
                    .await
                    .map_err(|err| anyhow!("clipboard image worker failed: {err}"))?
            }
            .await;
            let _ = tx.send(OverlayImageCopyEvent {
                error: result.err().map(|err| format!("{err:#}")),
            });
        });
    }

    fn consume_overlay_copy_events(&mut self) {
        while let Ok(event) = self.overlay_copy_event_rx.try_recv() {
            match event.error {
                None => {
                    let message = self.text().image_copied().to_string();
                    self.set_message_ok(message);
                }
                Some(error) => {
                    self.report_error_message(error, Some(self.text().could_not_copy_image()));
                }
            }
        }
    }

    fn consume_manual_image_events(&mut self) {
        while let Ok(event) = self.manual_image_event_rx.try_recv() {
            self.manual_image_imports_pending = self.manual_image_imports_pending.saturating_sub(1);
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
                        let Some(mod_entry) =
                            self.state.mods.iter_mut().find(|item| item.id == mod_id)
                        else {
                            self.report_warn(
                                format!(
                                    "manual images imported for missing mod {mod_id}: {}",
                                    rel_paths.join(", ")
                                ),
                                Some(self.text().could_not_attach_images()),
                            );
                            continue;
                        };
                        let old_cover = mod_entry.metadata.user.cover_image.clone();
                        mod_entry.metadata.user.screenshots.extend(rel_paths);
                        Self::sync_mod_cover_to_first_screenshot(mod_entry);
                        let cover_changed = old_cover != mod_entry.metadata.user.cover_image;
                        if let Err(err) = xxmi::save_mod_metadata(mod_entry) {
                            self.report_error(err, Some(self.text().could_not_save_images()));
                            continue;
                        }
                        cover_changed
                    };
                    if cover_changed {
                        self.clear_mod_card_texture(&mod_id);
                    }
                    self.save_state();
                    self.log_action(self.text().images_added_action(), &folder_name);
                    self.set_message_ok(self.text().images_added(count));
                }
                ManualImageEvent::BundledAdopted {
                    mod_id,
                    folder_name,
                    rel_paths,
                } => {
                    if rel_paths.is_empty() {
                        continue;
                    }
                    let cover_changed = {
                        let Some(mod_entry) =
                            self.state.mods.iter_mut().find(|item| item.id == mod_id)
                        else {
                            continue;
                        };
                        let old_cover = mod_entry.metadata.user.cover_image.clone();
                        // Adoption only ran because no listed screenshot still
                        // existed on disk; drop those dead entries so the
                        // adopted previews become the visible set.
                        let root_path = mod_entry.root_path.clone();
                        mod_entry
                            .metadata
                            .user
                            .screenshots
                            .retain(|rel| root_path.join(rel).exists());
                        mod_entry.metadata.user.screenshots.extend(rel_paths);
                        Self::sync_mod_cover_to_first_screenshot(mod_entry);
                        let cover_changed = old_cover != mod_entry.metadata.user.cover_image;
                        if let Err(err) = xxmi::save_mod_metadata(mod_entry) {
                            self.report_error(err, Some(self.text().could_not_save_images()));
                            continue;
                        }
                        cover_changed
                    };
                    if cover_changed {
                        self.clear_mod_card_texture(&mod_id);
                    }
                    self.save_state();
                    self.log_action(self.text().images_added_action(), &folder_name);
                }
                ManualImageEvent::Failed { folder_name, error } => {
                    self.report_error_message(
                        format!("manual image import failed for {folder_name}: {error}"),
                        Some(self.text().app_could_not_add_images()),
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
            self.game_cover_textures.insert(result.game_id, texture);
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
            // `_v2` busts caches from before card thumbs were baked cover-cropped
            // to the card aspect; the old letterboxed `card_thumb.png` files are
            // valid by source-identity, so a filename change is what forces the
            // one-time regeneration.
            ThumbnailProfile::Card => "card_thumb_v2.png",
            ThumbnailProfile::Rail => "rail_thumb.png",
            ThumbnailProfile::Icon => "icon_thumb.png",
        };
        mod_root.join(MOD_META_DIR).join(file_name)
    }

    fn current_card_thumb_meta(
        mod_entry: &ModEntry,
    ) -> (CardThumbMeta, Option<PathBuf>, Option<String>) {
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

    fn is_mod_thumb_valid(
        mod_entry: &ModEntry,
        expected: &CardThumbMeta,
        profile: ThumbnailProfile,
    ) -> bool {
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
            ThumbnailProfile::Icon => return false,
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

    fn update_mod_thumb_meta(
        mod_entry: &mut ModEntry,
        expected: &CardThumbMeta,
        profile: ThumbnailProfile,
    ) {
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
            ThumbnailProfile::Icon => return,
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
            if matches!(
                existing.mode,
                LocalModImageMode::CardThumbOnly | LocalModImageMode::ThumbOnly
            ) && req.mode == LocalModImageMode::FullOnly
            {
                existing.mode = LocalModImageMode::FullOnly;
                existing.payload = req.payload;
            }
            return;
        }
        self.pending_mod_image_queue.push(req);
    }

    fn queue_mod_card_thumb_load_with_priority(
        &mut self,
        mod_id: &str,
        priority: u32,
    ) -> CardThumbQueueOutcome {
        if self.mod_cover_textures.contains_key(mod_id) {
            return CardThumbQueueOutcome::NotNeeded;
        }
        let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id) else {
            return CardThumbQueueOutcome::NotNeeded;
        };
        let (expected_meta, source_path, source_url) = Self::current_card_thumb_meta(mod_entry);
        // A mod with no usable cover source would otherwise be re-requested on
        // every frame that renders its card, and each empty result schedules the
        // next frame — a self-sustaining loop for the whole session.
        if let Some(failure) = self.mod_thumb_unavailable.get(mod_id)
            && failure.still_applies(&expected_meta)
        {
            if expected_meta.kind == "none" {
                return CardThumbQueueOutcome::NoSource;
            }
            return CardThumbQueueOutcome::CoolingDown(failure.remaining_cooldown());
        }
        let force_regen =
            !Self::is_mod_thumb_valid(mod_entry, &expected_meta, ThumbnailProfile::Card);
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
            return CardThumbQueueOutcome::Requested;
        }
        self.pending_mod_image_requests.insert(mod_id.to_string());
        self.enqueue_local_mod_image_request(LocalModImageRequest {
            texture_key: mod_id.to_string(),
            mode: LocalModImageMode::CardThumbOnly,
            priority,
            generation: 0,
            payload,
        });
        CardThumbQueueOutcome::Requested
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
            payload: LocalModImagePayload::Path {
                path,
                thumb_profile,
            },
        });
    }

    fn queue_mod_image_full_load(&mut self, texture_key: String, path: PathBuf, priority: u32) {
        if self.mod_full_textures.contains_key(&texture_key) {
            return;
        }
        // Callers ask once per frame for as long as the texture is missing, and a
        // full-size decode outlives many frames. `enqueue_local_mod_image_request`
        // only dedupes against what is still queued, so a dispatched request has
        // to be tracked here or the same image is decoded dozens of times over.
        if self.inflight_full_image_requests.contains(&texture_key) {
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

    fn prewarm_markdown_images(&mut self, markdown: &str) {
        for dest in extract_markdown_image_dests(markdown) {
            let lower_dest = dest.to_ascii_lowercase();
            if !lower_dest.starts_with("http://") && !lower_dest.starts_with("https://")
                || is_gif_dest(&dest)
            {
                continue;
            }
            self.queue_browse_image_with_profile(dest, None, false, ThumbnailProfile::Rail, 100);
        }
    }

    fn cached_rewrite_markdown_gif_images(
        &mut self,
        markdown: &str,
        mod_root: Option<&Path>,
    ) -> String {
        let root_key = mod_root
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let key = format!(
            "{}:{}",
            hash64_hex(markdown.as_bytes()),
            hash64_hex(root_key.as_bytes())
        );
        if let Some(cached) = self.gif_rewritten_markdown_cache.get(&key) {
            return cached.clone();
        }
        if self.gif_rewritten_markdown_cache.len() >= 64 {
            self.gif_rewritten_markdown_cache.clear();
        }
        let rewritten = rewrite_markdown_gif_images(markdown, mod_root);
        self.gif_rewritten_markdown_cache
            .insert(key, rewritten.clone());
        rewritten
    }

    fn cached_rewrite_markdown_for_render(
        &mut self,
        markdown: &str,
        mod_root: Option<&Path>,
    ) -> String {
        const MARKDOWN_DEPENDENCY_SIGNATURE_TTL_SECS: f64 = 1.0;
        let root_key = mod_root
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let dependency_key = format!(
            "{}:{}",
            hash64_hex(markdown.as_bytes()),
            hash64_hex(root_key.as_bytes())
        );
        let now = Instant::now();
        let dependency_signature = self
            .markdown_dependency_signature_cache
            .get(&dependency_key)
            .filter(|(_, checked_at)| {
                now.duration_since(*checked_at).as_secs_f64()
                    < MARKDOWN_DEPENDENCY_SIGNATURE_TTL_SECS
            })
            .map(|(signature, _)| signature.clone())
            .unwrap_or_else(|| {
                let signature = markdown_image_dependency_signature(markdown, mod_root);
                if self.markdown_dependency_signature_cache.len() >= 64 {
                    self.markdown_dependency_signature_cache.clear();
                }
                self.markdown_dependency_signature_cache
                    .insert(dependency_key, (signature.clone(), now));
                signature
            });
        let key = format!(
            "{}:{}:{}",
            hash64_hex(markdown.as_bytes()),
            hash64_hex(root_key.as_bytes()),
            hash64_hex(dependency_signature.as_bytes())
        );
        if let Some(cached) = self.render_safe_markdown_cache.get(&key) {
            return cached.clone();
        }
        if self.render_safe_markdown_cache.len() >= 64 {
            self.render_safe_markdown_cache.clear();
        }
        let rewritten =
            rewrite_markdown_remote_images_for_render(markdown, &self.portable, mod_root);
        self.render_safe_markdown_cache
            .insert(key, rewritten.clone());
        rewritten
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
                    ui.label(RichText::new(self.text().watch_preview()).size(14.0));
                });
            });

        if button_response
            .response
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            if let Err(err) = open_external_url(&url) {
                self.report_error(err, Some(self.text().app_could_not_open_browser()));
            }
        }
    }

    fn render_markdown_with_inline_images(
        &mut self,
        ui: &mut Ui,
        markdown: &str,
        mod_root: Option<&Path>,
    ) {
        let render_markdown = self.cached_rewrite_markdown_for_render(markdown, mod_root);
        let markdown = render_markdown.as_str();
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
        embeds.extend(
            youtube_embeds
                .into_iter()
                .map(|(start, end, url)| (start, end, InlineMarkdownEmbed::Youtube { url })),
        );
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
                    let is_gif = self.gif_dest_by_texture_key.contains_key(&texture_key);
                    let texture = self
                        .browse_image_textures
                        .get(&texture_key)
                        .or_else(|| self.browse_thumb_textures.get(&texture_key));
                    if let Some(texture) = texture {
                        ui.add_space(8.0);
                        let response = render_inline_markdown_image(ui, texture);
                        ui.add_space(8.0);
                        if let Some(response) = response {
                            if is_gif {
                                let visible_fraction =
                                    visible_rect_fraction(response.rect, ui.clip_rect());
                                if visible_fraction >= GIF_PROCESS_VISIBLE_THRESHOLD {
                                    self.visible_gif_process_texture_keys
                                        .insert(texture_key.clone());
                                }
                                if visible_fraction >= GIF_ANIMATE_VISIBLE_THRESHOLD {
                                    self.ensure_gif_animation_requested(
                                        ui.ctx(),
                                        &texture_key,
                                        [
                                            response.rect.width().round().max(1.0) as u32,
                                            response.rect.height().round().max(1.0) as u32,
                                        ],
                                    );
                                    self.mark_gif_animation_visible(ui.ctx(), &texture_key);
                                    if self.animated_gif_state.contains_key(&texture_key) {
                                        let animation_key = gif_animation_texture_key(&texture_key);
                                        if let Some(texture) =
                                            self.browse_image_textures.get(&animation_key)
                                        {
                                            egui::Image::new(texture)
                                                .fit_to_exact_size(response.rect.size())
                                                .paint_at(ui, response.rect);
                                        }
                                    }
                                }
                            }
                            if response.clicked() {
                                self.browse_state.screenshot_overlay = Some(BrowseOverlayImage {
                                    texture_key: texture_key.clone(),
                                    caption: None,
                                });
                            }
                        }
                    } else if is_gif {
                        ui.add_space(8.0);
                        let preview_pending = self
                            .gif_dest_by_texture_key
                            .get(&texture_key)
                            .map(|dest| {
                                let max_width = gif_preview_max_width(ui.available_width());
                                let out_png = sized_gif_preview_out_path(
                                    gif_preview_out_path(dest, mod_root),
                                    max_width,
                                );
                                self.pending_gif_previews
                                    .contains_key(&out_png.to_string_lossy().to_string())
                            })
                            .unwrap_or(false);
                        let pending = preview_pending
                            || self.pending_gif_animations.contains_key(&texture_key);
                        let response = render_inline_gif_placeholder(ui, pending);
                        ui.add_space(8.0);
                        let visible_fraction = visible_rect_fraction(response.rect, ui.clip_rect());
                        if visible_fraction >= GIF_PROCESS_VISIBLE_THRESHOLD {
                            self.visible_gif_process_texture_keys
                                .insert(texture_key.clone());
                            let width = response.rect.width().round().max(1.0) as u32;
                            self.ensure_gif_preview_requested(
                                ui.ctx(),
                                &texture_key,
                                mod_root,
                                width as f32,
                            );
                        }
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

    fn process_local_mod_image_queue(&mut self, ctx: &egui::Context) {
        if self.pending_mod_image_queue.is_empty() {
            return;
        }
        // A foreground profile operation renames the live mod roots wholesale, and on Windows
        // any open handle below them, including this worker's own image reads, makes that rename
        // fail. Hold the queue until the operation is over; nothing is dropped.
        if self.profile_operation_locks_app() {
            return;
        }

        let pointer_motion_throttle = Self::pointer_motion_image_throttle_active(ctx);
        let dispatch_limit = if pointer_motion_throttle {
            LOCAL_IMAGE_INTERACTIVE_DISPATCH_BATCH
        } else {
            LOCAL_IMAGE_DISPATCH_BATCH
        };

        // CONTEXTUAL THROTTLING: Suspend background work to prioritize current user focus
        let mut allowed_mod_id = String::new();
        let mut focus_mode = false;

        if let Some(mod_id) = self.selected_mod_id.as_ref() {
            if self.mod_detail_open || self.browse_state.screenshot_overlay.is_some() {
                // Only allow if the overlay/detail actually belongs to this local mod
                let belongs = self
                    .browse_state
                    .screenshot_overlay
                    .as_ref()
                    .map_or(true, |o| {
                        o.texture_key == *mod_id
                            || o.texture_key.starts_with(&format!("my-mod-shot-{mod_id}-"))
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
        let mut deferred_for_pointer_motion = false;
        while i < self.pending_mod_image_queue.len() && eligible.len() < dispatch_limit {
            let req = &self.pending_mod_image_queue[i];
            let is_mod_task = req.texture_key == allowed_mod_id
                || req
                    .texture_key
                    .starts_with(&format!("my-mod-shot-{}-", allowed_mod_id))
                || req.texture_key.starts_with("file:///")
                || req.texture_key.starts_with("http");
            let is_background_thumb = req.mode == LocalModImageMode::CardThumbOnly;
            let is_overlay_task = self
                .browse_state
                .screenshot_overlay
                .as_ref()
                .is_some_and(|o| o.texture_key == req.texture_key);
            let is_pointer_motion_eligible = is_overlay_task
                || req.priority <= 5
                || (matches!(
                    req.mode,
                    LocalModImageMode::CardThumbOnly | LocalModImageMode::ThumbOnly
                ) && req.priority <= 20);
            if pointer_motion_throttle && !is_pointer_motion_eligible {
                deferred_for_pointer_motion = true;
                i += 1;
                continue;
            }
            if !focus_mode || is_mod_task || is_background_thumb || is_overlay_task {
                let mut req = self.pending_mod_image_queue.remove(i);
                req.generation = current_gen;
                eligible.push(req);
            } else {
                i += 1;
            }
        }
        if deferred_for_pointer_motion {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        let mut dispatch = eligible.into_iter();
        while let Some(req) = dispatch.next() {
            let full_key =
                (req.mode == LocalModImageMode::FullOnly).then(|| req.texture_key.clone());
            match self.mod_image_request_tx.send(req) {
                Ok(()) => {
                    if let Some(key) = full_key {
                        self.inflight_full_image_requests.insert(key);
                    }
                }
                Err(err) => {
                    for req in dispatch.rev() {
                        self.pending_mod_image_queue.insert(0, req);
                    }
                    self.pending_mod_image_queue.insert(0, err.0);
                    break;
                }
            }
        }
    }

    fn consume_mod_image_results(&mut self) {
        while let Ok(result) = self.mod_image_result_rx.try_recv() {
            if result.done {
                self.pending_mod_image_requests.remove(&result.texture_key);
                self.pending_image_loads.remove(&result.texture_key);
                self.inflight_full_image_requests.remove(&result.texture_key);
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
            // Only card-thumb results carry the source identity they were built
            // from. An empty one means that source has nothing to show, so record
            // it and stop asking until the mod points at a different source.
            if let Some(meta) = result.thumb_meta.as_ref() {
                if result.image_thumb.is_none() {
                    self.mod_thumb_unavailable.insert(
                        result.texture_key.clone(),
                        ModThumbFailure {
                            kind: meta.kind.clone(),
                            id: meta.id.clone(),
                            mtime: meta.mtime,
                            size: meta.size,
                            at: Instant::now(),
                        },
                    );
                } else {
                    self.mod_thumb_unavailable.remove(&result.texture_key);
                }
            }
            if let Some(image_thumb) = result.image_thumb {
                self.pending_texture_uploads
                    .push_back(PendingTextureUpload::ModThumb {
                        texture_key: result.texture_key.clone(),
                        image: image_thumb,
                    });
            }
            if let Some(image_full) = result.image_full {
                self.pending_texture_uploads
                    .push_back(PendingTextureUpload::ModFull {
                        texture_key: result.texture_key,
                        image: image_full,
                    });
            }
        }
    }

    fn consume_gif_animation_events(&mut self, ctx: &egui::Context) {
        let mut markdown_relayout_needed = false;
        while let Ok(event) = self.gif_animation_event_rx.try_recv() {
            self.gif_animation_requests_in_flight =
                self.gif_animation_requests_in_flight.saturating_sub(1);
            match event {
                GifAnimationEvent::Ready {
                    texture_key,
                    animation,
                } => {
                    if self.pending_gif_animations.remove(&texture_key).is_none() {
                        continue;
                    }
                    // Get current time for animation timing
                    let now = ctx.input(|i| i.time);

                    // Load the first frame as an immediate texture
                    if let Some(first_frame) = animation.frames.first() {
                        let anim_texture_key = gif_animation_texture_key(&texture_key);
                        let texture = ctx.load_texture(
                            &anim_texture_key,
                            first_frame.image.clone(),
                            egui::TextureOptions::LINEAR,
                        );
                        self.insert_tracked_texture(
                            TextureKind::BrowseFull,
                            anim_texture_key,
                            3,
                            texture,
                        );
                    }

                    // Store animation state
                    let state = AnimatedGifState {
                        animation,
                        current_frame: 0,
                        frame_start_time: now,
                    };
                    self.animated_gif_state.insert(texture_key.clone(), state);

                    markdown_relayout_needed = true;
                    ctx.request_repaint();
                }
                GifAnimationEvent::Failed {
                    texture_key,
                    error,
                    cancelled,
                } => {
                    if self.pending_gif_animations.remove(&texture_key).is_none() {
                        continue;
                    }
                    if !cancelled {
                        self.report_warn(
                            format!("failed to decode GIF animation for {texture_key}: {error}"),
                            None,
                        );
                    }
                    ctx.request_repaint();
                }
            }
        }
        // Reset once per batch: the reset forces a full markdown re-layout, so doing
        // it per event during a burst of GIF arrivals is wasted work.
        if markdown_relayout_needed {
            self.browse_commonmark_cache = CommonMarkCache::default();
        }
    }

    fn ensure_gif_animation_requested(
        &mut self,
        ctx: &egui::Context,
        texture_key: &str,
        max_size: [u32; 2],
    ) {
        if self.animated_gif_state.contains_key(texture_key) {
            return;
        }
        if self.pending_gif_animations.contains_key(texture_key) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if self.gif_animation_requests_in_flight >= GIF_ANIMATION_MAX_IN_FLIGHT
            || self.gif_requests_in_flight() >= GIF_MAX_IN_FLIGHT
        {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }

        let Some(dest) = self.gif_dest_by_texture_key.get(texture_key).cloned() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));

        let request = if let Some(path) = file_uri_to_path(&dest) {
            Some(GifAnimationRequest::FromFile {
                src_path: path,
                texture_key: texture_key.to_string(),
                max_size,
                cancel: Arc::clone(&cancel),
            })
        } else if dest.starts_with("http://") || dest.starts_with("https://") {
            Some(GifAnimationRequest::FromUrl {
                url: dest,
                texture_key: texture_key.to_string(),
                max_size,
                cancel: Arc::clone(&cancel),
            })
        } else {
            None
        };

        if let Some(request) = request {
            let texture_key = texture_key.to_string();
            self.pending_gif_animations.insert(
                texture_key.clone(),
                PendingGifAnimation {
                    cancel,
                    started_at: Instant::now(),
                },
            );
            if self.gif_animation_request_tx.send(request).is_err() {
                self.pending_gif_animations.remove(&texture_key);
            } else {
                self.gif_animation_requests_in_flight += 1;
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }

    fn mark_gif_animation_visible(&mut self, ctx: &egui::Context, texture_key: &str) {
        self.visible_gif_texture_keys
            .insert(texture_key.to_string());
        if !self.last_visible_gif_texture_keys.contains(texture_key) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn cancel_invisible_gif_work(&mut self) {
        for (texture_key, pending) in &self.pending_gif_animations {
            if !self.visible_gif_texture_keys.contains(texture_key) {
                pending.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    fn cancel_all_gif_work(&mut self) {
        for pending in self.pending_gif_previews.values() {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        self.pending_gif_previews.clear();
        self.gif_preview_requests_in_flight = 0;
        for pending in self.pending_gif_animations.values() {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        self.pending_gif_animations.clear();
        self.gif_animation_requests_in_flight = 0;
        self.animated_gif_state.clear();
        self.visible_gif_process_texture_keys.clear();
        self.visible_gif_texture_keys.clear();
        self.last_visible_gif_texture_keys.clear();
    }

    fn gif_requests_in_flight(&self) -> usize {
        self.gif_preview_requests_in_flight + self.gif_animation_requests_in_flight
    }

    fn enforce_gif_work_timeouts(&mut self) {
        // GIF workers report every request back, including cancelled ones, but a panicked
        // or hung decode would leave its pending entry (and in-flight slot) held forever,
        // which keeps the idle repaint poll alive. Drop requests that outlive any
        // legitimate decode time. The event consumers tolerate a late reply for an
        // already-dropped key, and the counters saturate at zero.
        const GIF_WORK_HARD_TIMEOUT: Duration = Duration::from_secs(60);

        let timed_out: Vec<String> = self
            .pending_gif_previews
            .iter()
            .filter(|(_, pending)| pending.started_at.elapsed() >= GIF_WORK_HARD_TIMEOUT)
            .map(|(out_key, _)| out_key.clone())
            .collect();
        for out_key in timed_out {
            if let Some(pending) = self.pending_gif_previews.remove(&out_key) {
                pending.cancel.store(true, Ordering::Relaxed);
                self.gif_preview_requests_in_flight =
                    self.gif_preview_requests_in_flight.saturating_sub(1);
                self.log_warn(format!(
                    "gif preview timed out without a worker response; dropping it: {out_key}"
                ));
            }
        }

        let timed_out: Vec<String> = self
            .pending_gif_animations
            .iter()
            .filter(|(_, pending)| pending.started_at.elapsed() >= GIF_WORK_HARD_TIMEOUT)
            .map(|(texture_key, _)| texture_key.clone())
            .collect();
        for texture_key in timed_out {
            if let Some(pending) = self.pending_gif_animations.remove(&texture_key) {
                pending.cancel.store(true, Ordering::Relaxed);
                self.gif_animation_requests_in_flight =
                    self.gif_animation_requests_in_flight.saturating_sub(1);
                self.log_warn(format!(
                    "gif animation timed out without a worker response; dropping it: {texture_key}"
                ));
            }
        }
    }

    fn update_gif_animations(&mut self, ctx: &egui::Context) {
        if self.visible_gif_texture_keys.is_empty() || self.animated_gif_state.is_empty() {
            return;
        }

        let now = ctx.input(|i| i.time);
        let visible_keys: Vec<String> = self.visible_gif_texture_keys.iter().cloned().collect();
        let mut texture_updates = Vec::new();
        let mut next_repaint_ms: Option<u64> = None;

        for texture_key in visible_keys {
            let Some(state) = self.animated_gif_state.get_mut(&texture_key) else {
                continue;
            };
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
                    let until_next_frame = time_accum.saturating_sub(loop_elapsed).max(1);
                    next_repaint_ms =
                        Some(next_repaint_ms.map_or(until_next_frame as u64, |current| {
                            current.min(until_next_frame as u64)
                        }));
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
                    let anim_texture_key = gif_animation_texture_key(&texture_key);
                    let texture = ctx.load_texture(
                        &anim_texture_key,
                        frame.image.clone(),
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(
                        TextureKind::BrowseFull,
                        anim_texture_key,
                        3,
                        texture,
                    );
                }
            }
        }

        if let Some(next_repaint_ms) = next_repaint_ms {
            ctx.request_repaint_after(Duration::from_millis(next_repaint_ms));
        }
    }

    fn consume_gif_preview_events(&mut self, ctx: &egui::Context) {
        let mut markdown_relayout_needed = false;
        while let Ok(event) = self.gif_preview_event_rx.try_recv() {
            self.gif_preview_requests_in_flight =
                self.gif_preview_requests_in_flight.saturating_sub(1);
            match event {
                GifPreviewEvent::Ready {
                    out_png,
                    gif_dest,
                    image,
                } => {
                    let out_key = out_png.to_string_lossy().to_string();
                    let Some(pending) = self.pending_gif_previews.remove(&out_key) else {
                        continue;
                    };
                    let texture_key = format!("gif-preview-{}", hash64_hex(gif_dest.as_bytes()));
                    if pending.texture_key != texture_key {
                        continue;
                    }

                    let texture =
                        ctx.load_texture(&texture_key, image, egui::TextureOptions::LINEAR);
                    self.insert_tracked_texture(TextureKind::BrowseFull, texture_key, 3, texture);
                    markdown_relayout_needed = true;
                }
                GifPreviewEvent::Failed { out_png } => {
                    let out_key = out_png.to_string_lossy().to_string();
                    self.pending_gif_previews.remove(&out_key);
                }
            }
            ctx.request_repaint();
        }
        // Reset once per batch: the reset forces a full markdown re-layout, so doing
        // it per event during a burst of GIF arrivals is wasted work.
        if markdown_relayout_needed {
            self.browse_commonmark_cache = CommonMarkCache::default();
        }
    }

    fn queue_gif_previews_for_markdown(
        &mut self,
        ctx: &egui::Context,
        markdown: &str,
        mod_root: Option<&Path>,
        max_width: f32,
    ) {
        let max_width = gif_preview_max_width(max_width);
        for dest in extract_markdown_image_dests(markdown) {
            if !is_gif_dest(&dest) {
                continue;
            }
            let local_path = file_uri_to_path(&dest)
                .filter(|path| is_hestia_controlled_image_path(path, mod_root));
            let remote_url = if dest.starts_with("http://") || dest.starts_with("https://") {
                Some(dest.clone())
            } else {
                None
            };
            if local_path.is_none() && remote_url.is_none() {
                continue;
            }

            let texture_key = format!("gif-preview-{}", hash64_hex(dest.as_bytes()));
            self.gif_dest_by_texture_key
                .insert(texture_key.clone(), dest.clone());
            if self
                .browse_image_textures
                .get(&texture_key)
                .is_some_and(|texture| texture.size()[0] as u32 >= max_width)
            {
                continue;
            }

            self.ensure_gif_preview_requested(ctx, &texture_key, mod_root, max_width as f32);
        }
    }

    fn ensure_gif_preview_requested(
        &mut self,
        ctx: &egui::Context,
        texture_key: &str,
        mod_root: Option<&Path>,
        max_width: f32,
    ) {
        if self.browse_image_textures.contains_key(texture_key) {
            return;
        }
        let max_width = gif_preview_max_width(max_width);
        let Some(dest) = self.gif_dest_by_texture_key.get(texture_key).cloned() else {
            return;
        };
        let local_path =
            file_uri_to_path(&dest).filter(|path| is_hestia_controlled_image_path(path, mod_root));
        let remote_url = if dest.starts_with("http://") || dest.starts_with("https://") {
            Some(dest.clone())
        } else {
            None
        };
        if local_path.is_none() && remote_url.is_none() {
            return;
        }

        let out_png = sized_gif_preview_out_path(gif_preview_out_path(&dest, mod_root), max_width);
        if out_png.exists() {
            if let Ok(bytes) = std::fs::read(&out_png) {
                if let Some(image) = load_cover_color_image(&bytes) {
                    let texture =
                        ctx.load_texture(texture_key, image, egui::TextureOptions::LINEAR);
                    self.insert_tracked_texture(
                        TextureKind::BrowseFull,
                        texture_key.to_string(),
                        3,
                        texture,
                    );
                    self.browse_commonmark_cache = CommonMarkCache::default();
                    ctx.request_repaint();
                }
            }
            return;
        }

        let out_key = out_png.to_string_lossy().to_string();
        if self.pending_gif_previews.contains_key(&out_key) {
            return;
        }
        if self.gif_preview_requests_in_flight >= GIF_PREVIEW_MAX_IN_FLIGHT
            || self.gif_requests_in_flight() >= GIF_MAX_IN_FLIGHT
        {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_gif_previews.insert(
            out_key.clone(),
            PendingGifPreview {
                texture_key: texture_key.to_string(),
                cancel: Arc::clone(&cancel),
                started_at: Instant::now(),
            },
        );
        let sent = if let Some(path) = local_path {
            self.gif_preview_request_tx
                .send(GifPreviewRequest::FromFile {
                    src_path: path,
                    out_png,
                    gif_dest: dest,
                    max_width,
                    cancel: Arc::clone(&cancel),
                })
                .is_ok()
        } else if let Some(url) = remote_url {
            self.gif_preview_request_tx
                .send(GifPreviewRequest::FromUrl {
                    url,
                    out_png,
                    gif_dest: dest,
                    max_width,
                    cancel: Arc::clone(&cancel),
                })
                .is_ok()
        } else {
            false
        };

        if sent {
            self.gif_preview_requests_in_flight += 1;
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            self.pending_gif_previews.remove(&out_key);
        }
    }

    fn texture_upload_exceeds_budget(
        current_bytes: u64,
        upload_bytes: u64,
        max_bytes: u64,
        uploads_done: usize,
    ) -> bool {
        uploads_done > 0 && current_bytes.saturating_add(upload_bytes) > max_bytes
    }

    fn pointer_motion_image_throttle_active(ctx: &egui::Context) -> bool {
        ctx.input(|input| {
            input.pointer.is_moving()
                && input.pointer.hover_pos().is_some()
                && !input.pointer.any_pressed()
                && !input.pointer.any_released()
        })
    }

    fn process_pending_texture_uploads(&mut self, ctx: &egui::Context) {
        if self.pending_texture_uploads.len() > 1 {
            let mut uploads: Vec<_> = self.pending_texture_uploads.drain(..).collect();
            uploads.sort_by_key(PendingTextureUpload::priority_class);
            self.pending_texture_uploads = uploads.into();
        }
        let mut thumb_uploads = 0;
        let mut full_uploads = 0;
        let mut thumb_upload_bytes = 0_u64;
        let mut full_upload_bytes = 0_u64;
        let mut inspected = 0;
        let pending_count = self.pending_texture_uploads.len();
        let mut uploaded_any = false;
        while inspected < pending_count
            && (thumb_uploads < TEXTURE_THUMB_UPLOADS_PER_FRAME
                || full_uploads < TEXTURE_FULL_UPLOADS_PER_FRAME)
        {
            let Some(item) = self.pending_texture_uploads.pop_front() else {
                break;
            };
            inspected += 1;
            match item {
                PendingTextureUpload::ModThumb { texture_key, image } => {
                    if self.mod_cover_textures.contains_key(&texture_key) {
                        continue;
                    }
                    let upload_bytes = image.pixels.len().saturating_mul(4) as u64;
                    if thumb_uploads >= TEXTURE_THUMB_UPLOADS_PER_FRAME
                        || Self::texture_upload_exceeds_budget(
                            thumb_upload_bytes,
                            upload_bytes,
                            TEXTURE_THUMB_UPLOAD_BYTES_PER_FRAME,
                            thumb_uploads,
                        )
                    {
                        self.pending_texture_uploads
                            .push_back(PendingTextureUpload::ModThumb { texture_key, image });
                        continue;
                    }
                    let texture = ctx.load_texture(
                        format!("mod-thumb-{}", texture_key),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::ModThumb, texture_key, 2, texture);
                    thumb_uploads += 1;
                    thumb_upload_bytes = thumb_upload_bytes.saturating_add(upload_bytes);
                    uploaded_any = true;
                }
                PendingTextureUpload::ModFull { texture_key, image } => {
                    if self.mod_full_textures.contains_key(&texture_key) {
                        continue;
                    }
                    let upload_bytes = image.pixels.len().saturating_mul(4) as u64;
                    if full_uploads >= TEXTURE_FULL_UPLOADS_PER_FRAME
                        || Self::texture_upload_exceeds_budget(
                            full_upload_bytes,
                            upload_bytes,
                            TEXTURE_FULL_UPLOAD_BYTES_PER_FRAME,
                            full_uploads,
                        )
                    {
                        self.pending_texture_uploads
                            .push_back(PendingTextureUpload::ModFull { texture_key, image });
                        continue;
                    }
                    let texture =
                        ctx.load_texture(texture_key.clone(), image, egui::TextureOptions::LINEAR);
                    self.insert_tracked_texture(TextureKind::ModFull, texture_key, 3, texture);
                    full_uploads += 1;
                    full_upload_bytes = full_upload_bytes.saturating_add(upload_bytes);
                    uploaded_any = true;
                }
                PendingTextureUpload::BrowseThumb { texture_key, image } => {
                    if self.browse_thumb_textures.contains_key(&texture_key) {
                        continue;
                    }
                    let upload_bytes = image.pixels.len().saturating_mul(4) as u64;
                    if thumb_uploads >= TEXTURE_THUMB_UPLOADS_PER_FRAME
                        || Self::texture_upload_exceeds_budget(
                            thumb_upload_bytes,
                            upload_bytes,
                            TEXTURE_THUMB_UPLOAD_BYTES_PER_FRAME,
                            thumb_uploads,
                        )
                    {
                        self.pending_texture_uploads
                            .push_back(PendingTextureUpload::BrowseThumb { texture_key, image });
                        continue;
                    }
                    let texture = ctx.load_texture(
                        format!("browse-image-thumb-{}", texture_key),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.insert_tracked_texture(TextureKind::BrowseThumb, texture_key, 2, texture);
                    thumb_uploads += 1;
                    thumb_upload_bytes = thumb_upload_bytes.saturating_add(upload_bytes);
                    uploaded_any = true;
                }
                PendingTextureUpload::BrowseFull { texture_key, image } => {
                    if self.browse_image_textures.contains_key(&texture_key) {
                        continue;
                    }
                    let upload_bytes = image.pixels.len().saturating_mul(4) as u64;
                    if full_uploads >= TEXTURE_FULL_UPLOADS_PER_FRAME
                        || Self::texture_upload_exceeds_budget(
                            full_upload_bytes,
                            upload_bytes,
                            TEXTURE_FULL_UPLOAD_BYTES_PER_FRAME,
                            full_uploads,
                        )
                    {
                        self.pending_texture_uploads
                            .push_back(PendingTextureUpload::BrowseFull { texture_key, image });
                        continue;
                    }
                    let texture =
                        ctx.load_texture(texture_key.clone(), image, egui::TextureOptions::LINEAR);
                    self.insert_tracked_texture(TextureKind::BrowseFull, texture_key, 3, texture);
                    full_uploads += 1;
                    full_upload_bytes = full_upload_bytes.saturating_add(upload_bytes);
                    uploaded_any = true;
                }
            }
        }
        if uploaded_any || !self.pending_texture_uploads.is_empty() {
            ctx.request_repaint();
        }
        self.evict_textures_to_budget(ctx.input(|i| i.time));
    }
}

fn render_inline_markdown_image(
    ui: &mut Ui,
    texture: &egui::TextureHandle,
) -> Option<egui::Response> {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }

    let max_width = ui.available_width().max(1.0);
    let scale = (max_width / size.x).min(1.0);
    Some(
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(size * scale)
                .sense(Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand),
    )
}

fn render_inline_gif_placeholder(ui: &mut Ui, _pending: bool) -> egui::Response {
    let width = ui.available_width().max(1.0);
    let height = 44.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let fill = if response.hovered() {
        Color32::from_rgb(43, 45, 49)
    } else {
        Color32::from_rgb(35, 37, 41)
    };
    ui.painter().rect(
        rect,
        6.0,
        fill,
        egui::Stroke::new(1.0, Color32::from_rgb(76, 78, 84)),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Loading GIF...",
        egui::FontId::proportional(13.0),
        Color32::from_gray(225),
    );
    response
}

fn gif_preview_max_width(width: f32) -> u32 {
    let width = width.ceil().clamp(1.0, 4096.0) as u32;
    width.div_ceil(64).saturating_mul(64).min(4096)
}

fn gif_animation_texture_key(texture_key: &str) -> String {
    format!("{texture_key}:anim")
}

fn visible_rect_fraction(rect: egui::Rect, clip_rect: egui::Rect) -> f32 {
    let area = (rect.width().max(0.0) * rect.height().max(0.0)).max(1.0);
    let intersection_min = egui::pos2(
        rect.min.x.max(clip_rect.min.x),
        rect.min.y.max(clip_rect.min.y),
    );
    let intersection_max = egui::pos2(
        rect.max.x.min(clip_rect.max.x),
        rect.max.y.min(clip_rect.max.y),
    );
    let width = (intersection_max.x - intersection_min.x).max(0.0);
    let height = (intersection_max.y - intersection_min.y).max(0.0);
    (width * height / area).clamp(0.0, 1.0)
}

fn sized_gif_preview_out_path(path: PathBuf, max_width: u32) -> PathBuf {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return path;
    };
    let extension = path.extension().and_then(|extension| extension.to_str());
    let file_name = match extension {
        Some(extension) if !extension.is_empty() => {
            format!("{stem}_w{max_width}.{extension}")
        }
        _ => format!("{stem}_w{max_width}"),
    };
    path.with_file_name(file_name)
}
