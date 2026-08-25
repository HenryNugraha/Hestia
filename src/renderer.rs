//! Renderer and wgpu backend selection, plus the boot breadcrumb that demotes a
//! renderer which dies before it can put frames on screen.
//!
//! wgpu is preferred over glow because eframe's glow backend rebinds the GL
//! context twice per frame, and on Windows `wglMakeCurrent` flushes the
//! pipeline: ~5 ms of CPU per repaint, most of the frame cost whenever the
//! cursor moves over the window (egui #4173). Windows starts the ladder at
//! DX12 because wgpu otherwise tends to pick Vulkan, whose swapchain bypasses
//! DWM flip-model presentation and costs ~4x the GPU time per present
//! (measured 11% vs 3% GPU while repainting maximized at 2560x1440).
//!
//! Selection walks a per-platform ladder and takes the first rung with a
//! hardware adapter, so a machine without the preferred backend degrades one
//! step at a time instead of dropping straight to glow. Each rung is probed
//! with its own narrowly scoped `wgpu::Instance`, and probing stops at the
//! first hit: the common Windows path never loads the Vulkan or GL loader,
//! which keeps startup cheap and keeps overlay injectors (RTSS, Afterburner,
//! Discord, XXMI) out of a code path we do not use.

use crate::model::RendererPreference;
use crate::persistence::{self, PortablePaths};
use eframe::wgpu;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Sits next to `hestia.toml`, so a portable install keeps its own record.
const BREADCRUMB_FILE: &str = "hestia-renderer.toml";

/// Number of completed egui passes that counts as "this renderer works". Frames
/// are presented after the pass that built them, so reaching pass 3 means at
/// least two presents survived the driver.
const PASSES_TO_CONFIRM: u64 = 3;

/// One rung of the fallback ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rung {
    /// Stable id recorded in the breadcrumb file. Never rename these.
    id: &'static str,
    /// Fallback API name, used when no adapter was probed (glow) or the probed
    /// backend is unrecognized.
    label: &'static str,
    renderer: eframe::Renderer,
    /// Backends the wgpu instance is restricted to. Unused by the glow rung.
    backends: wgpu::Backends,
}

const DX12: Rung = Rung {
    id: "dx12",
    label: "DirectX 12",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::DX12,
};

const VULKAN: Rung = Rung {
    id: "vulkan",
    label: "Vulkan",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::VULKAN,
};

const METAL: Rung = Rung {
    id: "metal",
    label: "Metal",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::METAL,
};

const WGPU_GL: Rung = Rung {
    id: "wgpu-gl",
    label: "OpenGL (wgpu)",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::GL,
};

/// Terminal rung: no adapter probe, always accepted. eframe ignores
/// `wgpu_options` entirely when the renderer is glow.
const GLOW: Rung = Rung {
    id: "glow",
    label: "OpenGL",
    renderer: eframe::Renderer::Glow,
    backends: wgpu::Backends::empty(),
};

#[cfg(windows)]
const AUTO_LADDER: &[Rung] = &[DX12, VULKAN, WGPU_GL, GLOW];
/// wgpu builds no GL backend for macOS without the `angle` feature, so the
/// ladder skips straight from Metal to glow.
#[cfg(target_os = "macos")]
const AUTO_LADDER: &[Rung] = &[METAL, GLOW];
#[cfg(all(not(windows), not(target_os = "macos")))]
const AUTO_LADDER: &[Rung] = &[VULKAN, WGPU_GL, GLOW];

/// What [`select`] decided, plus everything the rest of the app needs to know
/// about that decision.
pub struct Selection {
    pub renderer: eframe::Renderer,
    pub backends: wgpu::Backends,
    /// API name Auto resolves to on this machine. Settings compares it against
    /// the active renderer so the restart button only appears for a selection
    /// that would actually change something.
    pub auto_label: &'static str,
    /// A breadcrumb was written and must be cleared once frames land; see
    /// [`confirm_boot`]. False when an env override is pinning the renderer.
    pub breadcrumb_pending: bool,
}

/// Adapter probe with a memo, so overlapping ladders (the preference ladder and
/// the Auto ladder) never enumerate the same backend twice.
#[derive(Default)]
struct Prober {
    memo: Vec<(wgpu::Backends, Option<wgpu::Backend>)>,
}

impl Prober {
    /// The backend of the first non-software adapter in `backends`, or `None`
    /// when the machine has no hardware adapter there.
    fn hardware_backend(&mut self, backends: wgpu::Backends) -> Option<wgpu::Backend> {
        if let Some((_, hit)) = self.memo.iter().find(|(probed, _)| *probed == backends) {
            return *hit;
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let hit = pollster::block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .map(|adapter| adapter.get_info())
            .find(|info| info.device_type != wgpu::DeviceType::Cpu)
            .map(|info| {
                tracing::debug!(
                    "wgpu adapter for {backends:?}: {} ({:?})",
                    info.name,
                    info.backend
                );
                info.backend
            });
        if hit.is_none() {
            tracing::debug!("no hardware wgpu adapter for {backends:?}");
        }
        self.memo.push((backends, hit));
        hit
    }
}

/// Proper API name for a backend; matches the mapping the settings window uses
/// for the *active* renderer so the two strings can be compared.
fn label_for_backend(backend: wgpu::Backend) -> Option<&'static str> {
    match backend {
        wgpu::Backend::Dx12 => Some("DirectX 12"),
        wgpu::Backend::Vulkan => Some("Vulkan"),
        wgpu::Backend::Metal => Some("Metal"),
        wgpu::Backend::Gl => Some("OpenGL (wgpu)"),
        _ => None,
    }
}

/// First rung with a hardware adapter, labelled by the adapter actually found
/// rather than by the rung, so a multi-backend rung (`WGPU_BACKEND`) still
/// reports the truth.
fn resolve(rungs: &[Rung], prober: &mut Prober) -> Option<(Rung, &'static str)> {
    for rung in rungs {
        if rung.renderer == eframe::Renderer::Glow {
            return Some((*rung, rung.label));
        }
        if let Some(backend) = prober.hardware_backend(rung.backends) {
            return Some((*rung, label_for_backend(backend).unwrap_or(rung.label)));
        }
    }
    None
}

fn preferred_rung(pref: RendererPreference) -> Option<Rung> {
    if !pref.valid_on_current_platform() {
        return None;
    }
    match pref {
        RendererPreference::Auto => None,
        RendererPreference::Dx12 => Some(DX12),
        RendererPreference::Vulkan => Some(VULKAN),
        RendererPreference::Metal => Some(METAL),
        RendererPreference::OpenGl => Some(GLOW),
    }
}

/// The preference first, then the Auto ladder as fallback. Platform-invalid
/// preferences contribute nothing and behave like Auto.
fn ladder(pref: RendererPreference) -> Vec<Rung> {
    let mut rungs: Vec<Rung> = Vec::with_capacity(AUTO_LADDER.len() + 1);
    if let Some(rung) = preferred_rung(pref) {
        rungs.push(rung);
    }
    for rung in AUTO_LADDER {
        if !rungs.iter().any(|existing| existing.id == rung.id) {
            rungs.push(*rung);
        }
    }
    rungs
}

/// `HESTIA_RENDERER` / `WGPU_BACKEND` pin the renderer for debugging. They win
/// over the stored preference and bypass the breadcrumb entirely, so a crash
/// under an override never demotes the user's normal boot.
fn env_ladder() -> Option<Vec<Rung>> {
    let env_backends = wgpu::Backends::from_env();
    let pinned = |backends: wgpu::Backends| Rung {
        id: "wgpu-env",
        label: "wgpu",
        renderer: eframe::Renderer::Wgpu,
        backends,
    };
    match std::env::var("HESTIA_RENDERER").as_deref() {
        Ok("glow") => return Some(vec![GLOW]),
        Ok("wgpu") => {
            let mut rungs: Vec<Rung> = match env_backends {
                Some(backends) => vec![pinned(backends)],
                None => AUTO_LADDER
                    .iter()
                    .copied()
                    .filter(|rung| rung.renderer != eframe::Renderer::Glow)
                    .collect(),
            };
            // Still better to start on glow than not at all.
            rungs.push(GLOW);
            return Some(rungs);
        }
        _ => {}
    }
    env_backends.map(|backends| vec![pinned(backends), GLOW])
}

/// Persisted next to the config. Absent on a healthy machine that has finished
/// booting at least once.
#[derive(Debug, Default, Serialize, Deserialize)]
struct BootRecord {
    /// Renderer id currently being tried. Still present at startup means the
    /// previous launch never reached [`PASSES_TO_CONFIRM`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt: Option<String>,
    /// Renderer ids that died before confirming. Skipped by the ladder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    failed: Vec<String>,
    /// App version that produced `failed`. A different version clears it: an
    /// app or driver update is exactly when a broken backend can start working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_version: Option<String>,
}

impl BootRecord {
    fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&raw).unwrap_or_else(|err| {
            tracing::warn!("ignoring unreadable renderer boot record: {err}");
            Self::default()
        })
    }

    /// Removed rather than written when there is nothing left worth keeping, so
    /// the healthy case leaves no file behind.
    fn save(&self, path: &Path) -> anyhow::Result<()> {
        if self.attempt.is_none() && self.failed.is_empty() {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            return Ok(());
        }
        let raw = toml::to_string_pretty(self)?;
        persistence::write_atomic_text(path, &raw)
    }

    fn retire_stale_version(&mut self) {
        let current = env!("CARGO_PKG_VERSION");
        if self.app_version.as_deref() == Some(current) {
            return;
        }
        if !self.failed.is_empty() {
            tracing::info!(
                "app version changed; giving previously failed renderers ({}) another chance",
                self.failed.join(", ")
            );
            self.failed.clear();
        }
        self.app_version = Some(current.to_owned());
    }

    /// An attempt still on record was never confirmed by a running window.
    fn demote_unconfirmed_attempt(&mut self) {
        let Some(attempt) = self.attempt.take() else {
            return;
        };
        tracing::warn!(
            "renderer '{attempt}' did not survive startup last launch; demoting to the next one"
        );
        if !self.failed.contains(&attempt) {
            self.failed.push(attempt);
        }
    }
}

fn breadcrumb_path(portable: &PortablePaths) -> PathBuf {
    match portable.state_archive.parent() {
        Some(dir) => dir.join(BREADCRUMB_FILE),
        None => PathBuf::from(BREADCRUMB_FILE),
    }
}

/// Picks the renderer for this launch and records the attempt.
///
/// Priority: `HESTIA_RENDERER` / `WGPU_BACKEND` env overrides, then the user's
/// stored preference, then Auto; at every step a rung known to have failed
/// startup is skipped, and a rung with no hardware adapter falls through to the
/// next.
pub fn select(portable: &PortablePaths, pref: RendererPreference) -> Selection {
    let mut prober = Prober::default();

    if let Some(rungs) = env_ladder() {
        let (rung, label) = resolve(&rungs, &mut prober).unwrap_or((GLOW, GLOW.label));
        tracing::info!("renderer pinned by environment: {label}");
        return Selection {
            renderer: rung.renderer,
            backends: rung.backends,
            // Nothing a restart could change while the override is set.
            auto_label: label,
            breadcrumb_pending: false,
        };
    }

    // Probed even when the preference is explicit, for the settings comparison.
    let auto_label =
        resolve(&ladder(RendererPreference::Auto), &mut prober).map_or(GLOW.label, |(_, l)| l);

    let path = breadcrumb_path(portable);
    let mut record = BootRecord::load(&path);
    record.retire_stale_version();
    record.demote_unconfirmed_attempt();

    let rungs = ladder(pref);
    let mut allowed: Vec<Rung> = rungs
        .iter()
        .copied()
        .filter(|rung| !record.failed.iter().any(|id| id == rung.id))
        .collect();
    if allowed.is_empty() {
        // Every option has failed at some point. Wipe the slate rather than
        // leave the app with nothing to start on.
        tracing::warn!(
            "every renderer has failed to start before; retrying the ladder from the top"
        );
        record.failed.clear();
        allowed = rungs;
    }

    let (rung, label) = resolve(&allowed, &mut prober).unwrap_or((GLOW, GLOW.label));
    tracing::info!("using renderer: {label}");

    record.attempt = Some(rung.id.to_owned());
    let breadcrumb_pending = match record.save(&path) {
        Ok(()) => true,
        Err(err) => {
            tracing::warn!("could not record the renderer boot attempt: {err}");
            false
        }
    };

    Selection {
        renderer: rung.renderer,
        backends: rung.backends,
        auto_label,
        breadcrumb_pending,
    }
}

/// True once enough passes have run that this renderer counts as working.
pub fn boot_confirmed(ctx: &egui::Context) -> bool {
    ctx.cumulative_pass_nr() >= PASSES_TO_CONFIRM
}

/// Clears the boot breadcrumb; the renderer reached the screen.
pub fn confirm_boot(portable: &PortablePaths) {
    let path = breadcrumb_path(portable);
    let mut record = BootRecord::load(&path);
    if record.attempt.take().is_none() {
        return;
    }
    if let Err(err) = record.save(&path) {
        tracing::warn!("could not clear the renderer boot attempt: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_ladder_ends_on_glow() {
        assert_eq!(AUTO_LADDER.last().map(|rung| rung.id), Some(GLOW.id));
    }

    #[test]
    fn explicit_preference_leads_then_falls_back_to_auto() {
        let rungs = ladder(RendererPreference::OpenGl);
        assert_eq!(rungs.first().map(|rung| rung.id), Some(GLOW.id));
        // No duplicate rungs, so a preference that is also in the Auto ladder
        // is not probed twice.
        for rung in &rungs {
            assert_eq!(rungs.iter().filter(|other| other.id == rung.id).count(), 1);
        }
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn platform_invalid_preference_behaves_like_auto() {
        let ids = |pref| {
            ladder(pref)
                .iter()
                .map(|rung| rung.id)
                .collect::<Vec<&'static str>>()
        };
        assert_eq!(
            ids(RendererPreference::Metal),
            ids(RendererPreference::Auto)
        );
    }

    #[test]
    fn unconfirmed_attempt_becomes_a_failure() {
        let mut record = BootRecord {
            attempt: Some("dx12".to_owned()),
            ..BootRecord::default()
        };
        record.demote_unconfirmed_attempt();
        assert!(record.attempt.is_none());
        assert_eq!(record.failed, vec!["dx12".to_owned()]);
        // Repeated failures do not pile up duplicates.
        record.attempt = Some("dx12".to_owned());
        record.demote_unconfirmed_attempt();
        assert_eq!(record.failed, vec!["dx12".to_owned()]);
    }

    #[test]
    fn version_change_clears_failures() {
        let mut record = BootRecord {
            failed: vec!["dx12".to_owned()],
            app_version: Some("0.0.0-old".to_owned()),
            ..BootRecord::default()
        };
        record.retire_stale_version();
        assert!(record.failed.is_empty());
        assert_eq!(
            record.app_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    fn portable_in(dir: &Path) -> PortablePaths {
        PortablePaths {
            state_archive: dir.join("hestia.toml"),
            state_source: None,
            history_db: dir.join("hestia.dat"),
        }
    }

    /// Runs a real adapter probe on whatever machine the tests run on, so the
    /// assertions are about the breadcrumb lifecycle rather than which backend
    /// wins. A GPU-less runner simply lands on glow.
    #[test]
    fn breadcrumb_is_written_on_select_and_cleared_on_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let portable = portable_in(dir.path());
        let path = breadcrumb_path(&portable);

        let selection = select(&portable, RendererPreference::Auto);
        assert!(selection.breadcrumb_pending);
        let attempt = BootRecord::load(&path).attempt;
        assert!(attempt.is_some(), "select must record what it is trying");

        confirm_boot(&portable);
        assert!(
            !path.exists(),
            "a clean boot should leave no breadcrumb behind"
        );
    }

    /// The crash case: a breadcrumb survives the launch, so the next `select`
    /// blames that renderer and refuses to pick it again.
    #[test]
    fn an_unconfirmed_launch_demotes_the_renderer_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let portable = portable_in(dir.path());
        let path = breadcrumb_path(&portable);

        let first = select(&portable, RendererPreference::Auto);
        assert!(first.breadcrumb_pending);
        let crashed = BootRecord::load(&path).attempt.unwrap();

        // No `confirm_boot`: stands in for a process that died before drawing.
        let second = select(&portable, RendererPreference::Auto);
        assert!(second.breadcrumb_pending);
        let record = BootRecord::load(&path);
        assert!(record.failed.contains(&crashed));
        assert_ne!(record.attempt, Some(crashed));

        // Auto's advertised label ignores the demotion: it describes the
        // hardware, not this machine's accumulated bad luck.
        assert_eq!(first.auto_label, second.auto_label);
    }

    #[test]
    fn record_round_trips_through_toml() {
        let record = BootRecord {
            attempt: Some("vulkan".to_owned()),
            failed: vec!["dx12".to_owned()],
            app_version: Some("1.2.3".to_owned()),
        };
        let raw = toml::to_string_pretty(&record).unwrap();
        let parsed: BootRecord = toml::from_str(&raw).unwrap();
        assert_eq!(parsed.attempt.as_deref(), Some("vulkan"));
        assert_eq!(parsed.failed, vec!["dx12".to_owned()]);
        assert_eq!(parsed.app_version.as_deref(), Some("1.2.3"));
    }
}
