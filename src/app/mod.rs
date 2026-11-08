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
    integrations::{gamebanana, profiles, thalia, unrealengine, xxmi, xxmi_persist},
    model::{
        AcceptedLocalChanges, AfterInstallBehavior, AppFontStyle, AppLanguage, AppState,
        BrowseDownloadTaskFile, BrowseDownloadTaskPayload, BrowseSort, CacheSizeTier,
        ConflictChoice, CustomProxyConfig, DeleteBehavior, FileSetRecipe, FloatingWindowLayouts,
        FloatingWindowRect, GameBackend, GameBananaFileMeta, GameBananaLink, GameBananaSnapshot,
        GameInstall, GpuChoice, IgnoredUpdateSignature, ImportInspection, ImportResolution,
        ImportSource, InterfaceSize, LaunchBehavior, LibraryCategoryDisplayMode, LibraryGroupMode,
        LibrarySort, MOD_META_DIR, MetadataSourceKind, ModCategory, ModCategorySortMode, ModEntry,
        ModInstallState, ModSourceData, ModStatus, ModStatusTargets, ModUpdateState,
        ModifiedUpdateBehavior, OperationLogEntry, OverlayHotkey, OverlayHotkeyProblem,
        ProfileCatalog, ProfileId, ProfileRecord, RefusedThaliaAnswer, RefusedThaliaRequiredFile,
        ReloadHotkeyTrigger, RendererPreference,
        SearchSort, StagedAppUpdate, TaskEntry, TaskKind, TaskRetryPayload, TaskStatus,
        TasksLayout, TasksOrder, ToolEntry, TrackedFileMeta, UnsafeContentMode,
        UnsafeContentPreference, default_modded_exe_candidates, default_mods_path,
        default_mods_path_from_launcher, default_unreal_bypasser_paths_from_exe,
        default_unreal_pak_mods_path_from_exe, default_vanilla_exe_candidates, feedback_survey,
        path_allows_dir_creation, registry_modded_exe_candidates, registry_vanilla_exe_candidates,
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
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_C, VK_CONTROL, VK_V};

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

static UI_REPAINT_CONTEXT: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

fn wake_ui() {
    if let Some(ctx) = UI_REPAINT_CONTEXT.get() {
        ctx.request_repaint();
    }
}

fn request_animation_repaint(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
        ctx.request_repaint();
    } else {
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn logic_time() -> f64 {
    static START: Lazy<Instant> = Lazy::new(Instant::now);
    START.elapsed().as_secs_f64()
}

impl eframe::App for HestiaApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        profiling::scope!("app::logic");
        set_current_language(self.state.static_prefs.language);
        self.poll_folder_batch_job();

        if self.check_pending_worker_events() {
            profiling::scope!("logic::worker_events");
            self.consume_icon_results(ctx);
            self.consume_mod_image_results();
            self.consume_manual_image_events();
            self.consume_overlay_copy_events();
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
            self.consume_thalia_matching_results();
            self.consume_startup_path_scan_events(ctx);
            self.consume_startup_scan_events();
            self.handle_translation_events();
            self.consume_install_events();
            self.consume_refresh_events();
            self.consume_xxmi_reload_events();
            self.consume_grant_access_events();
            self.consume_hotkey_customization_events();
            self.consume_profile_events();
        }
        self.complete_startup_launch(ctx);
        self.confirm_renderer_boot(ctx);

        profiling::scope!("logic::frame_upkeep");
        self.cancel_invisible_gif_work();
        self.update_gif_animations(ctx);
        self.last_visible_gif_texture_keys = std::mem::take(&mut self.visible_gif_texture_keys);
        self.visible_gif_process_texture_keys.clear();
        self.evict_textures_to_budget(ctx.input(|i| i.time));
        self.enforce_browse_page_timeout();
        self.enforce_browse_request_timeouts();
        self.enforce_browse_image_timeouts();
        self.enforce_gif_work_timeouts();
        self.poll_live_state_watch(ctx);
        self.poll_d3dx_reload_config_watch(ctx);
        self.poll_game_overlay(ctx, frame);
        let window_shown = ctx.input(|input| input.viewport().visible().unwrap_or(true));
        if window_shown && !self.profile_operation_locks_app() {
            self.detect_drag_and_drop(ctx);
            self.handle_shortcuts(ctx);
        }

        if self.check_pending_process_work() {
            profiling::scope!("logic::queues");
            self.process_local_mod_image_queue(ctx);
            self.process_pending_texture_uploads(ctx);
            self.ensure_browse_bootstrap();
            self.process_pending_browse_open(ctx);
            self.process_browse_image_queue(ctx);
            self.process_browse_download_queue();
            self.process_app_update_download();
            self.process_install_queue();
        }

        self.image_queue_len_after_logic = self.pending_mod_image_queue.len();
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        profiling::scope!("app::ui");
        let ctx = root_ui.ctx().clone();

        let popup_open_at_frame_start = egui::Popup::is_any_open(&ctx);
        let profile_operation_locks_app = self.profile_operation_locks_app();
        if self.folder_batch_job.is_some() && ctx.input(|input| input.viewport().close_requested())
        {
            self.cancel_folder_batch();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if profile_operation_locks_app {
            if ctx.input(|input| input.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            ctx.memory_mut(|memory| memory.stop_text_input());
            ctx.input_mut(|input| {
                input.raw.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Copy
                            | egui::Event::Cut
                            | egui::Event::Paste(_)
                            | egui::Event::Text(_)
                            | egui::Event::Key { .. }
                            | egui::Event::Ime(_)
                    )
                });
                input.keys_down.clear();
                input.smooth_scroll_delta = Vec2::ZERO;
            });
        }

        egui::CentralPanel::default().show(root_ui, |ui| {
            install_resize_handles(&ctx);

            {
                profiling::scope!("render_top_bar");
                self.render_top_bar(ui);
            }
            {
                profiling::scope!("render_nav_rail");
                self.render_nav_rail(ui);
            }

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
                .show(ui, |ui| {
                    profiling::scope!("render_workspace_view");
                    self.render_workspace_view(ui)
                });

            self.render_right_pane_window_scrim(&ctx);

            {
                profiling::scope!("floating_windows");
                self.render_settings_window(&ctx);
                self.render_d3dx_foreground_conflict_prompt(&ctx);
                self.render_tasks_window(&ctx);
                self.render_tools_window(&ctx);
                self.render_tool_launch_options_prompt(&ctx);
                self.render_whats_new_window(&ctx);
                self.render_feedback_survey_window(&ctx);
                self.render_log_panel(&ctx);
                self.render_startup_path_scan_overlay(&ctx);
            }

            {
                profiling::scope!("dialogs_and_window_state");
                self.render_pending_conflict(&ctx);
                self.render_pending_import(&ctx);
                self.render_game_overlay_file_prompt(&ctx);
                self.render_profile_dialogs(&ctx);
                self.handle_interface_zoom_keys(&ctx);
                self.update_main_window_state(&ctx);
            }
        });
        self.handle_window_close_shortcuts(&ctx, popup_open_at_frame_start);
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

        let poll_delay = if ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(500)
        };
        let queued_new_image_work =
            self.pending_mod_image_queue.len() > self.image_queue_len_after_logic;
        let has_undispatched_work = !self.pending_mod_image_queue.is_empty()
            || !self.pending_texture_uploads.is_empty()
            || self.pending_events.has_worker_events
            || self.pending_events.has_process_work;
        if needs_continuous_repaint {
            request_animation_repaint(&ctx);
        } else if queued_new_image_work {
            ctx.request_repaint();
        } else if self.folder_batch_job.is_some()
            || has_pending_browse_image_work
            || has_pending_gif_work
            || has_pending_browse_request
            || has_undispatched_work
        {
            ctx.request_repaint_after(poll_delay);
        } else if relative_time_visible {
            ctx.request_repaint_after(Duration::from_secs(60));
        }
        if let Some(remaining) = self.thalia_matching.next_hold_expiry() {
            ctx.request_repaint_after(remaining);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 26, 29).to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.invalidate_thalia_matching();
        crate::integrations::xxmi_persist::release_synthetic_keys_for_shutdown();
        self.shut_down_game_overlay();
        self.cancel_all_gif_work();
        if self.floating_window_save_due.take().is_some() {
            self.save_state();
        }
        if self.renderer_boot_unconfirmed {
            self.renderer_boot_unconfirmed = false;
            crate::renderer::confirm_boot(&self.portable);
        }
    }
}

impl HestiaApp {
    fn handle_window_close_shortcuts(&mut self, ctx: &egui::Context, popup_was_open: bool) {
        if self.profile_operation_locks_app() {
            return;
        }

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

        let escape_requested = !popup_was_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let ctrl_w_requested = ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::CTRL,
                egui::Key::W,
            ))
        });
        if escape_requested || ctrl_w_requested {
            let closed = self.close_frontmost_window(ctx);
            if !closed && escape_requested && !self.selected_mods.is_empty() {
                self.selected_mods.clear();
            } else if !closed && escape_requested && self.library_folder_selection_active() {
                self.clear_library_folder_selection();
            }
        }
    }

    fn close_frontmost_window(&mut self, ctx: &egui::Context) -> bool {
        let foreground_id = ctx.memory(|memory| {
            memory
                .areas()
                .top_layer_id(egui::Order::Foreground)
                .map(|layer| layer.id)
        });
        if foreground_id.is_some_and(|id| self.close_window_by_id(id)) {
            return true;
        }

        let Some(id) = ctx.top_layer_id().map(|layer| layer.id) else {
            return false;
        };

        self.close_window_by_id(id)
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

        if id == egui::Id::new(("mod_detail_window", self.mod_detail_window_nonce))
            && self.mod_detail_open
        {
            self.set_selected_mod_id(None);
            true
        } else if id == egui::Id::new((BROWSE_DETAIL_WINDOW_ID, self.browse_detail_window_nonce))
            && self.browse_detail_open
        {
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
        } else if id == egui::Id::new(("settings_window", self.settings_window_nonce))
            && self.settings_open
        {
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
