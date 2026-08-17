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
        let values = self
            .game_for_mod(entry)
            .and_then(|game| {
                xxmi_persist::read_mod_variables(
                    &game,
                    self.state.static_prefs.use_default_mods_path,
                    entry,
                )
                .map_err(|err| {
                    self.report_warn(format!("hotkey values could not be read: {err:#}"), None);
                })
                .ok()
            })
            .unwrap_or_default();
        self.mod_hotkey_values_cache
            .insert(entry.id.clone(), (entry.ini_hash.clone(), values));
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

    fn set_hotkey_value(&mut self, mod_id: &str, ini_rel_path: &str, var_name: &str, value: &str) {
        let Some(entry) = self.state.mods.iter().find(|entry| entry.id == mod_id).cloned() else {
            return;
        };
        let Some(game) = self.game_for_mod(&entry) else {
            self.report_warn("hotkey value could not be changed: game was not found", None);
            return;
        };
        if !game.is_xxmi() {
            self.report_warn("hotkey value could not be changed: game is not XXMI-backed", None);
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
        match xxmi_persist::set_mod_variable(&game, use_default, &entry, ini_rel_path, var_name, value)
        {
            Ok(wrote) => {
                self.refresh_hotkey_values_cache_for_entry(&entry);
                let status = if wrote { "wrote" } else { "unchanged" };
                self.push_log(format!(
                    "XXMI mod customization ({} / {}): {var_name} = {value} ({status})",
                    game.definition.id, entry.folder_name
                ));
                if entry.status == ModStatus::Active
                    && wrote
                    && self.xxmi_reload_enabled_for_game(&game, ReloadHotkeyTrigger::CustomizingMods)
                {
                    if let Some(other_game) = self.other_running_xxmi_game(&game) {
                        self.push_log(format!(
                            "XXMI mod customization ({} / {}): skipped reload because {other_game} is also running",
                            game.definition.id, entry.folder_name
                        ));
                    } else {
                        self.send_xxmi_reload_hotkey_if_supported(&game);
                    }
                }
            }
            Err(err) => self.report_error(err, None),
        }
    }

    fn run_hotkey_command(&mut self, mod_id: &str, key_spec: &str, label: &str) {
        let Some(entry) = self.state.mods.iter().find(|entry| entry.id == mod_id).cloned() else {
            return;
        };
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
        match xxmi_persist::send_mod_hotkey_foreground_aware(
            &game,
            self.state.static_prefs.use_default_mods_path,
            key_spec,
        ) {
            Ok(report) => {
                let message = report.message;
                if message.starts_with("sent ") {
                    self.push_log(format!("XXMI mod hotkey ({}): {message}", game.definition.id));
                    self.set_message_ok(format!("Triggered: {label}"));
                } else {
                    self.report_warn(
                        format!("XXMI mod hotkey ({}): {message}", game.definition.id),
                        Some("Could not trigger hotkey"),
                    );
                }
            }
            Err(err) => self.report_error(err, Some("Could not trigger hotkey")),
        }
    }
}
