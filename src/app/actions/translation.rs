impl HestiaApp {
    fn current_translation_lang(&self) -> &'static str {
        match self.state.static_prefs.language {
            AppLanguage::English => "en",
            AppLanguage::Indonesian => "id",
            AppLanguage::ChineseSimplified => "cn",
            AppLanguage::Russian => "ru",
        }
    }

    fn translation_cache_path(mod_id: u64, lang: &str) -> std::path::PathBuf {
        persistence::cache_file_path(&format!("gb_profile_{}-{}.json", mod_id, lang))
    }

    fn cached_translation_profile(
        mod_id: u64,
        lang: &str,
    ) -> Option<gamebanana::ProfileResponse> {
        let cache_path = Self::translation_cache_path(mod_id, lang);
        std::fs::read_to_string(cache_path)
            .ok()
            .and_then(|json| serde_json::from_str::<gamebanana::ProfileResponse>(&json).ok())
    }

    fn apply_translated_profile(
        &mut self,
        mod_id: u64,
        lang: &str,
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
                    translation_loading: false,
                });
            state.translated_profile = Some(profile.clone());
            state.translation_lang = Some(lang.to_string());
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
                    translation_loading: false,
                });
            state.translation_loading = loading;
        }
    }

    fn request_translation_for_gamebanana_mod(&mut self, mod_id: u64, lang: &str) {
        if self.translation_inflight.contains(&(mod_id, lang.to_string())) {
            self.set_translation_loading_for_gamebanana_mod(mod_id, true);
            return;
        }

        if let Some(profile) = Self::cached_translation_profile(mod_id, lang) {
            self.apply_translated_profile(mod_id, lang, profile);
            return;
        }

        self.set_translation_loading_for_gamebanana_mod(mod_id, true);
        self.translation_inflight.insert((mod_id, lang.to_string()));
        let _ = self.translation_request_tx.send(TranslationRequest {
            mod_id,
            lang: lang.to_string(),
        });
    }

    pub(crate) fn maybe_translate_gamebanana_mod_details(&mut self, mod_id: u64) {
        if !self.state.static_prefs.always_translate_mod_details {
            return;
        }
        if mod_id == 0 {
            return;
        }
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(mod_id, lang);
    }

    pub(crate) fn maybe_translate_my_mod_details(&mut self, mod_entry_id: &str) {
        if !self.state.static_prefs.always_translate_mod_details {
            return;
        }
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
        self.request_translation_for_gamebanana_mod(gb_id, lang);
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
        self.request_translation_for_gamebanana_mod(mod_id, lang);
    }

    pub(crate) fn toggle_my_mods_translation(&mut self, mod_id: String) {
        let translation_state = self.my_mods_translation_state.entry(mod_id.clone()).or_insert_with(|| MyModTranslationState {
            translated_profile: None,
            translation_lang: None,
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

        // Get the GameBanana mod ID
        let gb_id = self.state.mods.iter()
            .find(|m| m.id == mod_id)
            .and_then(|m| m.source.as_ref())
            .and_then(|s| s.gamebanana.as_ref())
            .map(|l| l.mod_id);
        
        let Some(gb_id) = gb_id else {
            return;
        };

        // Start translation
        let lang = self.current_translation_lang();
        self.request_translation_for_gamebanana_mod(gb_id, lang);
    }

    pub(crate) fn handle_translation_events(&mut self) {
        while let Ok(event) = self.translation_event_rx.try_recv() {
            self.translation_inflight.remove(&(event.mod_id, event.lang.clone()));

            match event.result {
                Ok(profile) => {
                    self.apply_translated_profile(event.mod_id, &event.lang, profile);
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

    pub(crate) fn clear_translation_caches(&mut self) {
        // Clear in-memory state
        for detail in self.browse_state.details.values_mut() {
            detail.translated_profile = None;
            detail.translation_lang = None;
            detail.translation_loading = false;
        }
        self.my_mods_translation_state.clear();
        self.translation_inflight.clear();

        // Delete cached translation files
        let cache_dir = persistence::cache_file_path("");
        if let Some(parent) = cache_dir.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if let Ok(file_name) = entry.file_name().into_string() {
                        if file_name.starts_with("gb_profile_") && 
                           (file_name.ends_with("-en.json") || 
                            file_name.ends_with("-id.json") || 
                            file_name.ends_with("-cn.json")) {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }
}
