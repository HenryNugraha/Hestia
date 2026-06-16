impl HestiaApp {
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
        let lang = match self.state.static_prefs.language {
            AppLanguage::English => "en",
            AppLanguage::Indonesian => "id",
            AppLanguage::ChineseSimplified => "cn",
        };

        // Check if already in flight
        if self.translation_inflight.contains(&(mod_id, lang.to_string())) {
            return;
        }

        // Check cache first by attempting to load
        let cache_key = format!("gb_profile_{}-{}.json", mod_id, lang);
        let cache_path = persistence::cache_file_path(&cache_key);
        
        if cache_path.exists() {
            if let Ok(json) = std::fs::read_to_string(&cache_path) {
                if let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(&json) {
                    // Cache hit - use it immediately and regenerate markdown
                    if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                        let markdown = prepare_markdown_for_display(
                            profile.html_text.as_deref().unwrap_or_default(),
                            None,
                            Some(mod_id),
                            &self.portable,
                        );
                        
                        detail.translated_profile = Some(profile);
                        detail.translation_lang = Some(lang.to_string());
                        detail.markdown = markdown;
                    }
                    return;
                }
            }
        }

        // Cache miss - request from API
        detail.translation_loading = true;
        self.translation_inflight.insert((mod_id, lang.to_string()));
        let _ = self.translation_request_tx.send(TranslationRequest {
            mod_id,
            lang: lang.to_string(),
        });
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
        let lang = match self.state.static_prefs.language {
            AppLanguage::English => "en",
            AppLanguage::Indonesian => "id",
            AppLanguage::ChineseSimplified => "cn",
        };

        // Check if already in flight
        if self.translation_inflight.contains(&(gb_id, lang.to_string())) {
            return;
        }

        // Check cache first
        let cache_key = format!("gb_profile_{}-{}.json", gb_id, lang);
        let cache_path = persistence::cache_file_path(&cache_key);
        
        if cache_path.exists() {
            if let Ok(json) = std::fs::read_to_string(&cache_path) {
                if let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(&json) {
                    // Cache hit - use it immediately
                    if let Some(state) = self.my_mods_translation_state.get_mut(&mod_id) {
                        state.translated_profile = Some(profile);
                        state.translation_lang = Some(lang.to_string());
                    }
                    return;
                }
            }
        }

        // Cache miss - request from API
        translation_state.translation_loading = true;
        self.translation_inflight.insert((gb_id, lang.to_string()));
        let _ = self.translation_request_tx.send(TranslationRequest {
            mod_id: gb_id,
            lang: lang.to_string(),
        });
    }

    pub(crate) fn handle_translation_events(&mut self) {
        while let Ok(event) = self.translation_event_rx.try_recv() {
            self.translation_inflight.remove(&(event.mod_id, event.lang.clone()));

            match event.result {
                Ok(profile) => {
                    // Update browse detail cache
                    if let Some(detail) = self.browse_state.details.get_mut(&event.mod_id) {
                        // Regenerate markdown from translated profile
                        let markdown = prepare_markdown_for_display(
                            profile.html_text.as_deref().unwrap_or_default(),
                            None,
                            Some(event.mod_id),
                            &self.portable,
                        );
                        
                        detail.translated_profile = Some(profile.clone());
                        detail.translation_lang = Some(event.lang.clone());
                        detail.translation_loading = false;
                        detail.markdown = markdown;
                    }

                    // Update my mods translation state
                    let mod_ids_to_update: Vec<String> = self.state.mods.iter()
                        .filter(|m| {
                            m.source.as_ref()
                                .and_then(|s| s.gamebanana.as_ref())
                                .map(|l| l.mod_id == event.mod_id)
                                .unwrap_or(false)
                        })
                        .map(|m| m.id.clone())
                        .collect();
                    
                    for mod_id in mod_ids_to_update {
                        if let Some(state) = self.my_mods_translation_state.get_mut(&mod_id) {
                            state.translated_profile = Some(profile.clone());
                            state.translation_lang = Some(event.lang.clone());
                            state.translation_loading = false;
                        }
                    }
                }
                Err(err) => {
                    // Clear loading state
                    if let Some(detail) = self.browse_state.details.get_mut(&event.mod_id) {
                        detail.translation_loading = false;
                    }

                    let mod_ids_to_update: Vec<String> = self.state.mods.iter()
                        .filter(|m| {
                            m.source.as_ref()
                                .and_then(|s| s.gamebanana.as_ref())
                                .map(|l| l.mod_id == event.mod_id)
                                .unwrap_or(false)
                        })
                        .map(|m| m.id.clone())
                        .collect();
                    
                    for mod_id in mod_ids_to_update {
                        if let Some(state) = self.my_mods_translation_state.get_mut(&mod_id) {
                            state.translation_loading = false;
                        }
                    }

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
