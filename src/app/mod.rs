pub(crate) mod content;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
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
use tokio::sync::{Semaphore, mpsc as tokio_mpsc};
use uuid::Uuid;
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    importing::{self, PreparedImport},
    integrations::{gamebanana, xxmi},
    model::{
        AfterInstallBehavior, AppFontStyle, AppLanguage, AppState, BrowseDownloadTaskFile,
        BrowseDownloadTaskPayload, BrowseSort, CacheSizeTier, ConflictChoice, DeleteBehavior,
        CustomProxyConfig,
        FileSetRecipe, GameBananaFileMeta, GameBananaLink, GameBananaSnapshot, GameInstall,
        IgnoredUpdateSignature, ImportInspection, ImportResolution, ImportSource, LaunchBehavior,
        LibraryCategoryDisplayMode, LibraryGroupMode, LibrarySort, MOD_META_DIR,
        MetadataVisibility, ModCategory, ModCategorySortMode, ModEntry, ModSourceData, ModStatus,
        ModStatusTargets, ModUpdateState, ModifiedUpdateBehavior, OperationLogEntry, SearchSort,
        StagedAppUpdate, TaskEntry, TaskKind, TaskRetryPayload, TaskStatus, TasksLayout,
        TasksOrder, ToolEntry, TrackedFileMeta, UnsafeContentMode, default_modded_exe_candidates,
        default_mods_path, default_mods_path_from_launcher, default_vanilla_exe_candidates,
        feedback_survey, registry_modded_exe_candidates, registry_vanilla_exe_candidates,
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            self.consume_feedback_survey_events();
            self.consume_update_check_results();
            self.consume_startup_path_scan_events(ctx);
            self.consume_startup_scan_events();
            self.handle_translation_events();
            self.consume_install_events();
            self.consume_refresh_events();
        }
        
        // Always run these - they have internal checks or are always needed
        self.update_gif_animations(ctx);
        self.evict_textures_to_budget(ctx.input(|i| i.time));
        self.enforce_browse_page_timeout();
        self.detect_drag_and_drop(ctx);
        self.handle_shortcuts(ctx);
        
        // Process queues - only when there's work
        if self.check_pending_process_work() {
            self.process_local_mod_image_queue();
            self.process_pending_texture_uploads(ctx);
            self.ensure_browse_bootstrap();
            self.process_pending_browse_open(ctx);
            self.process_browse_image_queue();
            self.process_browse_download_queue();
            self.process_app_update_download();
            self.process_install_queue();
        }
        
        // Render UI
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_window_background(ctx);
            install_resize_handles(ctx);
            
            self.render_top_bar(ui);
            self.render_settings_window(ctx);
            self.render_nav_rail(ui);

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(24, 26, 29, 242))
                        .outer_margin(egui::Margin {
                            left: 0,
                            right: WINDOW_INSET,
                            top: 0,
                            bottom: WINDOW_INSET,
                        }),
                )
                .show_inside(ui, |ui| self.render_workspace_view(ui));

            self.render_tasks_window(ctx);
            self.render_tools_window(ctx);
            self.render_tool_launch_options_prompt(ctx);
            self.render_whats_new_window(ctx);
            self.render_feedback_survey_window(ctx);
            self.render_log_panel(ctx);
            paint_window_frame(ctx);
            self.render_startup_path_scan_overlay(ctx);

            self.render_pending_conflict(ctx);
            self.render_pending_import(ctx);
            self.update_main_window_state(ctx);
        });
        
        // Control repaint behavior to reduce CPU usage on idle
        // Only request continuous repaints when necessary
        let has_pending_browse_request = self.browse_state.loading_page
            || self.browse_state.character_categories_loading
            || !self.browse_state.loading_details.is_empty();
        let relative_time_visible = matches!(self.current_view, ViewMode::Library | ViewMode::Browse);
        let needs_continuous_repaint = 
            !self.animated_gif_state.is_empty()
            || self.app_update_download_inflight.is_some()
            || !self.browse_download_inflight.is_empty()
            || !self.install_inflight.is_empty()
            || self.reload_spin_until > ctx.input(|i| i.time)
            || self.app_update_button_spin_until > ctx.input(|i| i.time);
        
        if needs_continuous_repaint {
            ctx.request_repaint();
        } else if has_pending_browse_request {
            // Worker channels do not wake egui themselves. Poll while a Browse request is in
            // flight so completed results are consumed even without user interaction.
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if relative_time_visible {
            // Relative-time labels advance at minute granularity. Wake once per minute rather
            // than continuously repainting while the app is idle.
            ctx.request_repaint_after(Duration::from_secs(60));
        } else {
            // On idle, only repaint when there's actual interaction INSIDE the window
            // This prevents repaints from mouse movement outside the window
            let has_interaction = ctx.input(|i| {
                i.pointer.any_pressed()
                    || i.pointer.any_released()
                    || !i.events.is_empty()
                    || (i.pointer.is_moving() && i.pointer.hover_pos().is_some())
            });
            
            if !has_interaction {
                // Check if we have pending work that needs processing
                if self.pending_events.has_worker_events || self.pending_events.has_process_work {
                    ctx.request_repaint();
                }
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 26, 29).to_normalized_gamma_f32()
    }
}
