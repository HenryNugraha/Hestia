#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod importing;
mod integrations;
mod manifest_cli;
mod model;
mod persistence;
#[cfg(feature = "profile")]
mod profiler;
mod renderer;

use anyhow::Context;
use eframe::icon_data;
use egui::{pos2, vec2};
use mimalloc::MiMalloc;
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

fn main() -> anyhow::Result<()> {
    let log_filter = EnvFilter::from_default_env().add_directive(
        "egui_winit::clipboard=off"
            .parse()
            .expect("valid log filter"),
    );
    let _ = fmt().with_env_filter(log_filter).try_init();

    #[cfg(feature = "profile")]
    profiler::init();

    if manifest_cli::try_run()? {
        return Ok(());
    }
    // A main-thread panic takes the process down without `on_exit`, which must not leave a
    // synthetic XXMI reload/hotkey press stuck (3DMigoto would keep reloading every frame).
    // Worker-thread panics unwind through the sender's own drop guard, so only the main
    // thread flips the shutdown flag here.
    {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("main") {
                integrations::xxmi_persist::release_synthetic_keys_for_shutdown();
            }
            default_hook(info);
        }));
    }
    let after_update_launch = std::env::args_os().any(|arg| arg == "--after-update");
    let after_proxy_restart = std::env::args_os().any(|arg| arg == "--after-proxy-restart");
    let after_elevated_restart = std::env::args_os().any(|arg| arg == "--after-elevated-restart");
    let skip_instance_guard = after_update_launch || after_proxy_restart || after_elevated_restart;

    let portable =
        persistence::PortablePaths::discover().context("failed to discover portable paths")?;
    portable.ensure_layout()?;

    let state =
        persistence::load_app_state(&portable).context("failed to load portable app state")?;
    let mut state = state;
    if app::apply_staged_app_update_before_gui(&portable, &mut state).unwrap_or(false) {
        return Ok(());
    }
    let _single_instance_guard = if skip_instance_guard {
        None
    } else {
        acquire_single_instance_guard()?
    };
    if _single_instance_guard.is_none() && !skip_instance_guard {
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
        .and_then(|g| g.mods_path(state.static_prefs.use_default_mods_path));
    let _ = persistence::cleanup_orphan_tmp_files(selected_mods_root.as_deref(), &HashSet::new());
    let icon_bytes = include_bytes!("asset/icon.png");
    let icon =
        icon_data::from_png_bytes(icon_bytes).context("failed to load app icon from icon.png")?;
    let custom_proxy = model::CustomProxyConfig::from_preferences(&state.static_prefs)
        .map_err(|err| anyhow::anyhow!("invalid custom proxy configuration: {err}"))?;
    let runtime_services =
        app::RuntimeServices::new(custom_proxy).context("failed to create runtime services")?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1540.0, 960.0])
        .with_min_inner_size([1180.0, 760.0])
        .with_decorations(false)
        .with_icon(icon)
        .with_title("Hestia");
    if state.static_prefs.window_maximized {
        viewport = viewport.with_visible(false);
    } else {
        if let Some([x, y]) = state.static_prefs.window_pos {
            viewport = viewport.with_position(pos2(x, y));
        }
        if let Some([w, h]) = state.static_prefs.window_size {
            viewport = viewport.with_inner_size(vec2(w, h));
        }
    }
    let renderer_selection = renderer::select(&portable, state.static_prefs.renderer);
    let auto_renderer_label = renderer_selection.auto_label;
    let renderer_boot_unconfirmed = renderer_selection.breadcrumb_pending;
    // eframe injects the display handle at instance creation time.
    let mut wgpu_setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    wgpu_setup.instance_descriptor.backends = renderer_selection.backends;
    // Rendering on a GPU other than the one that drives the window's monitor
    // costs a full cross-adapter copy every present, which pins both GPUs on a
    // multi-GPU box. So instead of a blanket power-preference hint (which
    // hardcoded the iGPU and produced exactly that copy whenever the window sat
    // on a dedicated-GPU display), pick the adapter ourselves: `Auto` matches
    // the display's GPU, and an explicit preference forces integrated/dedicated.
    let renderer_gpu = renderer_selection.gpu;
    let display_pci = if renderer_gpu == renderer::GpuTarget::MatchDisplay {
        let pos = if state.static_prefs.window_maximized {
            None
        } else {
            state
                .static_prefs
                .window_pos
                .map(|[x, y]| (x as i32, y as i32))
        };
        renderer::display_pci_for_window(pos)
    } else {
        None
    };
    wgpu_setup.native_adapter_selector = Some(std::sync::Arc::new(
        move |adapters: &[eframe::wgpu::Adapter],
              _surface: Option<&eframe::wgpu::Surface>|
              -> Result<eframe::wgpu::Adapter, String> {
            renderer::pick_adapter(adapters, renderer_gpu, display_pci)
                .ok_or_else(|| "no compatible GPU adapter found".to_string())
        },
    ));
    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        renderer: renderer_selection.renderer,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(wgpu_setup),
            ..Default::default()
        },
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
                runtime_services,
                startup_path_scan_due,
                auto_renderer_label,
                renderer_boot_unconfirmed,
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
