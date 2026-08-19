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
    integrations::{gamebanana, profiles, unrealengine, xxmi, xxmi_persist},
    model::{
        AfterInstallBehavior, AppFontStyle, AppLanguage, AppState, BrowseDownloadTaskFile,
        BrowseDownloadTaskPayload, BrowseSort, CacheSizeTier, ConflictChoice, CustomProxyConfig,
        DeleteBehavior, FileSetRecipe, GameBackend, GameBananaFileMeta, GameBananaLink,
        GameBananaSnapshot, GameInstall, IgnoredUpdateSignature, ImportInspection,
        ImportResolution, ImportSource, LaunchBehavior, LibraryCategoryDisplayMode,
        LibraryGroupMode, LibrarySort, MOD_META_DIR, MetadataSourceKind, ModCategory,
        ModCategorySortMode, ModEntry, ModSourceData, ModStatus, ModStatusTargets, ModUpdateState,
        ModifiedUpdateBehavior, OperationLogEntry, ProfileCatalog, ProfileId, ProfileRecord,
        ReloadHotkeyTrigger, RendererPreference, SearchSort, StagedAppUpdate, TaskEntry, TaskKind,
        TaskRetryPayload, TaskStatus, TasksLayout, TasksOrder, ToolEntry, TrackedFileMeta,
        UnsafeContentMode, default_modded_exe_candidates, default_mods_path,
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

/// Wake the UI event loop so a freshly sent worker event is consumed on the next
/// frame instead of waiting for a poll tick. Safe to call from any thread; no-op
/// until the GUI is up.
fn wake_ui() {
    if let Some(ctx) = UI_REPAINT_CONTEXT.get() {
        ctx.request_repaint();
    }
}

/// Continuous repaints are for visible animation only. When the window is not
/// focused, present at a bounded rate instead: an occluded window's buffer swap may
/// not block on vsync, and the resulting present spam contends with a foreground
/// game's frame pacing even while overall CPU/GPU usage stays low.
fn request_animation_repaint(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
        ctx.request_repaint();
    } else {
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

impl eframe::App for HestiaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        profiling::scope!("app::logic");
        set_current_language(self.state.static_prefs.language);

        // Batch worker event consumption - only poll channels when flagged
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
            self.consume_startup_path_scan_events(ctx);
            self.consume_startup_scan_events();
            self.handle_translation_events();
            self.consume_install_events();
            self.consume_refresh_events();
            self.consume_xxmi_reload_events();
            self.consume_hotkey_customization_events();
            self.consume_profile_events();
        }
        self.complete_startup_launch(ctx);

        // Always run these - they have internal checks or are always needed
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
        if !self.profile_operation_locks_app() {
            self.detect_drag_and_drop(ctx);
            self.handle_shortcuts(ctx);
        }

        // Process queues - only when there's work
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

        // Sampled after the queues have been serviced so the end of `ui` can tell
        // whether rendering added work of its own.
        self.image_queue_len_after_logic = self.pending_mod_image_queue.len();
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        profiling::scope!("app::ui");
        let ctx = root_ui.ctx().clone();

        // Sampled before any UI runs: popups close themselves on Escape *during*
        // rendering (without consuming the key), so by end of frame the memory
        // already reports no open popup and the close-window handler below would
        // also fire on the same key press.
        let popup_open_at_frame_start = egui::Popup::is_any_open(&ctx);
        let profile_operation_locks_app = self.profile_operation_locks_app();
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

        // Render UI
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
                // Blocking profile operations are modal and must stay above every
                // other Hestia window and overlay.
                self.render_profile_dialogs(&ctx);
                self.update_main_window_state(&ctx);
            }
        });
        self.handle_window_close_shortcuts(&ctx, popup_open_at_frame_start);
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

        // Worker sends wake the UI directly (see wake_ui), so these polls are a safety
        // net for work that can no longer report back, not the primary wake path. Poll
        // gently while the window is unfocused: background presents contend with a
        // foreground game's frame pacing.
        let poll_delay = if ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(500)
        };
        // `logic` runs before `ui`, so image requests the UI just queued are not
        // dispatched until another frame happens. Nothing else asks for that frame:
        // a freshly installed mod's card would sit on the placeholder until the
        // next idle tick or the next stray input event. Growth is the trigger, not
        // mere non-emptiness — a queue held back by focus mode stays full frame
        // after frame and must not drive repaints at vsync rate.
        let queued_new_image_work =
            self.pending_mod_image_queue.len() > self.image_queue_len_after_logic;
        // `pending_events` was sampled in `logic`, before the UI could add to these.
        let has_undispatched_work = !self.pending_mod_image_queue.is_empty()
            || !self.pending_texture_uploads.is_empty()
            || self.pending_events.has_worker_events
            || self.pending_events.has_process_work;
        if needs_continuous_repaint {
            request_animation_repaint(&ctx);
        } else if queued_new_image_work {
            ctx.request_repaint();
        } else if has_pending_browse_image_work || has_pending_gif_work {
            ctx.request_repaint_after(poll_delay);
        } else if has_pending_browse_request {
            ctx.request_repaint_after(poll_delay);
        } else if has_undispatched_work {
            // A queue that cannot drain must never drive vsync-rate repaints, which
            // contend with fullscreen games even at low CPU/GPU usage.
            ctx.request_repaint_after(poll_delay);
        } else if relative_time_visible {
            // Relative-time labels advance at minute granularity. Wake once per minute rather
            // than continuously repainting while the app is idle. Checked last: this
            // fires in every library and browse frame, so ahead of the branches above
            // it would swallow their much shorter delays.
            ctx.request_repaint_after(Duration::from_secs(60));
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

        // While a popup/context menu is open, Escape belongs to it: egui closes
        // the popup itself, so leave the key alone and keep the window open.
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
