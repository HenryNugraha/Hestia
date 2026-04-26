impl HestiaApp {
    fn push_log(&mut self, summary: String) {
        let entry = OperationLogEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            summary,
        };
        self.state.operations.insert(
            0,
            entry.clone(),
        );
        if self.state.operations.len() > persistence::LOG_HISTORY_LIMIT {
            self.state.operations.truncate(persistence::LOG_HISTORY_LIMIT);
        }
        if let Err(err) = persistence::append_operation_log(&self.portable, &entry) {
            self.report_error_message(
                format!("failed to persist log history: {err:#}"),
                Some("Could not save data"),
            );
        }
        if self.state.show_log {
            self.log_scroll_to_bottom = true;
        }
    }

    fn log_action(&mut self, action: &str, subject: &str) {
        let subject = sanitize_log_subject(subject);
        if subject.is_empty() {
            self.push_log(action.to_string());
        } else {
            self.push_log(format!("{action}: {subject}"));
        }
    }

    fn import_source_path(source: &ImportSource) -> &Path {
        match source {
            ImportSource::Folder(path) | ImportSource::Archive(path) => path.as_path(),
        }
    }

    fn write_cached_download_to_temp_archive(
        &self,
        task_id: u64,
        cache_key: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        let Some(bytes) = persistence::cache_get(&self.portable, cache_key)? else {
            bail!("cached download not found");
        };
        let temp_dir = Self::runtime_temp_downloads_dir();
        fs::create_dir_all(&temp_dir)?;
        let path = temp_dir.join(format!(
            "{}-{}",
            task_id,
            sanitize_folder_name(file_name)
        ));
        fs::write(&path, bytes)?;
        Ok(path)
    }

    fn task_title_for_source(source: &ImportSource) -> String {
        Self::import_source_path(source)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mod")
            .to_string()
    }

    fn add_task(&mut self, id: u64, kind: TaskKind, status: TaskStatus, title: String, game_id: Option<String>, total_size: Option<u64>, unsafe_content: bool) {
        let now = Utc::now();
        let task = TaskEntry {
            id,
            kind,
            status,
            title,
            game_id,
            created_at: now,
            updated_at: now,
            total_size,
            unsafe_content,
        };
        self.state.tasks.push(task.clone());
        if let Err(err) = persistence::replace_task(&self.portable, &task) {
            self.report_error_message(
                format!("failed to persist task history: {err:#}"),
                Some("Could not save data"),
            );
        }
        self.tasks_scroll_to_edge = true;
        self.state.show_tasks = true;
        self.tasks_window_nonce = self.tasks_window_nonce.wrapping_add(1);
        self.tasks_force_default_pos = true;
        if self.state.tasks_layout == TasksLayout::Tabbed {
            if kind == TaskKind::Download {
                self.tasks_tab = TasksTab::Downloads;
            } else if kind == TaskKind::Install {
                self.tasks_tab = TasksTab::Installs;
            }
        }
    }

    fn add_install_task(&mut self, job: &InstallJob) {
        let now = Utc::now();
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone());
        let total_size = match &job.source {
            ImportSource::Archive(path) => fs::metadata(path).ok().map(|m| m.len()),
            _ => None,
        };
        if job.reuse_existing_task {
            if let Some(task) = self.state.tasks.iter_mut().find(|task| task.id == job.id) {
                task.status = TaskStatus::Queued;
                task.kind = TaskKind::Install;
                task.title = job
                    .title
                    .clone()
                    .unwrap_or_else(|| Self::task_title_for_source(&job.source));
                task.game_id = game_id;
                task.updated_at = now;
                if task.total_size.is_none() { task.total_size = total_size; }
                self.tasks_scroll_to_edge = true;
                let task_snapshot = task.clone();
                if let Err(err) = persistence::replace_task(&self.portable, &task_snapshot) {
                    self.report_error_message(
                        format!("failed to persist task history: {err:#}"),
                        Some("Could not save data"),
                    );
                }
            }
            return;
        }
        self.add_task(
            job.id,
            TaskKind::Install,
            TaskStatus::Queued,
            job.title
                .clone()
                .unwrap_or_else(|| Self::task_title_for_source(&job.source)),
            game_id,
            total_size,
            false,
        );
    }

    fn update_task_status(&mut self, job_id: u64, status: TaskStatus) {
        let mut snapshot = None;
        if let Some(task) = self.state.tasks.iter_mut().find(|task| task.id == job_id) {
            task.status = status;
            task.updated_at = Utc::now();
            snapshot = Some(task.clone());
        }
        if let Some(task) = snapshot {
            if let Err(err) = persistence::replace_task(&self.portable, &task) {
                self.report_error_message(
                    format!("failed to persist task history: {err:#}"),
                    Some("Could not save data"),
                );
            }
        }
        self.save_state();
    }

    fn remove_task(&mut self, job_id: u64) {
        self.state.tasks.retain(|task| task.id != job_id);
        if let Err(err) = persistence::remove_task(&self.portable, job_id) {
            self.report_error_message(
                format!("failed to persist task history: {err:#}"),
                Some("Could not save data"),
            );
        }
        self.save_state();
    }

    fn clear_completed_tasks(&mut self) {
        self.state.tasks.retain(|task| !task.status.is_terminal());
        if let Err(err) = persistence::clear_finished_tasks(&self.portable) {
            self.report_error_message(
                format!("failed to persist task history: {err:#}"),
                Some("Could not save data"),
            );
        }
        self.save_state();
    }

    fn clear_cache(&mut self) -> Result<()> {
        persistence::clear_cache_and_vacuum(&self.portable)?;
        Self::cleanup_runtime_temp_downloads_best_effort();
        self.browse_image_textures.clear();
        self.browse_thumb_textures.clear();
        self.browse_image_queue.clear();
        self.browse_image_inflight.clear();
        self.browse_state.details.clear();
        self.browse_state.screenshot_overlay = None;
        self.rebuild_texture_tracking();
        self.mark_usage_counters_dirty();
        Ok(())
    }

    fn game_archive_root(game: &GameInstall, use_default: bool) -> Option<PathBuf> {
        let live_root = game.mods_path(use_default)?;
        let parent = live_root.parent()?;
        Some(parent.join("Mods_Archived"))
    }

    fn archive_usage_bytes(&self) -> u64 {
        self.state
            .games
            .iter()
            .filter_map(|g| Self::game_archive_root(g, self.state.use_default_mods_path))
            .map(|root| Self::directory_size_bytes(&root))
            .sum()
    }

    fn cache_usage_bytes(&self) -> u64 {
        let db_bytes = fs::metadata(&self.portable.state_archive)
            .map(|m| m.len())
            .unwrap_or(0);
        let temp_bytes = Self::directory_size_bytes(&Self::runtime_temp_root());
        db_bytes.saturating_add(temp_bytes)
    }

    fn directory_size_bytes(root: &Path) -> u64 {
        if !root.exists() {
            return 0;
        }
        let mut total = 0_u64;
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
        total
    }

    fn clear_archives(&mut self) -> Result<usize> {
        let mut removed = 0_usize;
        for game in &self.state.games {
            let Some(root) = Self::game_archive_root(game, self.state.use_default_mods_path) else {
                continue;
            };
            if !root.exists() {
                continue;
            }
            for entry in fs::read_dir(&root)? {
                let entry = entry?;
                let path = entry.path();
                match self.state.delete_behavior {
                    DeleteBehavior::RecycleBin => trash::delete(&path)?,
                    DeleteBehavior::Permanent => {
                        if path.is_dir() {
                            fs::remove_dir_all(&path)?;
                        } else if path.is_file() {
                            fs::remove_file(&path)?;
                        }
                    }
                }
                removed += 1;
            }
            let _ = fs::remove_dir(&root);
        }
        self.mark_usage_counters_dirty();
        Ok(removed)
    }

    fn mark_usage_counters_dirty(&mut self) {
        self.usage_counters_dirty = true;
    }

    fn refresh_usage_counters_if_needed(&mut self, now: f64) {
        let should_refresh = self.usage_counters_dirty
            || now - self.usage_counters_last_refresh >= SETTINGS_USAGE_REFRESH_SECS;
        if !should_refresh {
            return;
        }
        self.usage_cache_bytes = self.cache_usage_bytes();
        self.usage_archive_bytes = self.archive_usage_bytes();
        self.usage_counters_last_refresh = now;
        self.usage_counters_dirty = false;
    }

    fn sorted_tasks(&self, filter: impl Fn(&TaskEntry) -> bool) -> Vec<TaskEntry> {
        let mut items: Vec<TaskEntry> = self
            .state
            .tasks
            .iter()
            .filter(|task| {
                if self.state.unsafe_content_mode == UnsafeContentMode::HideNoCounter
                    && task.unsafe_content
                {
                    return false;
                }
                filter(task)
            })
            .cloned()
            .collect();
        items.sort_by_key(|task| task.created_at);
        if self.state.tasks_order == TasksOrder::NewestFirst {
            items.reverse();
        }
        items
    }

    fn task_status_label(status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Queued => "Queued",
            TaskStatus::Installing => "Installing",
            TaskStatus::Downloading => "Downloading",
            TaskStatus::Canceling => "Canceling",
            TaskStatus::Completed => "Completed",
            TaskStatus::Failed => "Failed",
            TaskStatus::Canceled => "Canceled",
        }
    }

    fn task_status_color(status: TaskStatus) -> Color32 {
        match status {
            TaskStatus::Completed => Color32::from_rgb(96, 179, 123),
            TaskStatus::Failed => Color32::from_rgb(196, 86, 86),
            TaskStatus::Canceled => Color32::from_rgb(140, 140, 140),
            TaskStatus::Canceling => Color32::from_rgb(178, 122, 64),
            TaskStatus::Queued | TaskStatus::Installing => Color32::from_rgb(120, 154, 198),
            TaskStatus::Downloading => Color32::from_rgb(120, 154, 198),
        }
    }

    fn render_task_row(&mut self, ui: &mut Ui, task: &TaskEntry) {
        let status_label = Self::task_status_label(task.status);
        let status_color = Self::task_status_color(task.status);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let icon_size = 40.0;
                if self.is_app_update_task(task) {
                    if let Some(texture) = self.app_icon_texture.as_ref() {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::splat(icon_size)));
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(icon_size), Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            6.0,
                            Color32::from_rgba_premultiplied(40, 42, 46, 200),
                        );
                    }
                } else if let Some(game_id) = task.game_id.as_deref() {
                    if !self.game_icon_textures.contains_key(game_id) {
                        self.request_icon_texture(game_id);
                    }
                    if let Some(texture) = self.game_icon_textures.get(game_id) {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::splat(icon_size)));
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(icon_size), Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            6.0,
                            Color32::from_rgba_premultiplied(40, 42, 46, 200),
                        );
                    }
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::splat(icon_size), Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        6.0,
                        Color32::from_rgba_premultiplied(40, 42, 46, 200),
                    );
                }
                ui.add_space(-2.0);
                ui.vertical(|ui| {
                    ui.label(bold(task.title.clone()));
                    ui.add_space(-6.0);
                    let time_str = format_exact_local_timestamp(task.created_at.timestamp());
                    if matches!(task.status, TaskStatus::Completed) && task.total_size.is_some() {
                        let size = format_file_size(task.total_size.unwrap());
                        ui.label(
                            RichText::new(format!("{size} • {time_str}"))
                                .small()
                                .color(Color32::from_gray(150)),
                        );
                    } else {
                        ui.label(
                            RichText::new(time_str)
                                .small()
                                .color(Color32::from_gray(150)),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cancellable = matches!(
                        task.status,
                        TaskStatus::Queued
                            | TaskStatus::Installing
                            | TaskStatus::Downloading
                            | TaskStatus::Canceling
                    );
                    if cancellable {
                        let cancel_label = if task.status == TaskStatus::Canceling {
                            "Canceling…"
                        } else {
                            "Cancel"
                        };
                        let enabled = task.status != TaskStatus::Canceling;
                        let response = ui.add_enabled(enabled, egui::Button::new(cancel_label));
                        if response.clicked() {
                            self.cancel_task(task.id);
                        }
                    }
                    let badge = egui::Frame::new()
                        .fill(status_color)
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(8, 4));
                    badge.show(ui, |ui| {
                        ui.label(
                            RichText::new(status_label)
                                .color(Color32::WHITE)
                                .size(11.0),
                        );
                    });
                });
            });

            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::Installing
                    | TaskStatus::Downloading
                    | TaskStatus::Canceling
            ) {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 18.0),
                    Sense::hover(),
                );

                ui.painter().rect_filled(rect, egui::CornerRadius::same(4), Color32::from_gray(45));

                let text_left;
                let mut text_right = String::new();
                let mut ratio = 0.0;
                let mut is_indeterminate = true;

                if task.status == TaskStatus::Downloading {
                    if let Some(progress) = self
                        .browse_download_inflight
                        .get(&task.id)
                        .map(|inflight| Arc::clone(&inflight.progress))
                        .or_else(|| self.app_update_task_progress(task.id))
                    {
                        let progress = progress.read().unwrap();
                        ratio = if let Some(total) = progress.total {
                            if total > 0 { progress.downloaded as f32 / total as f32 } else { 0.0 }
                        } else { 0.0 };
                        is_indeterminate = false;
                        text_left = if let Some(total) = progress.total {
                            format!("{} / {} ({:.1}%)", format_file_size(progress.downloaded), format_file_size(total), ratio * 100.0)
                        } else { format_file_size(progress.downloaded) };
                        text_right = format!("↓ {}", format_speed(progress.speed));
                    } else {
                        text_left = "Starting download…".to_string();
                    }
                } else {
                    text_left = match task.status {
                        TaskStatus::Queued => "Queued…".to_string(),
                        TaskStatus::Installing => "Installing mod files…".to_string(),
                        TaskStatus::Canceling => "Canceling task…".to_string(),
                        _ => "".to_string(),
                    };
                    if let Some(size) = task.total_size {
                        text_right = format_file_size(size);
                    }
                }

                if is_indeterminate {
                    let time = ui.input(|i| i.time);
                    let t = (time * 0.6) % 1.0;
                    let seg_width = rect.width() * 0.3;
                    let x = -seg_width + (rect.width() + seg_width) * t as f32;
                    let filled_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x + x.max(0.0), rect.min.y),
                        egui::pos2(rect.min.x + (x + seg_width).min(rect.width()), rect.max.y)
                    ).intersect(rect);
                    ui.painter().rect_filled(filled_rect, egui::CornerRadius::same(4), Color32::from_rgb(60, 140, 200).linear_multiply(0.6));
                    ui.ctx().request_repaint();
                } else {
                    let filled_rect = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * ratio.clamp(0.0, 1.0), rect.height()));
                    ui.painter().rect_filled(filled_rect, egui::CornerRadius::same(4), Color32::from_rgb(60, 140, 200));
                }

                let text_color = Color32::WHITE;
                ui.painter().text(rect.min + Vec2::new(6.0, 9.0), egui::Align2::LEFT_CENTER, text_left, egui::FontId::proportional(11.5), text_color);
                ui.painter().text(rect.max + Vec2::new(-6.0, -9.0), egui::Align2::RIGHT_CENTER, text_right, egui::FontId::proportional(11.5), text_color);
            }
        });
    }

    fn cancel_task(&mut self, job_id: u64) {
        if self.cancel_app_update_task(job_id) {
            return;
        }

        if let Some(index) = self
            .browse_download_queue
            .iter()
            .position(|job| job.task_id == job_id)
        {
            let title = self.browse_download_queue[index].title.clone();
            self.browse_download_queue.remove(index);
            self.update_task_status(job_id, TaskStatus::Canceled);
            self.set_message_ok(format!("Download canceled: {title}"));
            return;
        }

        if let Some(inflight) = self.browse_download_inflight.get(&job_id) {
            inflight.cancel.store(true, Ordering::Relaxed);
            self.update_task_status(job_id, TaskStatus::Canceling);
            return;
        }

        if let Some(index) = self.install_queue.iter().position(|job| job.id == job_id) {
            if let Some(job) = self.install_queue.get(index).cloned() {
                Self::cleanup_runtime_temp_for_source(&job.source);
            }
            self.install_queue.remove(index);
            self.update_task_status(job_id, TaskStatus::Canceled);
            if self.install_batch_active {
                self.install_batch_stats.skipped += 1;
            }
            let _ = self.install_request_tx.send(InstallRequest::Drop { job_id });
            self.set_message_ok("Install canceled");
            return;
        }

        if self.install_inflight.contains_key(&job_id) {
            let pending_import = self
                .pending_imports
                .iter()
                .any(|pending| pending.job_id == job_id);
            let pending_conflict = self
                .pending_conflicts
                .iter()
                .any(|conflict| conflict.job_id == job_id);

            if pending_import || pending_conflict {
                self.pending_imports.retain(|pending| pending.job_id != job_id);
                self.pending_conflicts
                    .retain(|conflict| conflict.job_id != job_id);
                if let Some(current) = self.install_inflight.remove(&job_id) {
                    Self::cleanup_runtime_temp_for_source(&current.source);
                }
                self.update_task_status(job_id, TaskStatus::Canceled);
                let _ = self.install_request_tx.send(InstallRequest::Drop { job_id });
                self.set_message_ok("Install canceled");
            } else {
                self.update_task_status(job_id, TaskStatus::Canceling);
                let _ = self
                    .install_request_tx
                    .send(InstallRequest::Cancel { job_id });
            }
        }
    }

}
