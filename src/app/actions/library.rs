impl HestiaApp {
    fn finish_single_mod_action(&mut self, result: Result<()>, name: &str, action: &str, error_toast: &str) {
        match result {
            Ok(()) => {
                self.log_action(action, name);
                self.set_message_ok(self.text().action_message(action, name));
                self.save_state();
                self.refresh();
            }
            Err(err) => self.report_error(err, Some(error_toast)),
        }
    }

    fn finish_batch_mod_action(&mut self, count: usize, action: &str) {
        if count > 0 {
            let text = self.text();
            self.log_action(action, &text.library_mods_count(count));
            self.set_message_ok(text.action_count_message(action, count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn game_for_mod(&self, mod_entry: &ModEntry) -> Option<GameInstall> {
        self.state
            .games
            .iter()
            .find(|game| game.definition.id == mod_entry.game_id)
            .cloned()
    }

    fn disable_mod_by_id(&mut self, mod_id: &str) {
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default = self.state.static_prefs.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                let result = match game.as_ref().map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => xxmi::disable_mod(mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.as_ref().expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                (Some(result), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_disabled(), text.disable_failed());
        }
    }

    fn enable_or_restore_mod_by_id(&mut self, mod_id: &str) {
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let text = self.text();
        let (result, name, action) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => {
                    let result = match game.as_ref().map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => xxmi::enable_mod(mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.as_ref().expect("game checked"), use_default_path)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    (Some(result), Some(name), Some(text.action_enabled()))
                }
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        if game.is_xxmi() {
                            xxmi::restore_mod(mod_entry, game, use_default_path)?;
                        } else {
                            bail!("archive is not supported for Unreal Engine games");
                        }
                        Ok(())
                    })();
                    (Some(result), Some(name), Some(text.action_unarchived()))
                }
                _ => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        if let (Some(result), Some(name), Some(action)) = (result, name, action) {
            match result {
                Ok(()) => {
                    self.log_action(action, &name);
                    self.set_message_ok(text.action_message(action, &name));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == text.action_enabled() {
                        text.enable_failed()
                    } else {
                        text.restore_failed()
                    };
                    self.report_error(err, Some(toast));
                }
            }
        }
    }

    fn archive_mod_by_id(&mut self, mod_id: &str) {
        if let Some(snapshot) = self.state.mods.iter().find(|m| m.id == mod_id).cloned() {
            self.clear_mod_image_runtime_state(&snapshot);
        }
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                if game.is_xxmi() {
                    xxmi::archive_mod(mod_entry, game, use_default_path)?;
                } else {
                    bail!("archive is not supported for Unreal Engine games");
                }
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_archived(), text.archive_failed());
        }
    }

    fn delete_mod_by_id(&mut self, mod_id: &str) {
        let result = (|| -> Result<()> {
            let mod_entry = self
                .state
                .mods
                .iter()
                .find(|m| m.id == mod_id)
                .cloned()
                .ok_or_else(|| anyhow!("no mod selected"))?;
            let behavior = self.delete_mod_entry(&mod_entry)?;
            let text = self.text();
            let action = text.delete_action(behavior);
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(text.action_message(action, &mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some(self.text().delete_failed()));
        }
    }

    fn delete_mod_entry(&mut self, mod_entry: &ModEntry) -> Result<DeleteBehavior> {
        self.clear_mod_image_runtime_state(mod_entry);
        match self.state.static_prefs.delete_behavior {
            DeleteBehavior::RecycleBin => {
                xxmi::send_to_recycle_bin(mod_entry)?;
                Ok(DeleteBehavior::RecycleBin)
            }
            DeleteBehavior::Permanent => {
                if mod_entry.root_path.is_dir() {
                    fs::remove_dir_all(&mod_entry.root_path)?;
                } else if mod_entry.root_path.is_file() {
                    fs::remove_file(&mod_entry.root_path)?;
                }
                Ok(DeleteBehavior::Permanent)
            }
        }
    }

    fn delete_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_delete_selected();
            return;
        }

        let result = (|| -> Result<()> {
            let mod_entry = self.selected_mod().cloned().ok_or_else(|| anyhow!("no mod selected"))?;
            let behavior = self.delete_mod_entry(&mod_entry)?;
            let text = self.text();
            let action = text.delete_action(behavior);
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(text.action_message(action, &mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some(self.text().delete_failed()));
        }
    }

    fn disable_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_disable_selected();
            return;
        }

        let text = self.text();
        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default = self.state.static_prefs.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                let result = match game.as_ref().map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => xxmi::disable_mod(mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.as_ref().expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                (Some(result), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            self.finish_single_mod_action(result, &name, text.action_disabled(), text.disable_failed());
        }
    }

    fn enable_or_restore_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_enable_selected();
            return;
        }

        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let text = self.text();
        let (result, name, action) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => {
                    let result = match game.as_ref().map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => xxmi::enable_mod(mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.as_ref().expect("game checked"), use_default_path)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    (Some(result), Some(name), Some(text.action_enabled()))
                }
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        if game.is_xxmi() {
                            xxmi::restore_mod(mod_entry, game, use_default_path)?;
                        } else {
                            bail!("archive is not supported for Unreal Engine games");
                        }
                        Ok(())
                    })();
                    (Some(result), Some(name), Some(text.action_unarchived()))
                }
                _ => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        if let (Some(result), Some(name), Some(action)) = (result, name, action) {
            match result {
                Ok(()) => {
                    self.log_action(action, &name);
                    self.set_message_ok(text.action_message(action, &name));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == text.action_enabled() {
                        text.enable_failed()
                    } else {
                        text.restore_failed()
                    };
                    self.report_error(err, Some(toast));
                }
            }
        }
    }

    fn archive_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_archive_selected();
            return;
        }

        if let Some(snapshot) = self.selected_mod().cloned() {
            self.clear_mod_image_runtime_state(&snapshot);
        }
        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                if game.is_xxmi() {
                    xxmi::archive_mod(mod_entry, game, use_default_path)?;
                } else {
                    bail!("archive is not supported for Unreal Engine games");
                }
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_archived(), text.archive_failed());
        }
    }

    fn batch_update_selected(&mut self) {
        // Single iteration: collect IDs in one pass
        let update_ids: Vec<String> = self.state.mods.iter()
            .filter(|m| {
                self.selected_mods.contains(&m.id)
                    && (matches!(m.update_state, ModUpdateState::UpdateAvailable)
                        || (self.state.static_prefs.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                            && Self::has_modified_update_available(m)))
            })
            .map(|m| m.id.clone())
            .collect();

        let count = update_ids.len();
        for id in &update_ids {
            self.queue_update_apply(id);
        }

        if count > 0 {
            self.set_message_ok(self.text().queued_updates(count));
            self.selected_mods.clear();
        }
    }

    fn batch_disable_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        let mut disabled_count = 0;
        // Single iteration: filter selected mods and disable in one pass
        for mod_entry in self.state.mods.iter_mut() {
            if self.selected_mods.contains(&mod_entry.id) && mod_entry.status == ModStatus::Active {
                let game = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id);
                let result = match game.map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => xxmi::disable_mod(mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                if result.is_ok() {
                    disabled_count += 1;
                }
            }
        }
        let action = self.text().action_disabled();
        self.finish_batch_mod_action(disabled_count, action);
    }

    fn batch_enable_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        let mut enabled_count = 0;
        let mut unarchived_count = 0;
        // Single iteration: process all selected mods in one pass
        for mod_entry in self.state.mods.iter_mut() {
            if self.selected_mods.contains(&mod_entry.id) {
                let game = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id);
                if mod_entry.status == ModStatus::Disabled {
                    let result = match game.map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => xxmi::enable_mod(mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.expect("game checked"), use_default)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    if result.is_ok() {
                        enabled_count += 1;
                    }
                } else if mod_entry.status == ModStatus::Archived {
                    if let Some(game_ref) = game {
                        if game_ref.is_xxmi()
                            && xxmi::restore_mod(mod_entry, game_ref, use_default).is_ok()
                        {
                            unarchived_count += 1;
                        }
                    }
                }
            }
        }
        if enabled_count > 0 {
            let text = self.text();
            let action = text.action_enabled();
            self.log_action(action, &text.library_mods_count(enabled_count));
            self.set_message_ok(text.action_count_message(action, enabled_count));
        }
        if unarchived_count > 0 {
            let text = self.text();
            let action = text.action_unarchived();
            self.log_action(action, &text.library_mods_count(unarchived_count));
            self.set_message_ok(text.action_count_message(action, unarchived_count));
        }
        if enabled_count + unarchived_count > 0 {
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn rename_mod_folder(&mut self, mod_id: &str, new_name: &str) -> Result<()> {
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) else {
            bail!("mod not found");
        };
        if mod_entry.folder_name == new_name { return Ok(()); }
        let old_path = mod_entry.root_path.clone();
        let new_path = old_path.parent().ok_or_else(|| anyhow!("invalid path"))?.join(new_name);
        if new_path.exists() {
            bail!("destination folder already exists: {}", new_name);
        }
        fs::rename(&old_path, &new_path)?;
        mod_entry.root_path = new_path;
        mod_entry.folder_name = new_name.to_string();
        mod_entry.updated_at = Utc::now();
        let game = game.as_ref().ok_or_else(|| anyhow!("game not found"))?;
        match game.definition.backend {
            GameBackend::Xxmi => xxmi::save_mod_metadata(mod_entry)?,
            GameBackend::UnrealEngine => unrealengine::write_portable_metadata(mod_entry)?,
        }
        Ok(())
    }

    fn perform_mod_rename(&mut self, mod_id: String) {
        let raw = self.mod_detail_edit_name.trim().to_string();
        if raw.is_empty() {
            self.clear_mod_detail_rename();
            return;
        }
        let sanitized = sanitize_folder_name(&raw);
        if sanitized == self.text().imported_mod() || sanitized.chars().all(|c| c == '_') {
            self.clear_mod_detail_rename();
            return;
        }
        if let Err(err) = self.rename_mod_folder(&mod_id, &sanitized) {
            self.report_error(err, Some(self.text().rename_failed()));
        } else {
            let text = self.text();
            self.set_message_ok(text.renamed_to(&sanitized));
            self.log_action(text.action_renamed(), &sanitized);
        }
        self.clear_mod_detail_rename();
        self.refresh();
    }

    fn batch_archive_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        // Collect mod entries to clear image state (need owned data to avoid borrow conflicts)
        let mods_to_clear: Vec<ModEntry> = self.state.mods
            .iter()
            .filter(|m| self.selected_mods.contains(&m.id) 
                && matches!(m.status, ModStatus::Active | ModStatus::Disabled)
                && games
                    .iter()
                    .find(|game| game.definition.id == m.game_id)
                    .is_some_and(|game| game.is_xxmi()))
            .cloned()
            .collect();
        
        // Clear image states
        for mod_entry in &mods_to_clear {
            self.clear_mod_image_runtime_state(mod_entry);
        }
        
        // Archive mods in a single iteration
        let mut archived_count = 0;
        for mod_entry in self.state.mods.iter_mut() {
            if mods_to_clear.iter().any(|m| m.id == mod_entry.id) {
                if let Some(game_ref) = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id)
                {
                    if game_ref.is_xxmi()
                        && xxmi::archive_mod(mod_entry, game_ref, use_default).is_ok()
                    {
                        archived_count += 1;
                    }
                }
            }
        }
        let action = self.text().action_archived();
        self.finish_batch_mod_action(archived_count, action);
    }

    fn batch_delete_selected(&mut self) {
        // Single iteration: collect selected mods to delete in one pass
        let mods_to_delete: Vec<ModEntry> = self.state.mods
            .iter()
            .filter(|m| self.selected_mods.contains(&m.id))
            .cloned()
            .collect();
        let mut deleted_count = 0;
        let mut last_err: Option<anyhow::Error> = None;
        for mod_entry in mods_to_delete {
            match self.delete_mod_entry(&mod_entry) {
                Ok(_) => deleted_count += 1,
                Err(err) => last_err = Some(err),
            }
        }
        if deleted_count > 0 {
            let text = self.text();
            let action = text.delete_action(self.state.static_prefs.delete_behavior);
            self.log_action(action, &text.library_mods_count(deleted_count));
            self.set_message_ok(text.action_count_message(action, deleted_count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
        if let Some(err) = last_err {
            self.report_error(err, Some(self.text().delete_failed()));
        }
    }

    fn toggle_mod_selection(&mut self, mod_id: &str, checked: bool) {
        if checked {
            self.selected_mods.insert(mod_id.to_string());
        } else {
            self.selected_mods.remove(mod_id);
        }
    }
}
