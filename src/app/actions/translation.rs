impl HestiaApp {
    fn current_translation_lang(&self) -> &'static str {
        match self.state.static_prefs.language {
            AppLanguage::English => "en",
            AppLanguage::Indonesian => "id",
            AppLanguage::ChineseSimplified => "cn",
            AppLanguage::Russian => "ru",
        }
    }

    fn translation_source_hash_from_raw(raw: &str) -> String {
        format!("{:016x}", xxh3_64(raw.as_bytes()))
    }

    fn translation_source_hash_from_profile(profile: &gamebanana::ProfileResponse) -> String {
        serde_json::to_vec(profile)
            .map(|raw| format!("{:016x}", xxh3_64(&raw)))
            .unwrap_or_else(|_| format!("{:016x}", xxh3_64(profile.name.as_bytes())))
    }

    fn unlinked_text_content_hash(content: &str) -> String {
        Sha256::digest(content.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn unlinked_translation_memory_key(lang: &str, content_hash: &str) -> String {
        format!("{lang}:{content_hash}")
    }

    fn next_translation_request_id(&mut self) -> u64 {
        self.translation_request_nonce = self.translation_request_nonce.wrapping_add(1);
        if self.translation_request_nonce == 0 {
            self.translation_request_nonce = 1;
        }
        self.translation_request_nonce
    }

    fn unlinked_texts_to_translate(&self, mod_entry_id: &str) -> Vec<(String, String)> {
        let Some(mod_entry) = self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_entry_id)
        else {
            return Vec::new();
        };
        if mod_entry
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .is_some()
        {
            return Vec::new();
        }

        let personal_note_path = xxmi::personal_note_relative_path();
        let mut contents = Vec::new();
        if let Some(description) = mod_entry
            .metadata
            .user
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty())
        {
            contents.push(description.to_string());
        }
        contents.extend(
            mod_entry
                .metadata
                .extracted
                .text_sources
                .iter()
                .filter(|source| source.path != personal_note_path)
                .map(|source| source.content.trim())
                .filter(|content| !content.is_empty())
                .map(ToString::to_string),
        );
        let mut seen = HashSet::new();
        contents
            .into_iter()
            .map(|content| {
                let content_hash = Self::unlinked_text_content_hash(&content);
                (content, content_hash)
            })
            .filter(|(_, content_hash)| seen.insert(content_hash.clone()))
            .collect()
    }

    fn unlinked_metadata_source_content(&self, mod_entry_id: &str) -> Option<String> {
        let mod_entry = self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_entry_id)?;
        let source_path = mod_entry.metadata.extracted.readme_path.as_deref()?;
        if source_path == xxmi::personal_note_relative_path() {
            return None;
        }
        mod_entry
            .metadata
            .extracted
            .text_sources
            .iter()
            .find(|source| source.path == source_path)
            .map(|source| source.content.trim())
            .filter(|content| !content.is_empty())
            .map(ToString::to_string)
    }

    fn unlinked_translation_for_content(&self, mod_entry_id: &str, content: &str) -> Option<&str> {
        let content_hash = Self::unlinked_text_content_hash(content.trim());
        let key =
            Self::unlinked_translation_memory_key(self.current_translation_lang(), &content_hash);
        self.my_mods_translation_state
            .get(mod_entry_id)?
            .unlinked_translation_enabled
            .then(|| {
                self.my_mods_translation_state
                    .get(mod_entry_id)
                    .and_then(|state| state.unlinked_translations.get(&key))
                    .map(String::as_str)
            })
            .flatten()
    }

    fn request_unlinked_text_translation(
        &mut self,
        mod_entry_id: &str,
        content: String,
        force_refresh: bool,
    ) {
        let lang = self.current_translation_lang().to_string();
        let content_hash = Self::unlinked_text_content_hash(&content);
        let memory_key = Self::unlinked_translation_memory_key(&lang, &content_hash);
        if !force_refresh {
            if let Some(translation) = cached_unlinked_text_translation(&lang, &content_hash) {
                self.my_mods_translation_state
                    .entry(mod_entry_id.to_string())
                    .or_default()
                    .unlinked_translations
                    .insert(memory_key, translation);
                return;
            }
        }

        let inflight_key = (mod_entry_id.to_string(), lang.clone(), content_hash.clone());
        if self
            .unlinked_translation_inflight
            .contains_key(&inflight_key)
        {
            return;
        }
        let request_id = self.next_translation_request_id();
        self.unlinked_translation_inflight
            .insert(inflight_key, request_id);
        let cancellation = Arc::new(AtomicBool::new(false));
        self.unlinked_translation_cancellations
            .insert(request_id, Arc::clone(&cancellation));
        self.my_mods_translation_state
            .entry(mod_entry_id.to_string())
            .or_default()
            .unlinked_loading
            .insert(memory_key);
        let _ = self
            .translation_request_tx
            .send(TranslationRequest::UnlinkedText {
                request_id,
                cancellation,
                mod_entry_id: mod_entry_id.to_string(),
                lang,
                content,
                content_hash,
                force_refresh,
            });
    }

    fn enable_unlinked_translation(&mut self, mod_entry_id: &str, force_refresh: bool) {
        self.my_mods_translation_state
            .entry(mod_entry_id.to_string())
            .or_default()
            .unlinked_translation_enabled = true;
        for (content, _) in self.unlinked_texts_to_translate(mod_entry_id) {
            self.request_unlinked_text_translation(mod_entry_id, content, force_refresh);
        }
    }

    fn disable_unlinked_translation(&mut self, mod_entry_id: &str) {
        if let Some(state) = self.my_mods_translation_state.get_mut(mod_entry_id) {
            state.unlinked_translation_enabled = false;
            state.unlinked_loading.clear();
        }
    }

    fn cancel_unlinked_translation(&mut self, mod_entry_id: &str) {
        let keys: Vec<(String, String, String)> = self
            .unlinked_translation_inflight
            .keys()
            .filter(|(inflight_mod_id, _, _)| inflight_mod_id == mod_entry_id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(request_id) = self.unlinked_translation_inflight.remove(&key) {
                if let Some(cancellation) = self.unlinked_translation_cancellations.get(&request_id)
                {
                    cancellation.store(true, Ordering::Relaxed);
                }
                self.cancelled_translation_requests.insert(request_id);
            }
        }
        self.disable_unlinked_translation(mod_entry_id);
    }

    fn cancel_gamebanana_translation(&mut self, mod_id: u64) {
        let keys: Vec<(u64, String, String)> = self
            .translation_inflight
            .keys()
            .filter(|(inflight_mod_id, _, _)| *inflight_mod_id == mod_id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(request_id) = self.translation_inflight.remove(&key) {
                self.cancelled_translation_requests.insert(request_id);
            }
        }
        self.set_translation_loading_for_gamebanana_mod(mod_id, false);
    }

    pub(crate) fn handle_unlinked_metadata_source_changed(&mut self, mod_entry_id: &str) {
        if !self.state.static_prefs.always_translate_mod_details {
            self.disable_unlinked_translation(mod_entry_id);
            return;
        }
        self.my_mods_translation_state
            .entry(mod_entry_id.to_string())
            .or_default()
            .unlinked_translation_enabled = true;
        if let Some(content) = self.unlinked_metadata_source_content(mod_entry_id) {
            self.request_unlinked_text_translation(mod_entry_id, content, false);
        }
    }

    fn translation_cache_key(mod_id: u64, lang: &str, source_hash: &str) -> String {
        format!("gb_profile_{mod_id}-{lang}-{source_hash}.json")
    }

    fn legacy_translation_cache_key(mod_id: u64, lang: &str) -> String {
        format!("gb_profile_{mod_id}-{lang}.json")
    }

    fn translation_cache_path(mod_id: u64, lang: &str, source_hash: &str) -> std::path::PathBuf {
        persistence::cache_file_path(&Self::translation_cache_key(mod_id, lang, source_hash))
    }

    fn cached_translation_profile(
        mod_id: u64,
        lang: &str,
        source_hash: &str,
    ) -> Option<gamebanana::ProfileResponse> {
        let cache_path = Self::translation_cache_path(mod_id, lang, source_hash);
        std::fs::read_to_string(cache_path)
            .ok()
            .and_then(|json| serde_json::from_str::<gamebanana::ProfileResponse>(&json).ok())
    }

    fn translation_cache_index_path() -> std::path::PathBuf {
        persistence::runtime_temp_cache_dir().join("translation-cache-index.json")
    }

    fn known_translation_source_hash_for_gamebanana_mod(&self, mod_id: u64) -> String {
        if let Some(detail) = self.browse_state.details.get(&mod_id) {
            return Self::translation_source_hash_from_profile(&detail.profile);
        }

        self.state
            .mods
            .iter()
            .filter_map(|mod_entry| {
                let source = mod_entry.source.as_ref()?;
                let link = source.gamebanana.as_ref()?;
                (link.mod_id == mod_id).then_some(source)
            })
            .find_map(|source| {
                source
                    .raw_profile_json
                    .as_deref()
                    .map(Self::translation_source_hash_from_raw)
                    .or_else(|| {
                        source
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| serde_json::to_vec(snapshot).ok())
                            .map(|raw| format!("{:016x}", xxh3_64(&raw)))
                    })
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn apply_translated_profile(
        &mut self,
        mod_id: u64,
        lang: &str,
        source_hash: &str,
        profile: gamebanana::ProfileResponse,
    ) {
        if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
            let markdown = prepare_markdown_for_display(
                profile.html_text.as_deref().unwrap_or_default(),
                None,
                Some(mod_id),
                &self.portable,
            );

            detail.translated_profile = Some(profile.clone());
            detail.translation_lang = Some(lang.to_string());
            detail.translation_loading = false;
            detail.markdown = markdown;
        }

        let mod_ids_to_update: Vec<String> = self
            .state
            .mods
            .iter()
            .filter(|m| {
                m.source
                    .as_ref()
                    .and_then(|s| s.gamebanana.as_ref())
                    .map(|l| l.mod_id == mod_id)
                    .unwrap_or(false)
            })
            .map(|m| m.id.clone())
            .collect();

        for mod_entry_id in mod_ids_to_update {
            let state = self
                .my_mods_translation_state
                .entry(mod_entry_id)
                .or_default();
            state.translated_profile = Some(profile.clone());
            state.translation_lang = Some(lang.to_string());
            state.translation_source_hash = Some(source_hash.to_string());
            state.translation_loading = false;
        }
    }

    fn set_translation_loading_for_gamebanana_mod(&mut self, mod_id: u64, loading: bool) {
        if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
            detail.translation_loading = loading;
        }

        let mod_ids_to_update: Vec<String> = self
            .state
            .mods
            .iter()
            .filter(|m| {
                m.source
                    .as_ref()
                    .and_then(|s| s.gamebanana.as_ref())
                    .map(|l| l.mod_id == mod_id)
                    .unwrap_or(false)
            })
            .map(|m| m.id.clone())
            .collect();

        for mod_entry_id in mod_ids_to_update {
            let state = self
                .my_mods_translation_state
                .entry(mod_entry_id)
                .or_default();
            state.translation_loading = loading;
        }
    }

    fn request_translation_for_gamebanana_mod(
        &mut self,
        mod_id: u64,
        lang: &str,
        force_refresh: bool,
    ) {
        let source_hash = self.known_translation_source_hash_for_gamebanana_mod(mod_id);
        let inflight_key = (mod_id, lang.to_string(), source_hash.clone());
        if self.translation_inflight.contains_key(&inflight_key) {
            self.set_translation_loading_for_gamebanana_mod(mod_id, true);
            return;
        }

        if !force_refresh {
            if let Some(profile) = Self::cached_translation_profile(mod_id, lang, &source_hash) {
                self.apply_translated_profile(mod_id, lang, &source_hash, profile);
                return;
            }
        }

        self.set_translation_loading_for_gamebanana_mod(mod_id, true);
        let request_id = self.next_translation_request_id();
        self.translation_inflight.insert(inflight_key, request_id);
        let _ = self
            .translation_request_tx
            .send(TranslationRequest::GameBanana {
                request_id,
                mod_id,
                lang: lang.to_string(),
                source_hash,
                force_refresh,
            });
    }

    fn request_translation_for_mod_entry(&mut self, mod_entry_id: &str, force_refresh: bool) {
        let Some(gb_id) = self
            .state
            .mods
            .iter()
            .find(|m| m.id == mod_entry_id)
            .and_then(|m| m.source.as_ref())
            .and_then(|s| s.gamebanana.as_ref())
            .map(|l| l.mod_id)
        else {
            return;
        };
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(gb_id, lang, force_refresh);
    }

    pub(crate) fn maybe_translate_gamebanana_mod_details(&mut self, mod_id: u64) {
        if !self.state.static_prefs.always_translate_mod_details {
            return;
        }
        if mod_id == 0 {
            return;
        }
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(mod_id, lang, false);
    }

    pub(crate) fn maybe_translate_my_mod_details(&mut self, mod_entry_id: &str) {
        if !self.state.static_prefs.always_translate_mod_details {
            return;
        }
        let linked = self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_entry_id)
            .is_some_and(|mod_entry| {
                mod_entry
                    .source
                    .as_ref()
                    .and_then(|source| source.gamebanana.as_ref())
                    .is_some()
            });
        if linked {
            self.request_translation_for_mod_entry(mod_entry_id, false);
        } else {
            self.enable_unlinked_translation(mod_entry_id, false);
        }
    }

    pub(crate) fn toggle_browse_translation(&mut self, mod_id: u64) {
        let Some(detail) = self.browse_state.details.get_mut(&mod_id) else {
            return;
        };

        // If translation is loading, show toast and return
        if detail.translation_loading {
            self.cancel_gamebanana_translation(mod_id);
            return;
        }

        // If translation is active, toggle it off
        if detail.translation_lang.is_some() {
            // Restore original markdown
            let markdown = prepare_markdown_for_display(
                detail.profile.html_text.as_deref().unwrap_or_default(),
                None,
                Some(mod_id),
                &self.portable,
            );

            detail.translation_lang = None;
            detail.translated_profile = None;
            detail.markdown = markdown;
            return;
        }

        // Start translation
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(mod_id, lang, false);
    }

    pub(crate) fn retranslate_browse_translation(&mut self, mod_id: u64) {
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(mod_id, lang, true);
    }

    pub(crate) fn toggle_my_mods_translation(&mut self, mod_id: String) {
        let linked = self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_id)
            .is_some_and(|mod_entry| {
                mod_entry
                    .source
                    .as_ref()
                    .and_then(|source| source.gamebanana.as_ref())
                    .is_some()
            });
        if !linked {
            let active = self
                .my_mods_translation_state
                .get(&mod_id)
                .is_some_and(|state| state.unlinked_translation_enabled);
            if active {
                self.cancel_unlinked_translation(&mod_id);
            } else {
                self.enable_unlinked_translation(&mod_id, false);
            }
            return;
        }

        let translation_loading = self
            .my_mods_translation_state
            .entry(mod_id.clone())
            .or_default()
            .translation_loading;

        if translation_loading {
            let gamebanana_id = self
                .state
                .mods
                .iter()
                .find(|mod_entry| mod_entry.id == mod_id)
                .and_then(|mod_entry| mod_entry.source.as_ref())
                .and_then(|source| source.gamebanana.as_ref())
                .map(|link| link.mod_id);
            if let Some(gamebanana_id) = gamebanana_id {
                self.cancel_gamebanana_translation(gamebanana_id);
            }
            return;
        }

        let translation_state = self
            .my_mods_translation_state
            .entry(mod_id.clone())
            .or_default();

        // If translation is active, toggle it off
        if translation_state.translation_lang.is_some() {
            translation_state.translation_lang = None;
            translation_state.translated_profile = None;
            return;
        }

        // Start translation
        self.request_translation_for_mod_entry(&mod_id, false);
    }

    pub(crate) fn retranslate_my_mods_translation(&mut self, mod_id: String) {
        let linked = self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_id)
            .is_some_and(|mod_entry| {
                mod_entry
                    .source
                    .as_ref()
                    .and_then(|source| source.gamebanana.as_ref())
                    .is_some()
            });
        if linked {
            self.request_translation_for_mod_entry(&mod_id, true);
        } else {
            self.enable_unlinked_translation(&mod_id, true);
        }
    }

    pub(crate) fn retranslate_visible_detail_after_language_change(&mut self) {
        if !self.state.static_prefs.always_translate_mod_details {
            return;
        }

        match self.current_view {
            ViewMode::Browse => {
                let Some(mod_id) = self.browse_state.selected_mod_id else {
                    return;
                };
                if !self.browse_detail_open {
                    return;
                }

                let translation_active = self
                    .browse_state
                    .details
                    .get(&mod_id)
                    .map(|detail| detail.translation_lang.is_some())
                    .unwrap_or(false);
                if !translation_active {
                    return;
                }

                let lang = self.current_translation_lang();
                self.request_translation_for_gamebanana_mod(mod_id, lang, false);
            }
            ViewMode::Library => {
                if !self.mod_detail_open {
                    return;
                }

                let Some(mod_entry_id) = self.selected_mod().map(|mod_entry| mod_entry.id.clone())
                else {
                    return;
                };

                let translation_active = self
                    .my_mods_translation_state
                    .get(&mod_entry_id)
                    .is_some_and(|state| {
                        state.translation_lang.is_some() || state.unlinked_translation_enabled
                    });
                if !translation_active {
                    return;
                }

                self.maybe_translate_my_mod_details(&mod_entry_id);
            }
        }
    }

    pub(crate) fn handle_translation_events(&mut self) {
        while let Ok(event) = self.translation_event_rx.try_recv() {
            let TranslationEvent::GameBanana {
                request_id,
                mod_id,
                lang,
                source_hash,
                result,
            } = event
            else {
                let TranslationEvent::UnlinkedText {
                    request_id,
                    mod_entry_id,
                    lang,
                    content_hash,
                    result,
                } = event
                else {
                    unreachable!();
                };
                let inflight_key = (mod_entry_id.clone(), lang.clone(), content_hash.clone());
                if self.unlinked_translation_inflight.get(&inflight_key) == Some(&request_id) {
                    self.unlinked_translation_inflight.remove(&inflight_key);
                }
                self.unlinked_translation_cancellations.remove(&request_id);
                if self.cancelled_translation_requests.remove(&request_id) {
                    continue;
                }
                let memory_key = Self::unlinked_translation_memory_key(&lang, &content_hash);
                if let Some(state) = self.my_mods_translation_state.get_mut(&mod_entry_id) {
                    if !self
                        .unlinked_translation_inflight
                        .contains_key(&inflight_key)
                    {
                        state.unlinked_loading.remove(&memory_key);
                    }
                }
                if lang != self.current_translation_lang()
                    || !self
                        .unlinked_texts_to_translate(&mod_entry_id)
                        .iter()
                        .any(|(_, hash)| hash == &content_hash)
                {
                    continue;
                }
                match result {
                    Ok(translation) => {
                        self.my_mods_translation_state
                            .entry(mod_entry_id)
                            .or_default()
                            .unlinked_translations
                            .insert(memory_key, translation);
                    }
                    Err(err) => {
                        let text = self.text();
                        self.set_message_ok(text.translation_failed());
                        self.log_error(&format!("Translation failed for unlinked mod: {err}"));
                    }
                }
                continue;
            };
            let inflight_key = (mod_id, lang.clone(), source_hash.clone());
            if self.translation_inflight.get(&inflight_key) == Some(&request_id) {
                self.translation_inflight.remove(&inflight_key);
            }
            if self.cancelled_translation_requests.remove(&request_id) {
                continue;
            }

            if lang != self.current_translation_lang() {
                let still_loading = self
                    .translation_inflight
                    .iter()
                    .any(|((inflight_mod_id, _, _), _)| *inflight_mod_id == mod_id);
                self.set_translation_loading_for_gamebanana_mod(mod_id, still_loading);
                continue;
            }

            let current_source_hash = self.known_translation_source_hash_for_gamebanana_mod(mod_id);
            if source_hash != current_source_hash {
                let still_loading = self
                    .translation_inflight
                    .iter()
                    .any(|((inflight_mod_id, _, _), _)| *inflight_mod_id == mod_id);
                self.set_translation_loading_for_gamebanana_mod(mod_id, still_loading);
                continue;
            }

            match result {
                Ok(profile) => {
                    self.apply_translated_profile(mod_id, &lang, &source_hash, profile);
                }
                Err(err) => {
                    self.set_translation_loading_for_gamebanana_mod(mod_id, false);

                    // Show error toast
                    let text = self.text();
                    self.set_message_ok(text.translation_failed());
                    self.log_error(&format!("Translation failed for mod {mod_id}: {err}"));
                }
            }
        }
    }

    fn known_translation_mod_ids(&self) -> HashSet<u64> {
        let mut mod_ids: HashSet<u64> = self.browse_state.details.keys().copied().collect();
        mod_ids.extend(self.state.mods.iter().filter_map(|mod_entry| {
            mod_entry
                .source
                .as_ref()
                .and_then(|source| source.gamebanana.as_ref())
                .map(|link| link.mod_id)
        }));
        mod_ids
    }

    pub(crate) fn clear_translation_caches(&mut self) {
        // Clear in-memory state
        for detail in self.browse_state.details.values_mut() {
            detail.translated_profile = None;
            detail.translation_lang = None;
            detail.translation_loading = false;
        }
        self.my_mods_translation_state.clear();
        self.cancelled_translation_requests
            .extend(self.translation_inflight.values().copied());
        self.cancelled_translation_requests
            .extend(self.unlinked_translation_inflight.values().copied());
        self.translation_inflight.clear();
        self.unlinked_translation_inflight.clear();
        for cancellation in self.unlinked_translation_cancellations.values() {
            cancellation.store(true, Ordering::Relaxed);
        }
        self.unlinked_translation_cancellations.clear();

        let index_path = Self::translation_cache_index_path();
        if let Ok(raw) = std::fs::read_to_string(&index_path) {
            if let Ok(cache_keys) = serde_json::from_str::<Vec<String>>(&raw) {
                for cache_key in cache_keys {
                    let _ = std::fs::remove_file(persistence::cache_file_path(&cache_key));
                }
            }
            let _ = std::fs::remove_file(&index_path);
        }

        let langs = ["en", "id", "cn", "ru"];
        for mod_id in self.known_translation_mod_ids() {
            for lang in langs {
                let _ = std::fs::remove_file(persistence::cache_file_path(
                    &Self::legacy_translation_cache_key(mod_id, lang),
                ));
                let source_hash = self.known_translation_source_hash_for_gamebanana_mod(mod_id);
                let _ = std::fs::remove_file(persistence::cache_file_path(
                    &Self::translation_cache_key(mod_id, lang, &source_hash),
                ));
            }
        }
    }
}
