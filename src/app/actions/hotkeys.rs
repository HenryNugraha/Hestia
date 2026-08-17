impl HestiaApp {
    fn other_running_xxmi_game(&self, game: &GameInstall) -> Option<String> {
        self.state
            .games
            .iter()
            .filter(|candidate| candidate.definition.id != game.definition.id)
            .filter(|candidate| candidate.is_xxmi())
            .find(|candidate| self.game_process_running(candidate))
            .map(|candidate| candidate.definition.name.clone())
    }

    fn refresh_hotkey_values_cache_for_entry(&mut self, entry: &ModEntry) {
        if !self.mod_hotkey_values_loading.insert(entry.id.clone()) {
            return;
        }
        let Some(game) = self.game_for_mod(entry) else {
            self.mod_hotkey_values_loading.remove(&entry.id);
            self.mod_hotkey_values_cache
                .insert(entry.id.clone(), (entry.ini_hash.clone(), HashMap::new()));
            return;
        };
        let request = HotkeyCustomizationRequest::LoadValues {
            game,
            use_default: self.state.static_prefs.use_default_mods_path,
            entry: entry.clone(),
        };
        if self.hotkey_customization_tx.send(request).is_err() {
            self.mod_hotkey_values_loading.remove(&entry.id);
            self.push_log("hotkey values could not be read: worker is unavailable".to_string());
        }
    }

    fn ensure_hotkey_values_cached(&mut self, entry: &ModEntry) {
        let stale = self
            .mod_hotkey_values_cache
            .get(&entry.id)
            .is_none_or(|(hash, _)| *hash != entry.ini_hash);
        if stale {
            self.refresh_hotkey_values_cache_for_entry(entry);
        }
    }

    fn cached_hotkey_values(&self, mod_id: &str) -> HashMap<String, String> {
        self.mod_hotkey_values_cache
            .get(mod_id)
            .map(|(_, values)| values.clone())
            .unwrap_or_default()
    }

    fn consume_hotkey_customization_events(&mut self) {
        while let Ok(event) = self.hotkey_customization_rx.try_recv() {
            match event {
                HotkeyCustomizationEvent::ValuesLoaded {
                    mod_id,
                    ini_hash,
                    values,
                } => {
                    if self.mod_hotkey_values_loading.remove(&mod_id) {
                        self.mod_hotkey_values_cache.insert(mod_id, (ini_hash, values));
                    }
                }
                HotkeyCustomizationEvent::ValueFinished {
                    game_id,
                    folder_name,
                    var_name,
                    value,
                    status,
                } => {
                    self.push_log(format!(
                        "XXMI mod customization ({game_id} / {folder_name}): {var_name} = {value} ({status})"
                    ));
                }
                HotkeyCustomizationEvent::ClearFinished {
                    mod_id,
                    game_id,
                    folder_name,
                    status,
                    message,
                } => {
                    self.hotkey_clear_inflight.remove(&mod_id);
                    self.mod_hotkey_values_loading.remove(&mod_id);
                    self.mod_hotkey_values_cache
                        .insert(mod_id, (None, HashMap::new()));
                    self.push_log(format!(
                        "XXMI mod customization ({game_id} / {folder_name}): {status}; {message}"
                    ));
                }
                HotkeyCustomizationEvent::CommandFinished {
                    game_id,
                    label,
                    message,
                } => {
                    if message.starts_with("sent ") {
                        self.push_log(format!("XXMI mod hotkey ({game_id}): {message}"));
                        self.set_message_ok(format!("Triggered: {label}"));
                    } else {
                        self.push_log(format!("XXMI mod hotkey ({game_id}): {message}"));
                    }
                }
                HotkeyCustomizationEvent::Failed { mod_id, message } => {
                    if let Some(mod_id) = mod_id {
                        self.hotkey_clear_inflight.remove(&mod_id);
                        self.mod_hotkey_values_loading.remove(&mod_id);
                    }
                    self.push_log(message);
                }
            }
        }
    }

    fn action_hotkey_rel_var_key(ini_rel_path: &str, var_name: &str) -> Option<String> {
        let rel_path = ini_rel_path
            .trim()
            .trim_matches(['/', '\\'])
            .replace('/', "\\");
        if rel_path.is_empty() {
            return None;
        }
        Some(format!("{}\\{var_name}", rel_path.to_ascii_lowercase()))
    }

    fn action_hotkey_current_value<'a>(
        values: &'a HashMap<String, String>,
        ini_rel_path: &str,
        var_name: &str,
    ) -> Option<&'a str> {
        if let Some((_, value)) = values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(var_name))
        {
            return Some(value.as_str());
        }
        if let Some(rel_key) = Self::action_hotkey_rel_var_key(ini_rel_path, var_name)
            && let Some((_, value)) = values
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(&rel_key))
        {
            return Some(value.as_str());
        }
        let suffix = format!("\\{var_name}");
        let mut matches = values
            .iter()
            .filter(|(key, _)| key.to_lowercase().ends_with(&suffix.to_lowercase()));
        let first = matches.next()?;
        matches.next().is_none().then_some(first.1.as_str())
    }

    fn action_hotkey_cycle_steps(
        cycle_values: &[String],
        current: Option<&str>,
        target: &str,
    ) -> Option<usize> {
        if cycle_values.is_empty() {
            return None;
        }
        let target_index = cycle_values
            .iter()
            .position(|value| value.trim().eq_ignore_ascii_case(target.trim()))?;
        let current_index = current
            .and_then(|current| {
                cycle_values
                    .iter()
                    .position(|value| value.trim().eq_ignore_ascii_case(current.trim()))
            })
            .unwrap_or(0);
        Some((target_index + cycle_values.len() - current_index) % cycle_values.len())
    }

    fn remember_hotkey_value(
        &mut self,
        entry: &ModEntry,
        ini_rel_path: &str,
        var_name: &str,
        value: &str,
    ) {
        self.mod_hotkey_values_loading.remove(&entry.id);
        let (hash, values) = self
            .mod_hotkey_values_cache
            .entry(entry.id.clone())
            .or_insert_with(|| (entry.ini_hash.clone(), HashMap::new()));
        *hash = entry.ini_hash.clone();
        let replacement_key = values
            .keys()
            .find(|key| key.eq_ignore_ascii_case(var_name))
            .cloned()
            .or_else(|| {
                Self::action_hotkey_rel_var_key(ini_rel_path, var_name).and_then(|rel_key| {
                    values
                        .keys()
                        .find(|key| key.eq_ignore_ascii_case(&rel_key))
                        .cloned()
                })
            })
            .or_else(|| {
                let suffix = format!("\\{var_name}").to_lowercase();
                let mut matches = values
                    .keys()
                    .filter(|key| key.to_lowercase().ends_with(&suffix));
                let first = matches.next()?.clone();
                matches.next().is_none().then_some(first)
            })
            .or_else(|| Self::action_hotkey_rel_var_key(ini_rel_path, var_name))
            .unwrap_or_else(|| var_name.to_string());
        values.insert(replacement_key, value.trim().to_string());
    }

    fn set_hotkey_value(
        &mut self,
        mod_id: &str,
        ini_rel_path: &str,
        var_name: &str,
        value: &str,
        key_spec: &str,
        cycle_values: &[String],
    ) {
        let Some(entry) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id)
            .cloned()
        else {
            return;
        };
        let Some(game) = self.game_for_mod(&entry) else {
            self.report_warn(
                "hotkey value could not be changed: game was not found",
                None,
            );
            return;
        };
        if !game.is_xxmi() {
            self.report_warn(
                "hotkey value could not be changed: game is not XXMI-backed",
                None,
            );
            return;
        }
        if !matches!(
            entry.status,
            ModStatus::Active | ModStatus::Disabled | ModStatus::Archived
        ) {
            self.report_warn(
                "hotkey value could not be changed: mod status is unsupported",
                None,
            );
            return;
        }
        let use_default = self.state.static_prefs.use_default_mods_path;
        if entry.status == ModStatus::Active && self.game_process_running(&game) {
            if key_spec.trim().is_empty() {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live change for {var_name} because the mod does not define a keyboard hotkey",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            if let Some(other_game) = self.other_running_xxmi_game(&game) {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live change because {other_game} is also running",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            let current_values = self.cached_hotkey_values(&entry.id);
            let current =
                Self::action_hotkey_current_value(&current_values, ini_rel_path, var_name);
            if let Some(steps) = Self::action_hotkey_cycle_steps(cycle_values, current, value) {
                self.remember_hotkey_value(&entry, ini_rel_path, var_name, value);
                let request = HotkeyCustomizationRequest::SetValue {
                    game,
                    use_default,
                    entry,
                    ini_rel_path: ini_rel_path.to_string(),
                    var_name: var_name.to_string(),
                    value: value.to_string(),
                    key_spec: key_spec.to_string(),
                    steps: Some(steps),
                    reload_after_write: false,
                };
                if self.hotkey_customization_tx.send(request).is_err() {
                    self.push_log("XXMI mod customization: live change failed because the worker is unavailable".to_string());
                }
                return;
            }
            self.push_log(format!(
                "XXMI mod customization ({} / {}): skipped live change for {var_name} because the target value is not in the cycle",
                game.definition.id, entry.folder_name
            ));
            return;
        }
        self.remember_hotkey_value(&entry, ini_rel_path, var_name, value);
        let request = HotkeyCustomizationRequest::SetValue {
            game,
            use_default,
            entry,
            ini_rel_path: ini_rel_path.to_string(),
            var_name: var_name.to_string(),
            value: value.to_string(),
            key_spec: key_spec.to_string(),
            steps: None,
            reload_after_write: false,
        };
        if self.hotkey_customization_tx.send(request).is_err() {
            self.push_log("XXMI mod customization: value change failed because the worker is unavailable".to_string());
        }
    }

    fn clear_hotkey_customization(&mut self, mod_id: &str) {
        let Some(entry) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id)
            .cloned()
        else {
            return;
        };
        let Some(game) = self.game_for_mod(&entry) else {
            self.report_warn(
                "hotkey customization could not be cleared: game was not found",
                None,
            );
            return;
        };
        if !game.is_xxmi() {
            self.report_warn(
                "hotkey customization could not be cleared: game is not XXMI-backed",
                None,
            );
            return;
        }
        if !matches!(
            entry.status,
            ModStatus::Active | ModStatus::Disabled | ModStatus::Archived
        ) {
            self.report_warn(
                "hotkey customization could not be cleared: mod status is unsupported",
                None,
            );
            return;
        }
        let use_default = self.state.static_prefs.use_default_mods_path;
        if entry.status == ModStatus::Active && self.game_process_running(&game) {
            if !self.xxmi_reload_enabled_for_game(&game, ReloadHotkeyTrigger::CustomizingMods) {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live clear because the Customizing mods reload trigger is disabled",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            if self.xxmi_reload_inflight.contains(&game.definition.id) {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live clear because an XXMI reload is already in progress",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            if let Some(other_game) = self.other_running_xxmi_game(&game) {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live clear because {other_game} is also running",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            if !self.hotkey_clear_inflight.insert(entry.id.clone()) {
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): skipped live clear because a clear is already in progress",
                    game.definition.id, entry.folder_name
                ));
                return;
            }
            self.mod_hotkey_values_loading.remove(&entry.id);
            self.mod_hotkey_values_cache
                .insert(entry.id.clone(), (entry.ini_hash.clone(), HashMap::new()));
            let request = HotkeyCustomizationRequest::Clear {
                game,
                use_default,
                entry,
                live: true,
                reload_after_clear: false,
            };
            if self.hotkey_customization_tx.send(request).is_err() {
                self.hotkey_clear_inflight.remove(mod_id);
                self.push_log("XXMI mod customization: live clear failed because the worker is unavailable".to_string());
            }
            return;
        }
        self.mod_hotkey_values_loading.remove(&entry.id);
        self.mod_hotkey_values_cache
            .insert(entry.id.clone(), (entry.ini_hash.clone(), HashMap::new()));
        let request = HotkeyCustomizationRequest::Clear {
            game,
            use_default,
            entry,
            live: false,
            reload_after_clear: false,
        };
        if self.hotkey_customization_tx.send(request).is_err() {
            self.push_log("XXMI mod customization: clear failed because the worker is unavailable".to_string());
        }
    }

    fn run_hotkey_command(&mut self, mod_id: &str, key_spec: &str, label: &str) {
        let Some(entry) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id)
            .cloned()
        else {
            return;
        };
        if key_spec.trim().is_empty() {
            self.push_log(format!(
                "XXMI mod hotkey ({}): skipped {label} because the mod does not define a keyboard hotkey",
                entry.folder_name
            ));
            return;
        }
        if entry.status != ModStatus::Active {
            self.report_warn("hotkey could not be triggered: mod is not active", None);
            return;
        }
        let Some(game) = self.game_for_mod(&entry) else {
            self.report_warn("hotkey could not be triggered: game was not found", None);
            return;
        };
        if !self.game_process_running(&game) {
            self.report_warn("hotkey could not be triggered: game is not running", None);
            return;
        }
        if let Some(other_game) = self.other_running_xxmi_game(&game) {
            self.report_warn(
                format!(
                    "hotkey could not be triggered: {other_game} is also running, so the synthetic keypress would be ambiguous"
                ),
                None,
            );
            return;
        }
        let request = HotkeyCustomizationRequest::RunCommand {
            game,
            use_default: self.state.static_prefs.use_default_mods_path,
            mod_id: mod_id.to_string(),
            key_spec: key_spec.to_string(),
            label: label.to_string(),
        };
        if self.hotkey_customization_tx.send(request).is_err() {
            self.push_log("XXMI mod hotkey: trigger failed because the worker is unavailable".to_string());
        }
    }
}
