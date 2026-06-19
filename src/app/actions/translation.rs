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
                .or_insert_with(|| MyModTranslationState {
                    translated_profile: None,
                    translation_lang: None,
                    translation_source_hash: None,
                    translation_loading: false,
                });
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
                .or_insert_with(|| MyModTranslationState {
                    translated_profile: None,
                    translation_lang: None,
                    translation_source_hash: None,
                    translation_loading: false,
                });
            state.translation_loading = loading;
        }
    }

    fn request_translation_for_gamebanana_mod(&mut self, mod_id: u64, lang: &str, force_refresh: bool) {
        let source_hash = self.known_translation_source_hash_for_gamebanana_mod(mod_id);
        let inflight_key = (mod_id, lang.to_string(), source_hash.clone());
        if self.translation_inflight.contains(&inflight_key) {
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
        self.translation_inflight.insert(inflight_key);
        let _ = self.translation_request_tx.send(TranslationRequest {
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
        self.request_translation_for_mod_entry(mod_entry_id, false);
    }

    pub(crate) fn toggle_browse_translation(&mut self, mod_id: u64) {
        let Some(detail) = self.browse_state.details.get_mut(&mod_id) else {
            return;
        };

        // If translation is loading, show toast and return
        if detail.translation_loading {
            let text = self.text();
            self.set_message_ok(text.translation_in_progress());
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
        let translation_state = self.my_mods_translation_state.entry(mod_id.clone()).or_insert_with(|| MyModTranslationState {
            translated_profile: None,
            translation_lang: None,
            translation_source_hash: None,
            translation_loading: false,
        });

        // If translation is loading, show toast and return
        if translation_state.translation_loading {
            let text = self.text();
            self.set_message_ok(text.translation_in_progress());
            return;
        }

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
        self.request_translation_for_mod_entry(&mod_id, true);
    }

    pub(crate) fn handle_translation_events(&mut self) {
        while let Ok(event) = self.translation_event_rx.try_recv() {
            self.translation_inflight.remove(&(
                event.mod_id,
                event.lang.clone(),
                event.source_hash.clone(),
            ));

            if event.lang != self.current_translation_lang() {
                self.set_translation_loading_for_gamebanana_mod(event.mod_id, false);
                continue;
            }

            let current_source_hash = self.known_translation_source_hash_for_gamebanana_mod(event.mod_id);
            if event.source_hash != current_source_hash {
                self.set_translation_loading_for_gamebanana_mod(event.mod_id, false);
                continue;
            }

            match event.result {
                Ok(profile) => {
                    self.apply_translated_profile(
                        event.mod_id,
                        &event.lang,
                        &event.source_hash,
                        profile,
                    );
                }
                Err(err) => {
                    self.set_translation_loading_for_gamebanana_mod(event.mod_id, false);

                    // Show error toast
                    let text = self.text();
                    self.set_message_ok(text.translation_failed());
                    self.log_error(&format!("Translation failed for mod {}: {}", event.mod_id, err));
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
        self.translation_inflight.clear();

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
