#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod importing;
mod integrations;
mod manifest_cli;
mod model;
mod persistence;

use anyhow::Context;
use eframe::icon_data;
use egui::{pos2, vec2};
use mimalloc::MiMalloc;
use model::{SurveyAnswer, SurveyQuestion};
use std::collections::HashSet;
use tracing_subscriber::{EnvFilter, fmt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Generate via terminal with:
// >hestia.exe --public-key
pub(crate) const UPDATE_MANIFEST_PUBLIC_KEY_BASE64: &str =
    "TIoMuHl5kBva4HJ9NbagA3vOR1L5jJFokESKJGPGah0=";

// Generate via terminal with:
// >hestia.exe --manifest
pub(crate) const UPDATE_MANIFEST_URL: &[&str] = &[
    "https://hestia.hnawc.com/manifest/v1/latest.json",
    "https://raw.githubusercontent.com/HenryNugraha/Hestia/main/manifest.json",
];

pub(crate) const WHATS_NEW_DATE: &str = "xx June 2026";
pub(crate) const WHATS_NEW_HIGHLIGHTS: &[&str] = &["see changelog"];

pub(crate) const FEEDBACK_SURVEY_ENABLED: bool = true;
pub(crate) const FEEDBACK_SURVEY_QUESTIONS: &[SurveyQuestion] = &[
    SurveyQuestion {
        id: "new_category_in_settings",
        prompt: "Did you find the new Category settings useful?",
        answers: &[
            SurveyAnswer {
                id: 1,
                label: "Yes",
            },
            SurveyAnswer { id: 2, label: "No" },
            SurveyAnswer {
                id: 3,
                label: "Haven't used it",
            },
        ],
    },
    SurveyQuestion {
        id: "path_detection",
        prompt: "Have you ever had an issue with Hestia detecting paths?",
        answers: &[
            SurveyAnswer {
                id: 1,
                label: "Yes",
            },
            SurveyAnswer { id: 2, label: "No" },
        ],
    },
];
pub(crate) const FEEDBACK_SURVEY_MESSAGE_LABEL: &str = "Anything you'd like to say?";

fn main() -> anyhow::Result<()> {
    let log_filter = EnvFilter::from_default_env().add_directive(
        "egui_winit::clipboard=off"
            .parse()
            .expect("valid log filter"),
    );
    let _ = fmt().with_env_filter(log_filter).try_init();

    if manifest_cli::try_run()? {
        return Ok(());
    }
    let after_update_launch = std::env::args_os().any(|arg| arg == "--after-update");

    let portable =
        persistence::PortablePaths::discover().context("failed to discover portable paths")?;
    portable.ensure_layout()?;

    let state =
        persistence::load_app_state(&portable).context("failed to load portable app state")?;
    let mut state = state;
    if app::apply_staged_app_update_before_gui(&portable, &mut state).unwrap_or(false) {
        return Ok(());
    }
    let _single_instance_guard = if after_update_launch {
        None
    } else {
        acquire_single_instance_guard()?
    };
    if _single_instance_guard.is_none() && !after_update_launch {
        return Ok(());
    }
    let feedback_survey_changed = state.prepare_feedback_survey_on_launch(model::feedback_survey());
    if state.show_whats_new
        || state.show_feedback_survey
        || state.preferences_need_save
        || feedback_survey_changed
    {
        persistence::save_app_state(&portable, &state)
            .context("failed to save normalized app preferences")?;
        state.preferences_need_save = false;
    }
    if app::HestiaApp::auto_detect_game_paths(&mut state) {
        persistence::save_app_state(&portable, &state)
            .context("failed to save auto-detected game paths")?;
    }
    let startup_path_scan_due = !state.startup_path_scan_completed;
    persistence::load_history(&portable, &mut state).context("failed to load persisted history")?;
    let selected_mods_root = state
        .last_selected_game_id
        .as_ref()
        .and_then(|id| state.games.iter().find(|g| g.definition.id == *id))
        .and_then(|g| g.mods_path(state.use_default_mods_path));
    let _ = persistence::cleanup_orphan_tmp_files(selected_mods_root.as_deref(), &HashSet::new());
    let icon_bytes = include_bytes!("asset/icon.png");
    let icon =
        icon_data::from_png_bytes(icon_bytes).context("failed to load app icon from icon.png")?;
    let runtime_services =
        app::RuntimeServices::new().context("failed to create runtime services")?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1540.0, 960.0])
        .with_min_inner_size([1180.0, 760.0])
        .with_decorations(false)
        .with_icon(icon)
        .with_title("Hestia");
    if state.window_maximized {
        viewport = viewport.with_visible(false);
    } else {
        if let Some([x, y]) = state.window_pos {
            viewport = viewport.with_position(pos2(x, y));
        }
        if let Some([w, h]) = state.window_size {
            viewport = viewport.with_inner_size(vec2(w, h));
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "Hestia",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::HestiaApp::new(
                cc,
                portable.clone(),
                state,
                runtime_services.clone(),
                startup_path_scan_due,
            )))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

#[cfg(windows)]
fn acquire_single_instance_guard() -> anyhow::Result<Option<windows::Win32::Foundation::HANDLE>> {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::w;

    let handle =
        unsafe { CreateMutexW(None, true, w!("Local\\Hestia-Mod-Manager-Single-Instance")) }
            .context("failed to create single-instance mutex")?;
    let last_error = unsafe { GetLastError() };
    if last_error == ERROR_ALREADY_EXISTS {
        Ok(None)
    } else {
        Ok(Some(handle))
    }
}

#[cfg(not(windows))]
fn acquire_single_instance_guard() -> anyhow::Result<Option<()>> {
    Ok(Some(()))
}
