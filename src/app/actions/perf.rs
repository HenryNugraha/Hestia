impl HestiaApp {
    fn record_perf_diagnostics(
        &mut self,
        ctx: &egui::Context,
        frame_time: f64,
        update_elapsed: Duration,
    ) {
        let update_ms = update_elapsed.as_secs_f64() * 1000.0;
        let frame_interval_ms = self
            .perf_diagnostics
            .last_frame_time
            .map(|last| ((frame_time - last) * 1000.0).max(0.0))
            .filter(|value| value.is_finite());
        self.perf_diagnostics.last_frame_time = Some(frame_time);

        let Some(frame_interval_ms) = frame_interval_ms else {
            return;
        };

        let fps = if frame_interval_ms > 0.0 {
            1000.0 / frame_interval_ms
        } else {
            0.0
        };
        self.perf_diagnostics.frame_interval_ms_ema =
            Some(Self::perf_ema(self.perf_diagnostics.frame_interval_ms_ema, frame_interval_ms));
        self.perf_diagnostics.update_ms_ema =
            Some(Self::perf_ema(self.perf_diagnostics.update_ms_ema, update_ms));
        self.perf_diagnostics.fps_ema = Some(Self::perf_ema(
            self.perf_diagnostics.fps_ema,
            fps.clamp(0.0, 999.0),
        ));
        self.perf_diagnostics.max_frame_interval_ms = self
            .perf_diagnostics
            .max_frame_interval_ms
            .max(frame_interval_ms);
        self.perf_diagnostics.max_update_ms =
            self.perf_diagnostics.max_update_ms.max(update_ms);

        let frame_interval_ema = self
            .perf_diagnostics
            .frame_interval_ms_ema
            .unwrap_or(frame_interval_ms);
        let update_ema = self.perf_diagnostics.update_ms_ema.unwrap_or(update_ms);
        let fps_ema = self.perf_diagnostics.fps_ema.unwrap_or(fps);
        let slow_frame = frame_interval_ms >= PERF_SLOW_FRAME_INTERVAL_MS
            || frame_interval_ema >= PERF_SLOW_FRAME_EMA_MS
            || update_ms >= PERF_SLOW_UPDATE_MS;
        if slow_frame {
            self.perf_diagnostics.slow_frame_streak =
                self.perf_diagnostics.slow_frame_streak.saturating_add(1);
        } else {
            self.perf_diagnostics.slow_frame_streak = 0;
        }

        let summary = self.perf_diagnostics_summary(
            fps_ema,
            frame_interval_ema,
            frame_interval_ms,
            update_ema,
            update_ms,
        );
        self.perf_diagnostics.last_summary = summary.clone();

        let should_periodic_log = self.perf_diagnostics.periodic_logging
            && (self.perf_diagnostics.last_periodic_log_time == 0.0
                || frame_time - self.perf_diagnostics.last_periodic_log_time
                    >= PERF_PERIODIC_LOG_INTERVAL_SECS);
        let should_slow_log = slow_frame
            && (self.perf_diagnostics.last_slow_log_time == 0.0
                || frame_time - self.perf_diagnostics.last_slow_log_time
                    >= PERF_LOG_MIN_INTERVAL_SECS);

        if should_periodic_log || should_slow_log {
            let reason = if should_slow_log { "slow" } else { "periodic" };
            self.push_log(format!("perf {reason}: {summary}"));
            if should_slow_log {
                self.perf_diagnostics.last_slow_log_time = frame_time;
            }
            if should_periodic_log {
                self.perf_diagnostics.last_periodic_log_time = frame_time;
            }
            self.perf_diagnostics.max_frame_interval_ms = frame_interval_ms;
            self.perf_diagnostics.max_update_ms = update_ms;
        }

        if self.perf_diagnostics.overlay_enabled {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn perf_ema(previous: Option<f64>, value: f64) -> f64 {
        const ALPHA: f64 = 0.12;
        previous.map_or(value, |previous| previous + (value - previous) * ALPHA)
    }

    fn perf_diagnostics_summary(
        &self,
        fps_ema: f64,
        frame_interval_ema: f64,
        frame_interval_ms: f64,
        update_ema: f64,
        update_ms: f64,
    ) -> String {
        let texture_ram_gb =
            self.texture_ram_estimated_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let texture_budget_gb =
            self.texture_ram_budget_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        format!(
            "fps={fps_ema:.1} frame_ms={frame_interval_ema:.1}/{frame_interval_ms:.1} update_ms={update_ema:.1}/{update_ms:.1} max_ms={:.1}/{:.1} streak={} view={} tex_queue={} image_loads={} mod_q={} browse_q={} browse_inflight={} tex_ram={texture_ram_gb:.2}/{texture_budget_gb:.2}GB gif_visible={} gif_anim={} gif_pending={}/{} textures=m{}/{} b{}/{} workers={}/{}",
            self.perf_diagnostics.max_frame_interval_ms,
            self.perf_diagnostics.max_update_ms,
            self.perf_diagnostics.slow_frame_streak,
            self.current_view.perf_label(),
            self.pending_texture_uploads.len(),
            self.pending_image_loads.len(),
            self.pending_mod_image_queue.len(),
            self.browse_image_queue.len(),
            self.browse_image_inflight.len(),
            self.visible_gif_texture_keys.len(),
            self.animated_gif_state.len(),
            self.pending_gif_previews.len(),
            self.pending_gif_animations.len(),
            self.mod_cover_textures.len(),
            self.mod_full_textures.len(),
            self.browse_thumb_textures.len(),
            self.browse_image_textures.len(),
            self.pending_events.has_worker_events as u8,
            self.pending_events.has_process_work as u8,
        )
    }

    fn render_perf_diagnostics_overlay(&self, ctx: &egui::Context) {
        if !self.perf_diagnostics.overlay_enabled
            || self.perf_diagnostics.last_summary.is_empty()
        {
            return;
        }

        egui::Area::new(egui::Id::new("perf_diagnostics_overlay"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(23, 25, 29, 230))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(75, 80, 88)))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_max_width(520.0);
                        ui.label(
                            RichText::new(&self.perf_diagnostics.last_summary)
                                .monospace()
                                .size(11.0)
                                .color(Color32::from_rgb(224, 226, 230)),
                        );
                    });
            });
    }
}

impl ViewMode {
    fn perf_label(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Browse => "browse",
        }
    }
}
