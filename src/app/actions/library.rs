impl HestiaApp {
    fn disable_mod_by_id(&mut self, mod_id: &str) {
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                (Some(xxmi::disable_mod(mod_entry)), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            match result {
                Ok(()) => {
                    self.log_action("Disabled", &name);
                    self.set_message_ok(format!("Disabled: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => self.report_error(err, Some("Disable failed")),
            }
        }
    }

    fn enable_or_restore_mod_by_id(&mut self, mod_id: &str) {
        let game = self.selected_game().cloned();
        let use_default_path = self.state.use_default_mods_path;
        let (result, name, action) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => (Some(xxmi::enable_mod(mod_entry)), Some(name), Some("Enabled")),
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        xxmi::restore_mod(mod_entry, game, use_default_path)?;
                        Ok(())
                    })();
                    (Some(result), Some(name), Some("Unarchived"))
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
                    self.set_message_ok(format!("{action}: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == "Enabled" {
                        "Enable failed"
                    } else {
                        "Restore failed"
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
        let game = self.selected_game().cloned();
        let use_default_path = self.state.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                xxmi::archive_mod(mod_entry, game, use_default_path)?;
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            match result {
                Ok(()) => {
                    self.log_action("Archived", &name);
                    self.set_message_ok(format!("Archived: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => self.report_error(err, Some("Archive failed")),
            }
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
            let action = self.delete_mod_entry(&mod_entry)?;
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(format!("{action}: {}", mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some("Delete failed"));
        }
    }

    fn delete_mod_entry(&mut self, mod_entry: &ModEntry) -> Result<&'static str> {
        self.clear_mod_image_runtime_state(mod_entry);
        match self.state.delete_behavior {
            DeleteBehavior::RecycleBin => {
                xxmi::send_to_recycle_bin(mod_entry)?;
                Ok("Recycled")
            }
            DeleteBehavior::Permanent => {
                if mod_entry.root_path.is_dir() {
                    fs::remove_dir_all(&mod_entry.root_path)?;
                } else if mod_entry.root_path.is_file() {
                    fs::remove_file(&mod_entry.root_path)?;
                }
                Ok("Deleted")
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
            let action = self.delete_mod_entry(&mod_entry)?;
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(format!("{action}: {}", mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some("Delete failed"));
        }
    }

    fn disable_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_disable_selected();
            return;
        }

        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                (Some(xxmi::disable_mod(mod_entry)), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            match result {
                Ok(()) => {
                    self.log_action("Disabled", &name);
                    self.set_message_ok(format!("Disabled: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => self.report_error(err, Some("Disable failed")),
            }
        }
    }

    fn enable_or_restore_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_enable_selected();
            return;
        }

        let game = self.selected_game().cloned();
        let use_default_path = self.state.use_default_mods_path;
        let (result, name, action) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => (Some(xxmi::enable_mod(mod_entry)), Some(name), Some("Enabled")),
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        xxmi::restore_mod(mod_entry, game, use_default_path)?;
                        Ok(())
                    })();
                    (Some(result), Some(name), Some("Unarchived"))
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
                    self.set_message_ok(format!("{action}: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == "Enabled" {
                        "Enable failed"
                    } else {
                        "Restore failed"
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
        let game = self.selected_game().cloned();
        let use_default_path = self.state.use_default_mods_path;
        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                xxmi::archive_mod(mod_entry, game, use_default_path)?;
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };

        if let (Some(result), Some(name)) = (result, name) {
            match result {
                Ok(()) => {
                    self.log_action("Archived", &name);
                    self.set_message_ok(format!("Archived: {name}"));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => self.report_error(err, Some("Archive failed")),
            }
        }
    }

    fn batch_update_selected(&mut self) {
        let update_ids: Vec<String> = self.state.mods.iter()
            .filter(|m| self.selected_mods.contains(&m.id) && matches!(m.update_state, ModUpdateState::UpdateAvailable))
            .map(|m| m.id.clone())
            .collect();

        let count = update_ids.len();
        for id in update_ids {
            self.queue_update_apply(&id);
        }

        if count > 0 {
            self.set_message_ok(format!("Queued updates for {count} mod(s)"));
            self.selected_mods.clear();
        }
    }

    fn batch_disable_selected(&mut self) {
        let mods_to_disable: Vec<String> = self.selected_mods.iter().cloned().collect();
        let mut disabled_count = 0;
        for mod_id in mods_to_disable {
            if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
                if mod_entry.status == ModStatus::Active {
                    if xxmi::disable_mod(mod_entry).is_ok() {
                        disabled_count += 1;
                    }
                }
            }
        }
        if disabled_count > 0 {
            self.log_action("Disabled", &format!("{disabled_count} mods"));
            self.set_message_ok(format!("Disabled {} mods", disabled_count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn batch_enable_selected(&mut self) {
        let game = self.selected_game().cloned();
        let mods_to_enable: Vec<String> = self.selected_mods.iter().cloned().collect();
        let mut enabled_count = 0;
        let mut unarchived_count = 0;
        for mod_id in mods_to_enable {
            if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
                if mod_entry.status == ModStatus::Disabled {
                    if xxmi::enable_mod(mod_entry).is_ok() {
                        enabled_count += 1;
                    }
                } else if mod_entry.status == ModStatus::Archived {
                    if let Some(game_ref) = game.as_ref() {
                        if xxmi::restore_mod(mod_entry, game_ref, self.state.use_default_mods_path).is_ok() {
                            unarchived_count += 1;
                        }
                    }
                }
            }
        }
        if enabled_count > 0 {
            self.log_action("Enabled", &format!("{enabled_count} mods"));
            self.set_message_ok(format!("Enabled {} mods", enabled_count));
        }
        if unarchived_count > 0 {
            self.log_action("Unarchived", &format!("{unarchived_count} mods"));
            self.set_message_ok(format!("Unarchived {} mods", unarchived_count));
        }
        if enabled_count + unarchived_count > 0 {
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn rename_mod_folder(&mut self, mod_id: &str, new_name: &str) -> Result<()> {
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
        xxmi::save_mod_metadata(mod_entry)?;
        Ok(())
    }

    fn perform_mod_rename(&mut self, mod_id: String) {
        let raw = self.mod_detail_edit_name.trim().to_string();
        if raw.is_empty() {
            self.mod_detail_editing = false;
            return;
        }
        let sanitized = sanitize_folder_name(&raw);
        if sanitized == "Imported Mod" || sanitized.chars().all(|c| c == '_') {
            self.mod_detail_editing = false;
            return;
        }
        if let Err(err) = self.rename_mod_folder(&mod_id, &sanitized) {
            self.report_error(err, Some("Rename failed"));
        } else {
            self.set_message_ok(format!("Renamed to: {sanitized}"));
            self.log_action("Renamed", &sanitized);
        }
        self.mod_detail_editing = false;
        self.refresh();
    }

    fn batch_archive_selected(&mut self) {
        let game = self.selected_game().cloned();
        let mods_to_archive: Vec<String> = self.selected_mods.iter().cloned().collect();
        let mut archived_count = 0;
        for mod_id in mods_to_archive {
            if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id).cloned() {
                self.clear_mod_image_runtime_state(&mod_entry);
            }
            if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
                if matches!(mod_entry.status, ModStatus::Active | ModStatus::Disabled) {
                    if let Some(game_ref) = game.as_ref() {
                        if xxmi::archive_mod(mod_entry, game_ref, self.state.use_default_mods_path).is_ok() {
                            archived_count += 1;
                        }
                    }
                }
            }
        }
        if archived_count > 0 {
            self.log_action("Archived", &format!("{archived_count} mods"));
            self.set_message_ok(format!("Archived {} mods", archived_count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn batch_delete_selected(&mut self) {
        let mods_to_delete: Vec<ModEntry> = self
            .selected_mods
            .iter()
            .filter_map(|id| self.state.mods.iter().find(|m| &m.id == id).cloned())
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
            let action = match self.state.delete_behavior {
                DeleteBehavior::RecycleBin => "Recycled",
                DeleteBehavior::Permanent => "Deleted",
            };
            self.log_action(action, &format!("{deleted_count} mods"));
            self.set_message_ok(format!("{action} {} mods", deleted_count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
        if let Some(err) = last_err {
            self.report_error(err, Some("Delete failed"));
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
