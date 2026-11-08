use crate::model::RendererPreference;
use crate::persistence::{self, PortablePaths};
use eframe::wgpu;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuTarget {
    Any,
    Integrated,
    Dedicated,
    MatchDisplay,
}

impl GpuTarget {
    fn accepts(self, device_type: wgpu::DeviceType) -> bool {
        match self {
            GpuTarget::Integrated => device_type == wgpu::DeviceType::IntegratedGpu,
            GpuTarget::Dedicated => device_type == wgpu::DeviceType::DiscreteGpu,
            GpuTarget::Any | GpuTarget::MatchDisplay => device_type != wgpu::DeviceType::Cpu,
        }
    }
}

const BREADCRUMB_FILE: &str = "hestia-renderer.toml";

const PASSES_TO_CONFIRM: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rung {
    id: &'static str,
    label: &'static str,
    renderer: eframe::Renderer,
    backends: wgpu::Backends,
    gpu: GpuTarget,
}

const DX12: Rung = Rung {
    id: "dx12",
    label: "DirectX 12",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::DX12,
    gpu: GpuTarget::MatchDisplay,
};

const DX12_IGPU: Rung = Rung {
    id: "dx12-igpu",
    gpu: GpuTarget::Integrated,
    ..DX12
};

const DX12_DGPU: Rung = Rung {
    id: "dx12-dgpu",
    gpu: GpuTarget::Dedicated,
    ..DX12
};

const VULKAN: Rung = Rung {
    id: "vulkan",
    label: "Vulkan",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::VULKAN,
    gpu: GpuTarget::MatchDisplay,
};

const VULKAN_IGPU: Rung = Rung {
    id: "vulkan-igpu",
    gpu: GpuTarget::Integrated,
    ..VULKAN
};

const VULKAN_DGPU: Rung = Rung {
    id: "vulkan-dgpu",
    gpu: GpuTarget::Dedicated,
    ..VULKAN
};

const METAL: Rung = Rung {
    id: "metal",
    label: "Metal",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::METAL,
    gpu: GpuTarget::MatchDisplay,
};

const WGPU_GL: Rung = Rung {
    id: "wgpu-gl",
    label: "OpenGL (wgpu)",
    renderer: eframe::Renderer::Wgpu,
    backends: wgpu::Backends::GL,
    gpu: GpuTarget::Any,
};

const GLOW: Rung = Rung {
    id: "glow",
    label: "OpenGL",
    renderer: eframe::Renderer::Glow,
    backends: wgpu::Backends::empty(),
    gpu: GpuTarget::Any,
};

#[cfg(windows)]
const AUTO_LADDER: &[Rung] = &[DX12, VULKAN, WGPU_GL, GLOW];
#[cfg(target_os = "macos")]
const AUTO_LADDER: &[Rung] = &[METAL, GLOW];
#[cfg(all(not(windows), not(target_os = "macos")))]
const AUTO_LADDER: &[Rung] = &[VULKAN, WGPU_GL, GLOW];

pub struct Selection {
    pub renderer: eframe::Renderer,
    pub backends: wgpu::Backends,
    pub gpu: GpuTarget,
    pub auto_label: &'static str,
    pub breadcrumb_pending: bool,
}

#[derive(Default)]
struct Prober {
    memo: Vec<((wgpu::Backends, GpuTarget), Option<wgpu::Backend>)>,
}

impl Prober {
    fn hardware_backend(
        &mut self,
        backends: wgpu::Backends,
        gpu: GpuTarget,
    ) -> Option<wgpu::Backend> {
        let key = (backends, gpu);
        if let Some((_, hit)) = self.memo.iter().find(|(probed, _)| *probed == key) {
            return *hit;
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let hit = pollster::block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .map(|adapter| adapter.get_info())
            .find(|info| gpu.accepts(info.device_type))
            .map(|info| {
                tracing::debug!(
                    "wgpu adapter for {backends:?}/{gpu:?}: {} ({:?}, {:?})",
                    info.name,
                    info.backend,
                    info.device_type
                );
                info.backend
            });
        if hit.is_none() {
            tracing::debug!("no hardware wgpu adapter for {backends:?}/{gpu:?}");
        }
        self.memo.push((key, hit));
        hit
    }
}

fn label_for_backend(backend: wgpu::Backend) -> Option<&'static str> {
    match backend {
        wgpu::Backend::Dx12 => Some("DirectX 12"),
        wgpu::Backend::Vulkan => Some("Vulkan"),
        wgpu::Backend::Metal => Some("Metal"),
        wgpu::Backend::Gl => Some("OpenGL (wgpu)"),
        _ => None,
    }
}

fn resolve(rungs: &[Rung], prober: &mut Prober) -> Option<(Rung, &'static str)> {
    for rung in rungs {
        if rung.renderer == eframe::Renderer::Glow {
            return Some((*rung, rung.label));
        }
        if let Some(backend) = prober.hardware_backend(rung.backends, rung.gpu) {
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
        RendererPreference::Dx12Integrated => Some(DX12_IGPU),
        RendererPreference::Dx12Dedicated => Some(DX12_DGPU),
        RendererPreference::VulkanIntegrated => Some(VULKAN_IGPU),
        RendererPreference::VulkanDedicated => Some(VULKAN_DGPU),
    }
}

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

fn env_ladder() -> Option<Vec<Rung>> {
    let env_backends = wgpu::Backends::from_env();
    let pinned = |backends: wgpu::Backends| Rung {
        id: "wgpu-env",
        label: "wgpu",
        renderer: eframe::Renderer::Wgpu,
        backends,
        gpu: GpuTarget::MatchDisplay,
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
            rungs.push(GLOW);
            return Some(rungs);
        }
        _ => {}
    }
    env_backends.map(|backends| vec![pinned(backends), GLOW])
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BootRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    failed: Vec<String>,
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

pub fn select(portable: &PortablePaths, pref: RendererPreference) -> Selection {
    let mut prober = Prober::default();

    if let Some(rungs) = env_ladder() {
        let (rung, label) = resolve(&rungs, &mut prober).unwrap_or((GLOW, GLOW.label));
        tracing::info!("renderer pinned by environment: {label}");
        return Selection {
            renderer: rung.renderer,
            backends: rung.backends,
            gpu: rung.gpu,
            auto_label: label,
            breadcrumb_pending: false,
        };
    }

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
        gpu: rung.gpu,
        auto_label,
        breadcrumb_pending,
    }
}

pub fn pick_adapter(
    adapters: &[wgpu::Adapter],
    gpu: GpuTarget,
    display_pci: Option<(u32, u32)>,
) -> Option<wgpu::Adapter> {
    let infos: Vec<_> = adapters.iter().map(|a| a.get_info()).collect();
    let index = select_adapter_index(&infos, gpu, display_pci)?;
    let info = &infos[index];
    tracing::info!(
        "selected adapter for {gpu:?}: {} ({:?}, {:?})",
        info.name,
        info.backend,
        info.device_type
    );
    Some(adapters[index].clone())
}

fn select_adapter_index(
    infos: &[wgpu::AdapterInfo],
    gpu: GpuTarget,
    display_pci: Option<(u32, u32)>,
) -> Option<usize> {
    let first_hw = || (0..infos.len()).find(|&i| infos[i].device_type != wgpu::DeviceType::Cpu);
    let first_of = |t: wgpu::DeviceType| (0..infos.len()).find(|&i| infos[i].device_type == t);

    let chosen = match gpu {
        GpuTarget::Integrated => first_of(wgpu::DeviceType::IntegratedGpu).or_else(first_hw),
        GpuTarget::Dedicated => first_of(wgpu::DeviceType::DiscreteGpu).or_else(first_hw),
        GpuTarget::MatchDisplay => display_pci
            .and_then(|(vendor, device)| {
                (0..infos.len()).find(|&i| {
                    infos[i].device_type != wgpu::DeviceType::Cpu
                        && infos[i].vendor == vendor
                        && infos[i].device == device
                })
            })
            .or_else(|| first_of(wgpu::DeviceType::IntegratedGpu))
            .or_else(first_hw),
        GpuTarget::Any => first_hw(),
    };
    chosen.or_else(|| (!infos.is_empty()).then_some(0))
}

#[cfg(windows)]
pub fn display_pci_for_window(window_pos: Option<(i32, i32)>) -> Option<(u32, u32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    use windows::Win32::Graphics::Gdi::{
        HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint,
    };

    unsafe {
        let target: HMONITOR = match window_pos {
            Some((x, y)) => MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST),
            None => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
        };
        if target.0.is_null() {
            return None;
        }

        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut ai = 0u32;
        while let Ok(adapter) = factory.EnumAdapters1(ai) {
            ai += 1;
            let mut oi = 0u32;
            while let Ok(output) = adapter.EnumOutputs(oi) {
                oi += 1;
                let Ok(desc) = output.GetDesc() else { continue };
                if desc.Monitor.0 == target.0 {
                    return adapter
                        .GetDesc1()
                        .ok()
                        .map(|adesc| (adesc.VendorId, adesc.DeviceId));
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn display_pci_for_window(_window_pos: Option<(i32, i32)>) -> Option<(u32, u32)> {
    None
}

pub fn boot_confirmed(ctx: &egui::Context) -> bool {
    ctx.cumulative_pass_nr() >= PASSES_TO_CONFIRM
}

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

    fn info(
        device_type: wgpu::DeviceType,
        vendor: u32,
        device: u32,
        name: &str,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: name.to_owned(),
            vendor,
            device,
            device_type,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Dx12,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: Some(false),
            limit_bucket: None,
        }
    }

    #[test]
    fn auto_ladder_ends_on_glow() {
        assert_eq!(AUTO_LADDER.last().map(|rung| rung.id), Some(GLOW.id));
    }

    #[test]
    fn explicit_gpu_preferences_map_to_their_rungs() {
        let cases = [
            (RendererPreference::Dx12Integrated, DX12_IGPU),
            (RendererPreference::Dx12Dedicated, DX12_DGPU),
            (RendererPreference::VulkanIntegrated, VULKAN_IGPU),
            (RendererPreference::VulkanDedicated, VULKAN_DGPU),
        ];
        for (pref, rung) in cases {
            if pref.valid_on_current_platform() {
                assert_eq!(preferred_rung(pref), Some(rung));
                assert_ne!(
                    rung.id, DX12.id,
                    "per-GPU rungs need distinct breadcrumb ids"
                );
            }
        }
    }

    #[test]
    fn integrated_and_dedicated_targets_pick_their_device_type() {
        let igpu = info(wgpu::DeviceType::IntegratedGpu, 0x8086, 0x4680, "iGPU");
        let dgpu = info(wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2503, "dGPU");
        let infos = [igpu, dgpu];
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::Integrated, None),
            Some(0)
        );
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::Dedicated, None),
            Some(1)
        );
    }

    #[test]
    fn dedicated_falls_back_when_no_discrete_gpu_present() {
        let infos = [info(
            wgpu::DeviceType::IntegratedGpu,
            0x8086,
            0x4680,
            "iGPU",
        )];
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::Dedicated, None),
            Some(0)
        );
    }

    #[test]
    fn match_display_prefers_the_displays_gpu() {
        let igpu = info(wgpu::DeviceType::IntegratedGpu, 0x8086, 0x4680, "iGPU");
        let dgpu = info(wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2503, "dGPU");
        let infos = [igpu, dgpu];
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::MatchDisplay, Some((0x10de, 0x2503))),
            Some(1)
        );
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::MatchDisplay, None),
            Some(0)
        );
        assert_eq!(
            select_adapter_index(&infos, GpuTarget::MatchDisplay, Some((0xdead, 0xbeef))),
            Some(0)
        );
    }

    #[test]
    fn explicit_preference_leads_then_falls_back_to_auto() {
        let rungs = ladder(RendererPreference::OpenGl);
        assert_eq!(rungs.first().map(|rung| rung.id), Some(GLOW.id));
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

    #[test]
    fn an_unconfirmed_launch_demotes_the_renderer_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let portable = portable_in(dir.path());
        let path = breadcrumb_path(&portable);

        let first = select(&portable, RendererPreference::Auto);
        assert!(first.breadcrumb_pending);
        let crashed = BootRecord::load(&path).attempt.unwrap();

        let second = select(&portable, RendererPreference::Auto);
        assert!(second.breadcrumb_pending);
        let record = BootRecord::load(&path);
        assert!(record.failed.contains(&crashed));
        assert_ne!(record.attempt, Some(crashed));

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
