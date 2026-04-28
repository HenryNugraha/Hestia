use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Local, Utc};
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, RichText, ScrollArea, Sense,
    TextEdit, TextFormat, Ui, Vec2,
};
use fast_image_resize as fir;
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use regex::Regex;
use rfd::FileDialog;
use uuid::Uuid;
use walkdir::WalkDir;
use once_cell::sync::Lazy;
use xxhash_rust::xxh3::xxh3_64;
use tokio::sync::{Semaphore, mpsc as tokio_mpsc};
use reqwest_middleware::{ClientBuilder as MiddlewareClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use futures_util::StreamExt;

use crate::{
    integrations::{gamebanana, xxmi},
    importing::{self, PreparedImport},
    model::{
        default_modded_exe_candidates, default_vanilla_exe_candidates, AppState, ConflictChoice,
        GameInstall, ImportSource, ModEntry, ModStatus, OperationLogEntry, LaunchBehavior,
        ImportResolution, DeleteBehavior, ImportInspection, TasksLayout, TasksOrder,
        TaskEntry, TaskKind, TaskStatus, ToolEntry, AfterInstallBehavior,
        UnsafeContentMode, CacheSizeTier, MetadataVisibility, ModSourceData, GameBananaLink,
        GameBananaSnapshot, GameBananaFileMeta, FileSetRecipe, TrackedFileMeta,
        IgnoredUpdateSignature, LibraryGroupMode, LibrarySort, ModCategory,
        ModUpdateState, ModStatusTargets, ModifiedUpdateBehavior, StagedAppUpdate, MOD_META_DIR, BrowseSort, SearchSort,
    },
    persistence::{self, PortablePaths},
};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_STIMEFORMAT};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::core::PCWSTR;

include!("constants.rs");
include!("runtime.rs");
include!("state.rs");
include!("actions/mod.rs");
include!("ui/mod.rs");
include!("workers/mod.rs");
include!("util/mod.rs");

impl eframe::App for HestiaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        paint_window_background(ctx);
        install_resize_handles(ctx);
        self.consume_icon_results(ctx);
        self.consume_mod_image_results();
        self.consume_gif_preview_events(ctx);
        self.consume_gif_animation_events(ctx);
        self.update_gif_animations(ctx);
        self.consume_cover_results(ctx);
        self.consume_browse_events();
        self.consume_browse_image_results();
        self.consume_browse_download_events();
        self.consume_app_update_events();
        self.consume_update_check_results();
        self.consume_startup_scan_events();
        self.process_local_mod_image_queue();
        self.process_pending_texture_uploads(ctx);
        self.evict_textures_to_budget(ctx.input(|i| i.time));
        self.detect_drag_and_drop(ctx);
        self.consume_install_events();
        self.consume_refresh_events();
        self.ensure_browse_bootstrap();
        self.process_pending_browse_open(ctx);
        self.process_browse_image_queue();
        self.process_browse_download_queue();
        self.process_app_update_download();
        self.process_install_queue();
        self.handle_shortcuts(ctx);
        self.render_top_bar(ctx);
        self.render_settings_window(ctx);
        self.render_nav_rail(ctx);

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
            .show(ctx, |ui| self.render_workspace_view(ui));

        self.render_tasks_window(ctx);
        self.render_tools_window(ctx);
        self.render_tool_launch_options_prompt(ctx);
        self.render_log_panel(ctx);
        paint_window_frame(ctx);

        self.render_pending_conflict(ctx);
        self.render_pending_import(ctx);
        self.update_main_window_state(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 26, 29).to_normalized_gamma_f32()
    }
}

