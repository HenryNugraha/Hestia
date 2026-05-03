impl HestiaApp {
    fn render_pending_import(&mut self, ctx: &egui::Context) {
        if !self.pending_conflicts.is_empty() {
            return;
        }
        let Some(pending) = self.pending_imports.front().cloned() else {
            return;
        };
        let job_id = pending.job_id;
        let inspection = pending.inspection.clone();
        
        let pending_meta = self.pending_browse_install_meta.get(&job_id).cloned();
        let update_folder_name = pending_meta
            .as_ref()
            .and_then(|m| m.update_folder_name.clone());

        // Auto-proceed if only one candidate
        if inspection.candidates.len() == 1 {
            let Some(game) = self
                .state
                .games
                .iter()
                .find(|game| game.definition.id == inspection.game_id)
                .cloned()
            else {
                self.pending_imports.pop_front();
                return;
            };
            let candidate = &inspection.candidates[0];
            let mod_title = pending_meta
                .as_ref()
                .and_then(|m| self.browse_mod_title_for_install(m.mod_id, m.update_target_mod_id.as_deref()));
            let preferred = if let Some(target) = &update_folder_name {
                target.clone()
            } else {
                self.preferred_browse_folder_name(mod_title.as_deref(), &candidate.label)
            };
                            let target_root = game.mods_path(self.state.use_default_mods_path).unwrap_or_default();
                            let existing_target = target_root.join(&preferred);
                            if existing_target.exists() {
                                if update_folder_name.is_some() {
                                    self.pending_imports.pop_front();
                                    if let Some(choice) = self.resolve_update_existing_target_choice(job_id) {
                                        self.commit_import(job_id, vec![0], choice, target_root, pending.gb_profile.clone(), vec![preferred]);
                                    } else {
                                        let gb_profile = pending.gb_profile.clone();
                                        self.pending_conflicts.push_back(PendingConflict {
                                            job_id,
                                            candidate_indices: vec![0],
                                            preferred_name: preferred.clone(),
                                            target_root,
                                            existing_target,
                                            gb_profile,
                                        });
                                    }
                                    return;
                                }
                                let gb_profile = pending.gb_profile.clone();
                self.pending_conflicts.push_back(PendingConflict {
                    job_id,
                    candidate_indices: vec![0],
                    preferred_name: preferred.clone(),
                    target_root,
                    existing_target,
                    gb_profile,
                });
                self.pending_imports.pop_front();
                return;
            } else {
                let gb_profile = pending.gb_profile.clone();
                self.pending_imports.pop_front();
                self.commit_import(job_id, vec![0], ConflictChoice::KeepBoth, target_root, gb_profile, vec![preferred]);
                return;
            }
        }
        
        let mut commit_intent = None;
        let mut cancel = false;

        let mod_name = self.pending_browse_install_meta.get(&job_id)
            .and_then(|meta| self.browse_state.details.get(&meta.mod_id))
            .map(|d| d.profile.name.clone())
            .or_else(|| {
                self.state.tasks.iter().find(|t| t.id == job_id).map(|t| t.title.clone())
            })
            .unwrap_or_else(|| "Imported Mod".to_string());

        if update_folder_name.is_some() {
            let tracked_labels = pending_meta
                .as_ref()
                .and_then(|meta| meta.update_target_mod_id.as_ref())
                .and_then(|target_id| self.state.mods.iter().find(|m| m.id == *target_id))
                .and_then(|mod_entry| mod_entry.source.as_ref())
                .map(|source| source.file_set.selected_candidate_labels.clone())
                .unwrap_or_default();
            if !tracked_labels.is_empty() {
                let mut candidate_indices = Vec::new();
                for tracked_label in &tracked_labels {
                    if let Some(index) = inspection
                        .candidates
                        .iter()
                        .position(|candidate| candidate.label == *tracked_label)
                    {
                        candidate_indices.push(index);
                    } else {
                        candidate_indices.clear();
                        break;
                    }
                }
                candidate_indices.sort();
                candidate_indices.dedup();
                if !candidate_indices.is_empty() && candidate_indices.len() == tracked_labels.len() {
                    let Some(game) = self
                        .state
                        .games
                        .iter()
                        .find(|game| game.definition.id == inspection.game_id)
                        .cloned()
                    else {
                        self.pending_imports.pop_front();
                        return;
                    };
                    let target_root = game
                        .mods_path(self.state.use_default_mods_path)
                        .unwrap_or_default();
                    let preferred = update_folder_name
                        .clone()
                        .unwrap_or_else(|| "Imported Mod".to_string());
                    let preferred_names = vec![preferred.clone(); candidate_indices.len()];
                    if let Some(choice) = self.resolve_update_existing_target_choice(job_id) {
                        self.pending_imports.pop_front();
                        self.commit_import(
                            job_id,
                            candidate_indices,
                            choice,
                            target_root,
                            pending.gb_profile.clone(),
                            preferred_names,
                        );
                    } else {
                        self.pending_imports.pop_front();
                        self.pending_conflicts.push_back(PendingConflict {
                            job_id,
                            candidate_indices,
                            preferred_name: preferred.clone(),
                            existing_target: target_root.join(&preferred),
                            target_root,
                            gb_profile: pending.gb_profile.clone(),
                        });
                    }
                    return;
                }
            }
        }

        let constrain_rect = self.last_right_pane_rect.unwrap_or_else(|| ctx.available_rect());
        let window = egui::Window::new("Missing .ini")
            .id(egui::Id::new(("import_review", job_id)))
            .default_pos(constrain_rect.min + egui::vec2(16.0, 16.0))
            .default_size(egui::vec2(420.0, 420.0))
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .constrain_to(constrain_rect)
            .frame(
                egui::Frame::window(&ctx.style())
                    .inner_margin(egui::Margin::same(16))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(82, 134, 186))),
            );

        window.show(ctx, |ui| {
            ui.horizontal(|ui| {
                static_label(ui, icon_rich(Icon::Info, 96.0, Color32::from_rgb(148, 192, 232)));
                ui.vertical(|ui| {
                    static_label(ui, bold(&mod_name).underline().size(16.0));
                    ui.add_space(4.0);
                    static_label(
                        ui,
                        RichText::new("No recognizable .ini file found in the archive’s parent path, archhive may contains multiple mods.\nSelect which folder(s) to install:")
                            .size(14.0),
                    );
                });
            });
            ui.add_space(-8.0);

            let selection_id = ui.id().with("selection");
            let mut selected_indices: HashSet<usize> = ui.data_mut(|d| d.get_temp(selection_id).unwrap_or_default());

            ui.vertical_centered(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(180, 180, 180, 48))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(16))
                    .outer_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 0,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.set_width(388.0);
                        ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 6.0;
                                for (index, candidate) in inspection.candidates.iter().enumerate() {
                                    let is_selected = selected_indices.contains(&index);
                                    let row_response = egui::Frame::group(ui.style())
                                        .inner_margin(egui::Margin::symmetric(10, 8))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                larger_checkbox(ui, is_selected);
                                                static_label(ui, bold(&candidate.label));
                                            });
                                        }).response;
                                    if row_response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        if is_selected { selected_indices.remove(&index); }
                                        else { selected_indices.insert(index); }
                                    }
                                }
                            });
                    });
                ui.horizontal(|ui| {
                    let num_selected = selected_indices.len();
                    
                    if num_selected <= 1 {
                        if ui.add(egui::Button::new("Install").fill(Color32::from_rgb(180, 78, 35)))
                            .clicked() 
                        {
                            commit_intent = Some((selected_indices.clone(), true));
                        }
                    } else {
                        if ui.add(egui::Button::new("Install Merged").fill(Color32::from_rgb(180, 78, 35)))
                            .on_hover_text("Install selected folders into the same mod folder and treat them as a single mod")
                            .clicked() 
                        {
                            commit_intent = Some((selected_indices.clone(), true));
                        }
                        if ui.add(egui::Button::new("Install Separately").fill(Color32::from_rgb(180, 78, 35)))
                            .on_hover_text("Install selected folders into their own mod folder")
                            .clicked() 
                        {
                            commit_intent = Some((selected_indices.clone(), false));
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            ui.data_mut(|d| d.insert_temp(selection_id, selected_indices));
        });

        if let Some((indices, merged)) = commit_intent {
            if indices.is_empty() {
                self.set_message_ok("No folders selected");
                return;
            }
            let Some(game) = self.state.games.iter().find(|game| game.definition.id == inspection.game_id).cloned() else {
                self.pending_imports.pop_front();
                return;
            };
            let target_root = game.mods_path(self.state.use_default_mods_path).unwrap_or_default();
            let gb_profile = pending.gb_profile.clone();
            
            let title_name = self.sanitized_preferred_browse_title_name(Some(&mod_name));
            let mut candidate_indices: Vec<usize> = indices.into_iter().collect();
            candidate_indices.sort();

            if merged {
                let preferred = if let Some(target) = &update_folder_name {
                    target.clone()
                } else {
                    self.preferred_browse_folder_name(title_name.as_deref(), "Imported Mod")
                };
                let existing_target = target_root.join(&preferred);
                if existing_target.exists() && update_folder_name.is_none() {
                    self.pending_conflicts.push_back(PendingConflict {
                        job_id,
                        candidate_indices,
                        preferred_name: preferred.clone(),
                        target_root,
                        existing_target,
                        gb_profile,
                    });
                } else {
                    let preferred_names = vec![preferred.clone(); candidate_indices.len()];
                    if update_folder_name.is_some() && existing_target.exists() {
                        if let Some(choice) = self.resolve_update_existing_target_choice(job_id) {
                            self.commit_import(
                                job_id,
                                candidate_indices,
                                choice,
                                target_root,
                                gb_profile.clone(),
                                preferred_names,
                            );
                        } else {
                            self.pending_conflicts.push_back(PendingConflict {
                                job_id,
                                candidate_indices,
                                preferred_name: preferred.clone(),
                                target_root,
                                existing_target,
                                gb_profile,
                            });
                        }
                    } else {
                        let choice = if update_folder_name.is_some() && self.should_auto_replace_update(job_id) {
                            ConflictChoice::Replace
                        } else {
                            ConflictChoice::KeepBoth
                        };
                        self.commit_import(
                            job_id,
                            candidate_indices,
                            choice,
                            target_root,
                            gb_profile.clone(),
                            preferred_names,
                        );
                    }
                }
            } else {
                let mut preferred_names = Vec::new();
                for idx in &candidate_indices {
                    let label = &inspection.candidates[*idx].label;
                    if let Some(title_name) = title_name.as_deref() {
                        preferred_names.push(format!(
                            "{} - {}",
                            title_name,
                            sanitize_folder_name(label)
                        ));
                    } else {
                        preferred_names.push(sanitize_folder_name(label));
                    }
                }
                self.commit_import(
                    job_id,
                    candidate_indices,
                    ConflictChoice::KeepBoth,
                    target_root,
                    gb_profile,
                    preferred_names
                );
            }
            self.pending_imports.pop_front();
        }

        if cancel {
            let cancel_name = inspection
                .candidates
                .get(0)
                .map(|candidate| candidate.label.as_str())
                .unwrap_or("mod");
            let _ = self.install_request_tx.send(InstallRequest::Drop {
                job_id,
            });
            if self.install_batch_active {
                self.install_batch_stats.skipped += 1;
            }
            self.pending_imports.pop_front();
            if let Some(current) = self.install_inflight.remove(&job_id) {
                Self::cleanup_runtime_temp_for_source(&current.source);
            }
            self.update_task_status(job_id, TaskStatus::Canceled);
            self.set_message_ok(format!("Install canceled: {}", cancel_name));
        }
    }

    fn render_pending_conflict(&mut self, ctx: &egui::Context) {
        let Some(conflict) = self.pending_conflicts.front().cloned() else {
            return;
        };
        let conflict_existing_target = conflict.existing_target.clone();
        let conflict_target_root = conflict.target_root.clone();
        let conflict_preferred_name = conflict.preferred_name.clone();
            let mut choice = None;

        let warn_color = Color32::from_rgb(214, 96, 34);
        let mut window = egui::Window::new("Installation Conflict")
            .collapsible(false)
            .order(egui::Order::Foreground)
            .resizable(false)
            .frame(egui::Frame::window(&ctx.style()).stroke(egui::Stroke::new(1.0, warn_color)));

        if let Some(rect) = self.last_right_pane_rect {
            let inset_rect = rect.shrink2(egui::vec2(12.0, 12.0));
            window = window
                .default_pos(inset_rect.min + egui::vec2(16.0, 16.0))
                .constrain_to(inset_rect);
        }

        window.show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(icon_rich(
                            Icon::TriangleAlert,
                            96.0,
                            warn_color,
                        ))
                        .selectable(false),
                    ).on_hover_cursor(egui::CursorIcon::Default);
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                bold(conflict_existing_target
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("this folder"),
                                ).underline().size(16.0)
                            )
                            .selectable(false),
                        ).on_hover_cursor(egui::CursorIcon::Default);
                        ui.add(egui::Label::new("Already exists in:").selectable(false))
                            .on_hover_cursor(egui::CursorIcon::Default);
                        ui.add(
                            egui::Label::new(
                                RichText::new(conflict_target_root.display().to_string()).monospace()
                            )
                            .selectable(false),
                        ).on_hover_cursor(egui::CursorIcon::Default);
                        ui.horizontal(|ui| {
                            if ui.button("Replace").clicked() {
                                choice = Some(ConflictChoice::Replace);
                            }
                            if ui.button("Merge").clicked() {
                                choice = Some(ConflictChoice::Merge);
                            }
                            if ui.button("Keep Both").clicked() {
                                choice = Some(ConflictChoice::KeepBoth);
                            }
                            if ui.button("Cancel").clicked() {
                                choice = Some(ConflictChoice::Cancel);
                            }
                        });
                    });
                    ui.add_space(4.0);
                });
                ui.add_space(4.0);
            });

        if let Some(choice) = choice {
            let gb_profile = conflict.gb_profile.clone();
            let job_id = conflict.job_id;
            let conflict_name = conflict_existing_target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("mod");
            match choice {
                ConflictChoice::Replace => {
                    self.log_action("Conflict (Replace)", conflict_name);
                }
                ConflictChoice::Merge => {
                    self.log_action("Conflict (Merge)", conflict_name);
                }
                ConflictChoice::KeepBoth => {
                    self.log_action("Conflict (Keep Both)", conflict_name);
                }
                ConflictChoice::Cancel => {
                    self.log_action("Conflict (Cancel)", conflict_name);
                    self.set_message_ok(format!("Install canceled: {}", conflict_name));
                }
            }
            self.save_state();
            self.pending_conflicts.pop_front();
            if choice == ConflictChoice::Cancel {
                let _ = self.install_request_tx.send(InstallRequest::Drop { job_id });
                self.pending_imports.retain(|pending| pending.job_id != job_id);
                if let Some(current) = self.install_inflight.remove(&job_id) {
                    Self::cleanup_runtime_temp_for_source(&current.source);
                }
                if self.install_batch_active {
                    self.install_batch_stats.skipped += 1;
                }
                self.update_task_status(job_id, TaskStatus::Canceled);
            } else {
                let preferred_names = vec![conflict_preferred_name; conflict.candidate_indices.len()];
                self.commit_import(
                    job_id,
                    conflict.candidate_indices.clone(),
                    choice,
                    conflict_target_root,
                    gb_profile,
                    preferred_names,
                );
                self.pending_imports.retain(|pending| pending.job_id != job_id);
            }
        }
    }

    fn detect_drag_and_drop(&mut self, ctx: &egui::Context) {
        // Show a visual cue when files are hovered
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let painter =
                ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dnd_layer")));
            let screen_rect = ctx.viewport_rect();
            painter.rect_filled(screen_rect, 0.0, Color32::from_rgba_unmultiplied(24, 26, 29, 220));
            let drop_text = if let Some((_, mod_name)) = self.selected_unlinked_mod_context() {
                let mut display_name: String = mod_name.chars().take(60).collect();
                if display_name.chars().count() < mod_name.chars().count() {
                    display_name.push_str("...");
                }
                format!("Drop mods to install them\n\nor\n\ndrop images to add into:\n{display_name}")
            } else {
                "Drop to install".to_string()
            };
            painter.text(
                screen_rect.center(),
                egui::Align2::CENTER_CENTER,
                drop_text,
                egui::FontId::proportional(48.0),
                Color32::WHITE,
            );
        }

        // Handle dropped files
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() {
            let mut batch_sources = Vec::new();
            let mut image_paths = Vec::new();
            let mut seen_paths = HashSet::new();
            let mut queued_count = 0;
            for file in dropped_files {
                let Some(path) = file.path else {
                    continue;
                };
                if !seen_paths.insert(path.clone()) {
                    self.install_batch_stats.skipped += 1;
                    continue;
                }
                if path.is_dir() {
                    batch_sources.push(ImportSource::Folder(path));
                    queued_count += 1;
                    continue;
                }
                if Self::is_static_image_path(&path) {
                    image_paths.push(path);
                    continue;
                }
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    match ext.to_lowercase().as_str() {
                        "zip" | "rar" | "7z" => {
                            batch_sources.push(ImportSource::Archive(path));
                            queued_count += 1;
                        }
                        _ => {
                            self.install_batch_stats.unsupported += 1;
                            self.set_message_ok(format!(
                                "Unsupported: {}",
                                path.file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("file")
                            ));
                            self.log_action(
                                "Unsupported",
                                &path.file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("file"),
                            );
                        }
                    }
                } else {
                    self.install_batch_stats.unsupported += 1;
                    self.set_message_ok(format!(
                        "Unsupported: {}",
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("file")
                    ));
                    self.log_action(
                        "Unsupported",
                        &path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("file"),
                    );
                }
            }

            if !image_paths.is_empty() {
                if let Some((mod_id, _)) = self.selected_unlinked_mod_context() {
                    let image_count = image_paths.len();
                    match self.enqueue_add_images_to_unlinked_mod(&mod_id, image_paths) {
                        Ok(()) => self.set_message_ok(format!("Adding {} image(s)", image_count)),
                        Err(err) => self.report_error(err, Some("Could not add images")),
                    }
                } else {
                    self.report_warn(
                        "image files were dropped without an open unlinked mod detail",
                        Some("Open an unlinked mod detail first"),
                    );
                }
            }
            self.enqueue_install_sources(batch_sources);
            if queued_count > 0 {
                self.set_message_ok(format!("Installing: {} mod(s)", queued_count));
            }
        }
    }
}
