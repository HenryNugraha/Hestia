#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod importing;
mod integrations;
mod manifest_cli;
mod model;
mod persistence;
#[cfg(feature = "profile")]
mod profiler;

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
    let after_update_launch = std::env::args_os().any(|arg| arg == "--after-update");
    let after_proxy_restart = std::env::args_os().any(|arg| arg == "--after-proxy-restart");

    let portable =
        persistence::PortablePaths::discover().context("failed to discover portable paths")?;
    portable.ensure_layout()?;

    let state =
        persistence::load_app_state(&portable).context("failed to load portable app state")?;
    let mut state = state;
    if app::apply_staged_app_update_before_gui(&portable, &mut state).unwrap_or(false) {
        return Ok(());
    }
    let _single_instance_guard = if after_update_launch || after_proxy_restart {
        None
    } else {
        acquire_single_instance_guard()?
    };
    if _single_instance_guard.is_none() && !after_update_launch && !after_proxy_restart {
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
    let runtime_services = app::RuntimeServices::new(custom_proxy)
        .context("failed to create runtime services")?;
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
    let (renderer, wgpu_backends, auto_renderer_label) =
        select_renderer(state.static_prefs.renderer);
    // eframe injects the display handle at instance creation time.
    let mut wgpu_setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    wgpu_setup.instance_descriptor.backends = wgpu_backends;
    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        renderer,
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
            )))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

/// Renderer and wgpu backend selection. Priority: `HESTIA_RENDERER` /
/// `WGPU_BACKEND` env overrides, then the user's settings preference, then
/// Auto.
///
/// wgpu is preferred over glow because eframe's glow backend rebinds the GL
/// context twice per frame, and on Windows `wglMakeCurrent` flushes the
/// pipeline — ~5 ms of CPU per repaint, most of the frame cost whenever the
/// cursor moves over the window (egui #4173). Auto forces DX12 on Windows:
/// wgpu otherwise tends to pick Vulkan, whose swapchain bypasses DWM
/// flip-model presentation and costs ~4x the GPU time per present (measured
/// 11% vs 3% GPU while repainting maximized at 2560x1440).
///
/// An explicit preference whose backend has no hardware adapter falls back to
/// Auto so the app still starts. Auto itself falls back to glow when wgpu has
/// no hardware adapter at all (pre-DX12 boxes, GPU-less VMs): wgpu's software
/// rasterizer would be slower there than glow on real hardware GL.
fn select_renderer(
    pref: model::RendererPreference,
) -> (eframe::Renderer, eframe::wgpu::Backends, &'static str) {
    use eframe::wgpu;
    use model::RendererPreference;

    let env_backends = wgpu::Backends::from_env();
    let auto_backends = env_backends.unwrap_or(if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::PRIMARY | wgpu::Backends::GL
    });

    let instance = wgpu::Instance::default();
    let find_hardware_adapter = |backends: wgpu::Backends| {
        pollster::block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .find(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu)
    };
    // What Auto would run on this machine. Probed even when the preference is
    // explicit: settings compares it against the active renderer so the restart
    // button only appears for a selection that would actually change something.
    let auto_adapter = find_hardware_adapter(auto_backends);
    let auto_label = match auto_adapter.as_ref().map(|adapter| adapter.get_info().backend) {
        Some(wgpu::Backend::Dx12) => "DirectX 12",
        Some(wgpu::Backend::Vulkan) => "Vulkan",
        Some(wgpu::Backend::Metal) => "Metal",
        Some(wgpu::Backend::Gl) => "OpenGL (wgpu)",
        Some(_) => "wgpu",
        None => "OpenGL",
    };

    match std::env::var("HESTIA_RENDERER").as_deref() {
        Ok("glow") => return (eframe::Renderer::Glow, auto_backends, auto_label),
        Ok("wgpu") => return (eframe::Renderer::Wgpu, auto_backends, auto_label),
        _ => {}
    }

    let pref = if pref.valid_on_current_platform() {
        pref
    } else {
        RendererPreference::Auto
    };
    if pref == RendererPreference::OpenGl {
        return (eframe::Renderer::Glow, auto_backends, auto_label);
    }
    let requested_backends = if env_backends.is_some() {
        auto_backends
    } else {
        match pref {
            RendererPreference::Dx12 => wgpu::Backends::DX12,
            RendererPreference::Vulkan => wgpu::Backends::VULKAN,
            RendererPreference::Metal => wgpu::Backends::METAL,
            _ => auto_backends,
        }
    };

    // Try the requested backend first, then Auto's choice; reuse the Auto probe
    // when they are the same set.
    let candidates = if requested_backends == auto_backends {
        vec![(auto_backends, auto_adapter)]
    } else {
        let requested_adapter = find_hardware_adapter(requested_backends);
        vec![
            (requested_backends, requested_adapter),
            (auto_backends, auto_adapter),
        ]
    };
    for (backends, adapter) in candidates {
        if let Some(adapter) = adapter {
            let info = adapter.get_info();
            tracing::info!("using wgpu renderer: {} via {:?}", info.name, info.backend);
            return (eframe::Renderer::Wgpu, backends, auto_label);
        }
        tracing::warn!("no hardware wgpu adapter for {backends:?}");
    }
    tracing::warn!("no hardware wgpu adapter found; falling back to glow renderer");
    (eframe::Renderer::Glow, auto_backends, auto_label)
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
