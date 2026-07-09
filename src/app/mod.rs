pub(crate) mod content;

use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Local, Utc};
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, RichText, ScrollArea, Sense, TextEdit,
    TextFormat, Ui, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use fast_image_resize as fir;
use futures_util::StreamExt;
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest_middleware::{ClientBuilder as MiddlewareClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use rfd::FileDialog;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Semaphore, mpsc as tokio_mpsc};
use uuid::Uuid;
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    importing::{self, PreparedImport},
    integrations::{gamebanana, unrealengine, xxmi},
    model::{
        AfterInstallBehavior, AppFontStyle, AppLanguage, AppState, BrowseDownloadTaskFile,
        BrowseDownloadTaskPayload, BrowseSort, CacheSizeTier, ConflictChoice, CustomProxyConfig,
        DeleteBehavior, FileSetRecipe, GameBackend, GameBananaFileMeta, GameBananaLink,
        GameBananaSnapshot, GameInstall, IgnoredUpdateSignature, ImportInspection,
        ImportResolution, ImportSource, LaunchBehavior, LibraryCategoryDisplayMode,
        LibraryGroupMode, LibrarySort, MOD_META_DIR, MetadataVisibility, ModCategory,
        ModCategorySortMode, ModEntry, ModSourceData, ModStatus, ModStatusTargets, ModUpdateState,
        ModifiedUpdateBehavior, OperationLogEntry, SearchSort, StagedAppUpdate, TaskEntry,
        TaskKind, TaskRetryPayload, TaskStatus, TasksLayout, TasksOrder, ToolEntry,
        TrackedFileMeta, UnsafeContentMode, default_modded_exe_candidates, default_mods_path,
        default_mods_path_from_launcher, default_unreal_bypasser_paths_from_exe,
        default_unreal_pak_mods_path_from_exe, default_vanilla_exe_candidates, feedback_survey,
        registry_modded_exe_candidates, registry_vanilla_exe_candidates,
        shortcut_modded_exe_candidates, vanilla_exe_file_names, xxmi_launcher_file_names,
    },
    persistence::{self, PortablePaths},
};

use self::content::{WHATS_NEW_DATE, WHATS_NEW_HIGHLIGHTS};

#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, RECT};

#[cfg(windows)]
use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_STIMEFORMAT};

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};

#[cfg(windows)]
use windows::core::PCWSTR;

include!("constants.rs");
include!("runtime.rs");
include!("i18n.rs");
include!("state.rs");
include!("actions/mod.rs");
include!("ui/mod.rs");
include!("workers/mod.rs");
include!("util/mod.rs");

impl eframe::App for HestiaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        set_current_language(self.state.static_prefs.language);

        // Batch worker event consumption - only poll channels when flagged
        if self.check_pending_worker_events() {
            self.consume_icon_results(ctx);
            self.consume_mod_image_results();
            self.consume_manual_image_events();
            self.consume_gif_preview_events(ctx);
            self.consume_gif_animation_events(ctx);
            self.consume_cover_results(ctx);
            self.consume_browse_events();
            self.consume_browse_image_results();
            self.consume_browse_download_events();
            self.consume_app_update_events();
            self.consume_proxy_apply_events();
            self.consume_feedback_survey_events();
            self.consume_update_check_results();
            self.consume_startup_path_scan_events(ctx);
            self.consume_startup_scan_events();
            self.handle_translation_events();
            self.consume_install_events();
            self.consume_refresh_events();
        }
        self.complete_startup_launch(ctx);

        // Always run these - they have internal checks or are always needed
        self.cancel_invisible_gif_work();
        self.update_gif_animations(ctx);
        self.last_visible_gif_texture_keys = std::mem::take(&mut self.visible_gif_texture_keys);
        self.visible_gif_process_texture_keys.clear();
        self.evict_textures_to_budget(ctx.input(|i| i.time));
        self.enforce_browse_page_timeout();
        self.detect_drag_and_drop(ctx);
        self.handle_shortcuts(ctx);

        // Process queues - only when there's work
        if self.check_pending_process_work() {
            self.process_local_mod_image_queue(ctx);
            self.process_pending_texture_uploads(ctx);
            self.ensure_browse_bootstrap();
            self.process_pending_browse_open(ctx);
            self.process_browse_image_queue(ctx);
            self.process_browse_download_queue();
            self.process_app_update_download();
            self.process_install_queue();
        }
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();

        // Render UI
        egui::CentralPanel::default().show(root_ui, |ui| {
            install_resize_handles(&ctx);

            self.render_top_bar(ui);
            self.render_settings_window(&ctx);
            self.render_nav_rail(ui);

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(24, 26, 29))
                        .outer_margin(egui::Margin {
                            left: 0,
                            right: WINDOW_INSET,
                            top: 0,
                            bottom: WINDOW_INSET,
                        }),
                )
                .show(ui, |ui| self.render_workspace_view(ui));

            self.render_tasks_window(&ctx);
            self.render_tools_window(&ctx);
            self.render_tool_launch_options_prompt(&ctx);
            self.render_whats_new_window(&ctx);
            self.render_feedback_survey_window(&ctx);
            self.render_log_panel(&ctx);
            self.render_startup_path_scan_overlay(&ctx);

            self.render_pending_conflict(&ctx);
            self.render_pending_import(&ctx);
            self.update_main_window_state(&ctx);
        });
        self.handle_window_close_shortcuts(&ctx);
        // Control repaint behavior to reduce CPU usage on idle
        // Only request continuous repaints when necessary
        let has_pending_browse_request = self.browse_state.loading_page
            || self.browse_state.character_categories_loading
            || !self.browse_state.loading_details.is_empty();
        let has_pending_browse_image_work = !self.browse_image_inflight.is_empty();
        let has_pending_gif_work =
            !self.pending_gif_previews.is_empty() || !self.pending_gif_animations.is_empty();
        let relative_time_visible =
            matches!(self.current_view, ViewMode::Library | ViewMode::Browse);
        let needs_continuous_repaint = self.app_update_download_inflight.is_some()
            || !self.browse_download_inflight.is_empty()
            || !self.install_inflight.is_empty()
            || self.reload_spin_until > ctx.input(|i| i.time)
            || self.app_update_button_spin_until > ctx.input(|i| i.time);

        if needs_continuous_repaint {
            ctx.request_repaint();
        } else if has_pending_browse_image_work || has_pending_gif_work {
            // Worker channels do not wake egui themselves. Poll while images/GIFs are in
            // flight so completed downloads/decodes are consumed without waiting for input.
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if has_pending_browse_request {
            // Worker channels do not wake egui themselves. Poll while a Browse request is in
            // flight so completed results are consumed even without user interaction.
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if relative_time_visible {
            // Relative-time labels advance at minute granularity. Wake once per minute rather
            // than continuously repainting while the app is idle.
            ctx.request_repaint_after(Duration::from_secs(60));
        } else if self.pending_events.has_worker_events || self.pending_events.has_process_work {
            ctx.request_repaint();
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 26, 29).to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel_all_gif_work();
    }
}

impl HestiaApp {
    fn handle_window_close_shortcuts(&mut self, ctx: &egui::Context) {
        let close_all_requested = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
                egui::Key::W,
            ))
        });
        if close_all_requested {
            self.close_all_noncritical_windows();
            return;
        }

        let critical_window_open = !self.pending_imports.is_empty()
            || !self.pending_conflicts.is_empty()
            || self.browse_state.file_prompt.is_some()
            || self.browse_state.screenshot_overlay.is_some();
        if critical_window_open {
            return;
        }

        let close_requested = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || input.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::CTRL,
                    egui::Key::W,
                ))
        });
        if close_requested {
            self.close_frontmost_window(ctx);
        }
    }

    fn close_frontmost_window(&mut self, ctx: &egui::Context) {
        let foreground_id = ctx.memory(|memory| {
            memory
                .areas()
                .top_layer_id(egui::Order::Foreground)
                .map(|layer| layer.id)
        });
        if foreground_id.is_some_and(|id| self.close_window_by_id(id)) {
            return;
        }

        let Some(id) = ctx.top_layer_id().map(|layer| layer.id) else {
            return;
        };

        self.close_window_by_id(id);
    }

    fn close_all_noncritical_windows(&mut self) {
        let mut save_state = false;

        if self.mod_detail_open {
            self.set_selected_mod_id(None);
        }
        if self.browse_detail_open {
            self.browse_detail_open = false;
            self.browse_state.selected_mod_id = None;
        }
        if self.state.show_whats_new {
            self.state.show_whats_new = false;
        }
        if self.state.show_feedback_survey {
            self.state.show_feedback_survey = false;
            save_state = true;
        }
        if self.state.show_tasks {
            self.state.show_tasks = false;
            save_state = true;
        }
        if self.state.show_tools {
            self.state.show_tools = false;
            save_state = true;
        }
        if self.settings_open {
            self.settings_open = false;
        }
        if self.state.show_log {
            self.state.show_log = false;
            save_state = true;
        }
        if self.tool_launch_options_prompt.is_some() {
            self.tool_launch_options_prompt = None;
        }

        if save_state {
            self.save_state();
        }
    }

    fn close_window_by_id(&mut self, id: egui::Id) -> bool {
        if let Some(prompt) = &self.tool_launch_options_prompt {
            if id == egui::Id::new(("tool_launch_options", prompt.tool_id.clone())) {
                self.tool_launch_options_prompt = None;
                return true;
            }
        }

        if id == egui::Id::new("mod_detail_window") && self.mod_detail_open {
            self.set_selected_mod_id(None);
            true
        } else if id == egui::Id::new(BROWSE_DETAIL_WINDOW_ID) && self.browse_detail_open {
            self.browse_detail_open = false;
            self.browse_state.selected_mod_id = None;
            true
        } else if id == egui::Id::new(("whats_new_window", self.whats_new_window_nonce))
            && self.state.show_whats_new
        {
            self.state.show_whats_new = false;
            true
        } else if id == egui::Id::new(("feedback_survey_window", self.feedback_survey_window_nonce))
            && self.state.show_feedback_survey
        {
            self.state.show_feedback_survey = false;
            self.save_state();
            true
        } else if id == egui::Id::new(("tasks_window", self.tasks_window_nonce))
            && self.state.show_tasks
        {
            self.state.show_tasks = false;
            self.save_state();
            true
        } else if id == egui::Id::new(("tools_window", self.tools_window_nonce))
            && self.state.show_tools
        {
            self.state.show_tools = false;
            self.save_state();
            true
        } else if id == egui::Id::new("settings_window") && self.settings_open {
            self.settings_open = false;
            true
        } else if id == egui::Id::new(("log_window", self.log_window_nonce)) && self.state.show_log
        {
            self.state.show_log = false;
            self.save_state();
            true
        } else {
            false
        }
    }
}
