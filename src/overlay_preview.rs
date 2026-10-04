//! Disposable native costume-switching preview. Activation changes stay in memory.
//! Launch with `hestia --overlay-preview`; normal startup never enters this module.
//!
//! The same window is also the in-game overlay, which Hestia starts with
//! `--overlay` while a supported game runs.  It shows Hestia's library and
//! hides until its key, Alt+H unless Settings changed it.

mod data;
mod gamebanana;
mod hints;
mod keyboard;
mod layouts;
mod live;
mod motion;
mod platform;
mod restore;
mod search;
mod session;
mod thumbnails;

pub(crate) use data::mod_image;
pub(crate) use platform::hotkey_taken;

use std::{
    cell::Cell,
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::{Color32, RichText, ViewportCommand};
use gamebanana::InstallNews;

use crate::{
    app::{TextCatalog, TextKey},
    model::{AppLanguage, InterfaceSize, OverlayHotkey},
    overlay_protocol::{FromOverlay, Settings},
};

pub(crate) const OVERLAY_OPACITY_MIN: u8 = 50;
pub(crate) const OVERLAY_OPACITY_MAX: u8 = 94;
pub(crate) const DEFAULT_OVERLAY_OPACITY: u8 = 78;
/// How much bigger than Hestia's interface zoom the overlay draws, so it
/// reads over a game.
const OVERLAY_ZOOM: f32 = 1.2;
const EXPANDED_SIZE: egui::Vec2 = egui::vec2(560.0, 392.0);
const IDLE_SIZE: egui::Vec2 = egui::vec2(280.0, 48.0);
// Reserve the hold-preview space without resizing or recentering the native
// window during interaction. Native regions remove all unused hit-test space.
const CANVAS_SIZE: egui::Vec2 = egui::vec2(560.0, 660.0);
const CAROUSEL_HEIGHT: f32 = 266.0;
const HEADER_HEIGHT: f32 = 46.0;
const SEARCH_SIZE: egui::Vec2 = egui::vec2(120.0, 22.0);
/// Win32 `ERROR_HOTKEY_ALREADY_REGISTERED`.
const HOTKEY_ALREADY_REGISTERED: i32 = 1409;

thread_local! {
    /// Hestia's language, for the overlay's text.
    static LANGUAGE: Cell<AppLanguage> = const { Cell::new(AppLanguage::English) };
}

/// The overlay's text in Hestia's language.
fn text(key: TextKey) -> &'static str {
    TextCatalog::new(LANGUAGE.get()).get(key)
}

pub fn run() -> anyhow::Result<()> {
    launch(None)
}

/// The in-game overlay.  Hestia sends the library first.
pub fn run_live() -> anyhow::Result<()> {
    let start = live::read_start()?;
    data::save_sample(&start);
    launch(Some(start))
}

fn launch(start: Option<crate::overlay_protocol::Start>) -> anyhow::Result<()> {
    let preview = start.is_none();
    let (catalog, mut settings, live_start) = match start {
        Some(start) => (
            data::catalog_from_library(start.library.clone()),
            start.settings,
            Some((
                start.host_window,
                start.game_pids,
                start.selection,
                start.library,
            )),
        ),
        None => {
            let (catalog, settings) = data::load_sample();
            (catalog, settings, None)
        }
    };
    // The preview can show another language, to check its text.
    let language = preview
        .then(|| std::env::var("HESTIA_OVERLAY_PREVIEW_LANGUAGE").ok())
        .flatten()
        .and_then(|tag| AppLanguage::from_locale_tag(&tag))
        .unwrap_or(settings.language);
    LANGUAGE.set(language);
    // And another hotkey or size, to check they fit.  The capture runs'
    // clicks are laid out at a zoom of 1, so it's the preview's.
    if preview {
        settings.zoom = 1.0 / OVERLAY_ZOOM;
        if let Some(hotkey) = std::env::var("HESTIA_OVERLAY_PREVIEW_HOTKEY")
            .ok()
            .and_then(|name| OverlayHotkey::parse(&name))
        {
            settings.hotkey = hotkey;
        }
        if let Ok(name) = std::env::var("HESTIA_OVERLAY_PREVIEW_SIZE")
            && let Some(size) = InterfaceSize::ALL
                .into_iter()
                .find(|size| format!("{size:?}").eq_ignore_ascii_case(&name))
        {
            settings.zoom = size.zoom();
        }
    }
    let zoom = OVERLAY_ZOOM * settings.zoom;
    let game = catalog.game.clone();
    // The in-game overlay ignores the preview's test settings.
    let capture = preview
        .then(|| std::env::var_os("HESTIA_OVERLAY_PREVIEW_CAPTURE"))
        .flatten()
        .map(PathBuf::from);
    let capture_frame = std::env::var("HESTIA_OVERLAY_PREVIEW_CAPTURE_FRAME")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(24);
    let capture_at = capture.as_ref().and_then(|_| {
        std::env::var("HESTIA_OVERLAY_PREVIEW_CAPTURE_MS")
            .ok()?
            .parse::<u64>()
            .ok()
            .map(Duration::from_millis)
    });
    let capture_button = if std::env::var_os("HESTIA_OVERLAY_PREVIEW_SECONDARY").is_some() {
        egui::PointerButton::Secondary
    } else {
        egui::PointerButton::Primary
    };
    let pinned = preview && std::env::var_os("HESTIA_OVERLAY_PREVIEW_PINNED").is_some();
    // Capture runs press Alt+H through egui: on the first frame to open the
    // overlay, and on the close frame to close it.
    let capture_open =
        capture.is_some() && std::env::var_os("HESTIA_OVERLAY_PREVIEW_OPEN").is_some();
    let capture_close = capture.as_ref().and_then(|_| {
        std::env::var("HESTIA_OVERLAY_PREVIEW_CLOSE_FRAME")
            .ok()?
            .parse()
            .ok()
    });
    let opacity = preview
        .then(|| std::env::var("HESTIA_OVERLAY_PREVIEW_OPACITY").ok())
        .flatten()
        .and_then(|value| value.parse::<u8>().ok())
        .or(settings.opacity)
        .map(clamp_overlay_opacity)
        .unwrap_or(DEFAULT_OVERLAY_OPACITY);
    // Optional in-process input for native screenshot checks, never sent to Windows/the game.
    let capture_click = capture.as_ref().and_then(|_| {
        let value = std::env::var("HESTIA_OVERLAY_PREVIEW_CLICK").ok()?;
        let (x, y) = value.split_once(',')?;
        Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
    });
    let capture_release = capture_click.and_then(|pos| {
        let frame = std::env::var("HESTIA_OVERLAY_PREVIEW_RELEASE_FRAME")
            .ok()?
            .parse::<u32>()
            .ok()?;
        Some((frame, pos))
    });
    let capture_text = capture
        .as_ref()
        .and_then(|_| std::env::var("HESTIA_OVERLAY_PREVIEW_TEXT").ok());
    // Keys such as `F text:vow Enter`, pressed one per frame.
    let capture_keys: VecDeque<String> = capture
        .as_ref()
        .and_then(|_| std::env::var("HESTIA_OVERLAY_PREVIEW_KEYS").ok())
        .map(|keys| keys.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    let capture_wheel = capture
        .as_ref()
        .and_then(|_| capture_wheel(&std::env::var("HESTIA_OVERLAY_PREVIEW_WHEEL").ok()?));
    // The preview has no Hestia to read hotkeys, so it can give every mod
    // the same few.
    let sample_hotkeys = preview && std::env::var_os("HESTIA_OVERLAY_PREVIEW_HOTKEYS").is_some();
    let app_icon = eframe::icon_data::from_png_bytes(include_bytes!("asset/icon.png"))?;
    let brand_source =
        image::RgbaImage::from_raw(app_icon.width, app_icon.height, app_icon.rgba.clone())
            .expect("valid embedded app icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Hestia titles the overlay "Hestia" just while it presses the
            // reload key, never while the search takes the keys.
            .with_title(if preview {
                "Hestia — Overlay preview"
            } else {
                crate::overlay_protocol::OVERLAY_TITLE
            })
            .with_inner_size(CANVAS_SIZE * zoom)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_active(false)
            .with_always_on_top()
            .with_icon(app_icon),
        // The prototype deliberately does not load or alter the normal renderer preference.
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        if preview {
            "Hestia overlay preview"
        } else {
            "Hestia overlay"
        },
        options,
        Box::new(move |cc| {
            platform::configure(cc)?;
            #[cfg(windows)]
            if let Some(window) = cc.winit_window() {
                use winit::platform::windows::WindowExtWindows;
                // egui-winit enables a native shadow for undecorated windows. It
                // outlines even the clear carousel area, so suppress both it and
                // Windows 11's independently drawn border on this overlay only.
                window.set_undecorated_shadow(false);
                window.set_border_color(None);
            }
            apply_preview_style(&cc.egui_ctx);
            cc.egui_ctx.set_zoom_factor(zoom);
            // Hestia sets the size.  Ctrl + = here would outgrow the window.
            cc.egui_ctx
                .options_mut(|options| options.zoom_with_keyboard = false);
            keyboard::set_hotkey(settings.hotkey);
            let keyboard = capture
                .is_none()
                .then(|| keyboard::Keyboard::new(cc.egui_ctx.clone()));
            let hotkey_warning = if keyboard.is_some() {
                register_hotkey(settings.hotkey)
            } else {
                None
            };
            // On Windows, the window procedure routes the overlay's keys
            // before egui sees them.  Capture runs and other platforms read
            // the same keys from egui instead.
            let egui_keys =
                (keyboard.is_none() || !cfg!(windows)).then(keyboard::EguiKeys::default);
            let mut samples = layouts::Layouts::new(catalog);
            let live = match live_start {
                Some((host_window, game_processes, selection, library)) => {
                    samples.set_gamebanana(settings.gamebanana, settings.all_characters);
                    if let Some(selection) = &selection {
                        samples.restore_selection(selection);
                    }
                    platform::wait_for_game(&game_processes);
                    Some(Live {
                        link: live::Link::start(cc.egui_ctx.clone(), host_window, opacity)?,
                        strip: live::Strip::default(),
                        changes: live::Changes::new(library),
                        notice: None,
                    })
                }
                None => None,
            };
            Ok(Box::new(OverlayPreview {
                game,
                #[cfg(windows)]
                native_window: cc.winit_window().cloned(),
                brand: None,
                brand_source,
                brand_pixels: 0,
                restore_error: None,
                region_update_failed: false,
                samples,
                live,
                opacity,
                settings,
                pinned,
                expanded: pinned,
                motion: motion::ExpansionMotion::new(pinned),
                hint_ticker: hints::Ticker::default(),
                pointer_completion: PointerCompletion::default(),
                capture_open,
                capture_close,
                keyboard,
                egui_keys,
                session: session::Session::default(),
                search: search::Search::default(),
                focus_return_pending: false,
                hotkey_warning,
                focus_warning: None,
                capture,
                capture_click,
                capture_button,
                capture_at,
                capture_release,
                capture_hold: capture_release.is_some()
                    || (capture_text.is_none()
                        && std::env::var_os("HESTIA_OVERLAY_PREVIEW_HOLD").is_some()),
                capture_text,
                capture_keys,
                capture_wheel,
                sample_hotkeys,
                capture_requested: false,
                capture_frame,
                frames_drawn: 0,
                started: Instant::now(),
            }))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

struct OverlayPreview {
    game: String,
    #[cfg(windows)]
    native_window: Option<std::sync::Arc<winit::window::Window>>,
    brand: Option<egui::TextureHandle>,
    brand_source: image::RgbaImage,
    brand_pixels: u32,
    restore_error: Option<String>,
    region_update_failed: bool,
    samples: layouts::Layouts,
    /// Only in the in-game overlay.
    live: Option<Live>,
    opacity: u8,
    /// Hestia's settings for the overlay, or the sample's in the preview.
    settings: Settings,
    pinned: bool,
    expanded: bool,
    motion: motion::ExpansionMotion,
    hint_ticker: hints::Ticker,
    pointer_completion: PointerCompletion,
    capture_open: bool,
    capture_close: Option<u32>,
    keyboard: Option<keyboard::Keyboard>,
    egui_keys: Option<keyboard::EguiKeys>,
    session: session::Session,
    search: search::Search,
    /// Hand the keyboard back once no key pressed in the overlay is held.
    focus_return_pending: bool,
    hotkey_warning: Option<TextKey>,
    focus_warning: Option<TextKey>,
    capture: Option<PathBuf>,
    capture_click: Option<egui::Pos2>,
    capture_button: egui::PointerButton,
    capture_at: Option<Duration>,
    capture_release: Option<(u32, egui::Pos2)>,
    capture_hold: bool,
    capture_text: Option<String>,
    capture_keys: VecDeque<String>,
    capture_wheel: Option<(egui::Pos2, f32)>,
    /// The preview gives every mod sample hotkeys.
    sample_hotkeys: bool,
    capture_requested: bool,
    capture_frame: u32,
    frames_drawn: u32,
    started: Instant,
}

/// The in-game overlay's link to Hestia, and when its strip shows.
struct Live {
    link: live::Link,
    strip: live::Strip,
    changes: live::Changes,
    notice: Option<Notice>,
}

/// The line above the strip: why Hestia didn't make a change, which key
/// shows it in the game, or how an install went.
struct Notice {
    text: String,
    until: f64,
    /// Warnings are red.
    warning: bool,
}

impl eframe::App for OverlayPreview {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        if std::mem::take(&mut self.capture_open) {
            push_hotkey(input, self.settings.hotkey);
        }
        if self
            .capture_close
            .is_some_and(|frame| self.frames_drawn >= frame)
        {
            self.capture_close = None;
            push_hotkey(input, self.settings.hotkey);
        }
        if self.frames_drawn >= 12 {
            if let Some(pos) = self.capture_click.take() {
                input.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    if !pressed && self.capture_hold {
                        continue;
                    }
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: self.capture_button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
        }
        if self.frames_drawn >= 16 {
            if let Some(text) = self.capture_text.take() {
                input.events.push(egui::Event::Text(text));
            }
        }
        if self
            .capture_release
            .is_some_and(|(frame, _)| self.frames_drawn >= frame)
        {
            let (_, pos) = self.capture_release.take().expect("checked release frame");
            input.events.push(egui::Event::PointerButton {
                pos,
                button: self.capture_button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.frames_drawn >= 20 {
            // One key per frame, so each one lands on what the last one showed.
            if let Some(token) = self.capture_keys.pop_front() {
                if let Some(text) = token.strip_prefix("text:") {
                    push_capture_text(input, text);
                } else if let Some(wheel) = token.strip_prefix("wheel:") {
                    match capture_wheel(wheel) {
                        Some((pos, delta)) => push_capture_wheel(input, pos, delta),
                        None => tracing::warn!(%token, "Unknown capture wheel"),
                    }
                } else if let Some((click, pos)) = capture_pointer(&token) {
                    input.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false].into_iter().filter(|_| click) {
                        input.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                } else if token == "_" {
                    // A frame without a key, to wait for an animation.
                } else {
                    match capture_key(&token) {
                        Some((key, modifiers)) => push_capture_key(input, key, modifiers),
                        None => tracing::warn!(%token, "Unknown capture key"),
                    }
                }
            }
            if let Some((pos, delta)) = self.capture_wheel.take() {
                push_capture_wheel(input, pos, delta);
            }
        }
        if let Some(keys) = &mut self.egui_keys {
            keys.filter(&mut input.events);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(path) = &self.capture {
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(screenshot) = screenshot {
                let pixels: Vec<u8> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                    .collect();
                match image::save_buffer(
                    path,
                    &pixels,
                    screenshot.width() as u32,
                    screenshot.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => tracing::info!(?path, "Saved native overlay preview"),
                    Err(error) => tracing::error!(?path, %error, "Could not save overlay preview"),
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
                self.capture = None;
            } else if self.started.elapsed() > Duration::from_secs(15) {
                tracing::error!("Native preview capture timed out");
                ctx.send_viewport_cmd(ViewportCommand::Close);
                self.capture = None;
            }
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let now = ctx.input(|input| input.time);
        let news = self.live.as_mut().map(|live| live.link.take_news());
        if let Some(settings) = news.as_ref().and_then(|news| news.settings) {
            self.apply_settings(&ctx, settings, now);
        }
        if let (Some(live), Some(news)) = (&mut self.live, news) {
            if let Some(library) = news.library {
                self.game = library.game_name.clone();
                live.changes.receive(library);
            }
            let mut refused = None;
            let mut press_key = None;
            for answer in news.answers {
                press_key = answer.press_key.clone().or(press_key);
                refused = live.changes.answer(answer).or(refused);
            }
            if live.changes.expire(now) {
                refused = Some(text(TextKey::GameOverlayNoAnswer).to_owned());
            }
            if let Some(error) = refused {
                live.strip.warn(now);
                live.notice = Some(Notice {
                    text: error,
                    until: now + live::WARNING_SECONDS,
                    warning: true,
                });
            } else if let Some(key) = press_key {
                live.notice = Some(Notice {
                    text: text(TextKey::GameOverlayPressReloadKey).replace("{key}", &key),
                    until: now + live::WARNING_SECONDS,
                    warning: false,
                });
            }
            if let Some(library) = live.changes.take_update() {
                self.samples
                    .replace_catalog(data::catalog_from_library(library));
            }
            self.samples.receive_hotkeys(news.hotkeys);
            // After the library, which has the mods an install added.
            for news in self.samples.receive_gamebanana(news.gamebanana) {
                let (key, name, warning) = match news {
                    InstallNews::Installed(name) => {
                        (TextKey::GameOverlayInstalledNotice, name, false)
                    }
                    InstallNews::Failed(name) => {
                        (TextKey::GameOverlayInstallFailedNotice, name, true)
                    }
                    // The open overlay shows the question itself.
                    InstallNews::Question(_) if self.expanded => continue,
                    InstallNews::Question(name) => {
                        (TextKey::GameOverlayNeedsAnswerNotice, name, false)
                    }
                };
                live.strip.warn(now);
                live.notice = Some(Notice {
                    text: text(key)
                        .replace("{name}", &name)
                        .replace("{key}", &self.settings.hotkey.label()),
                    until: now + live::WARNING_SECONDS,
                    warning,
                });
            }
            // Installed mods that crossed out of sight last frame.
            for name in self.samples.take_arrivals() {
                live.notice = Some(Notice {
                    text: text(TextKey::GameOverlayArrivedNotice).replace("{name}", &name),
                    until: now + live::WARNING_SECONDS,
                    warning: false,
                });
            }
            match &live.notice {
                Some(notice) if now >= notice.until => live.notice = None,
                Some(notice) => {
                    ctx.request_repaint_after(Duration::from_secs_f64(notice.until - now))
                }
                None => {}
            }
            self.samples.set_waiting(live.changes.waiting());
            if live.link.take_closed() {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            // A hotkey warning shows even without the strip.
            if platform::take_game_arrived()
                && (self.settings.arrival_strip || self.hotkey_warning.is_some())
            {
                live.strip.arrive(now);
            }
        }
        // Alt+F4 closes the in-game overlay like its X.  Only Hestia ends it.
        if self.live.as_ref().is_some_and(|live| !live.link.closing())
            && ctx.input(|input| input.viewport().close_requested())
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.close_live(&ctx);
        }
        if ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::MouseWheel { .. }))
        }) {
            layouts::suppress_tooltips(&ctx);
        }
        // Warm the small neighboring set even while the overlay is collapsed.
        self.samples.prepare_thumbnails(&ctx);
        let brand_size = if self.expanded { 24.0 } else { 28.0 };
        let brand_pixels = (brand_size * ctx.pixels_per_point()).round().max(1.0) as u32;
        if self.brand_pixels != brand_pixels {
            let icon_image = filtered_icon(&self.brand_source, brand_pixels);
            self.brand =
                Some(ctx.load_texture("hestia-brand", icon_image, egui::TextureOptions::LINEAR));
            self.brand_pixels = brand_pixels;
        }
        let mut events = self
            .keyboard
            .as_ref()
            .map(keyboard::Keyboard::drain)
            .unwrap_or_default();
        if let Some(keys) = &mut self.egui_keys {
            events.extend(keys.drain());
        }
        let mut escape = false;
        let mut commands = Vec::new();
        for event in events {
            let transition = match event {
                keyboard::Event::Hotkey { focused } => {
                    self.focus_warning = (!focused).then_some(TextKey::GameOverlayFocusTakeFailed);
                    self.session.hotkey()
                }
                keyboard::Event::Command(command) => {
                    // Gate on the state when the key arrived: a command and the
                    // key that ends the session can share a frame.  Rows are
                    // settled here for the same reason, since opening and
                    // closing reset them.
                    if (self.session.is_open() || self.pinned) && self.samples.question_open() {
                        // An install's question takes W/S and Space until
                        // it's answered.
                        match command {
                            keyboard::Command::Row(direction) => {
                                self.samples.move_question(direction)
                            }
                            keyboard::Command::Exclusive => self.samples.answer_question(),
                            _ => {}
                        }
                    } else if self.session.is_open() || self.pinned {
                        match command {
                            keyboard::Command::Row(direction) => self.samples.switch_row(direction),
                            keyboard::Command::Move(direction) => {
                                commands.push(move_in_row(self.samples.row(), direction))
                            }
                            command => commands.push(command),
                        }
                    }
                    session::Transition::None
                }
                keyboard::Event::Search => {
                    if self.session.is_open() || self.pinned {
                        self.search.toggle();
                        ctx.memory_mut(|memory| {
                            if self.search.typing() {
                                memory.request_focus(search_id());
                            } else {
                                memory.surrender_focus(search_id());
                            }
                        });
                    }
                    session::Transition::None
                }
                keyboard::Event::Escape => {
                    // Esc clears a search first, then cancels an install's
                    // question, then closes.
                    if self.search.escape() {
                        ctx.memory_mut(|memory| memory.surrender_focus(search_id()));
                        session::Transition::None
                    } else if (self.session.is_open() || self.pinned)
                        && self.samples.cancel_question()
                    {
                        session::Transition::None
                    } else {
                        escape = true;
                        self.pinned = false;
                        self.session.escape()
                    }
                }
                keyboard::Event::Clicked => {
                    self.focus_warning = None;
                    self.session.clicked()
                }
                keyboard::Event::Deactivated => {
                    self.focus_return_pending = false;
                    if self.focus_warning == Some(TextKey::GameOverlayFocusReturnFailed) {
                        self.focus_warning = None;
                    }
                    self.session.deactivated()
                }
            };
            self.apply_transition(&ctx, transition);
        }
        platform::set_click_takes_keyboard(self.pinned);
        if self.focus_return_pending {
            if self
                .keyboard
                .as_ref()
                .is_some_and(keyboard::Keyboard::busy)
            {
                // Key releases ask for a frame themselves.  This also covers
                // a release that never arrived.
                ctx.request_repaint_after(Duration::from_millis(50));
            } else {
                self.focus_return_pending = false;
                if platform::return_focus() {
                    tracing::info!("Overlay returned the keyboard");
                } else {
                    tracing::warn!("Overlay could not return the keyboard");
                    self.focus_warning = Some(TextKey::GameOverlayFocusReturnFailed);
                }
            }
        }
        if escape {
            self.pointer_completion.cancel();
            self.samples.cancel_pointer_interaction();
        }
        // Keep the release frame in the expanded layout so its original card or
        // slider receives the completed gesture before the viewport moves.
        let (pressed_inside, pointer_down) = ctx.input(|input| {
            (
                input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| root.max_rect().contains(pos)),
                input.pointer.any_down(),
            )
        });
        let completing_pointer = self
            .pointer_completion
            .begin_frame(self.expanded && !escape, pressed_inside);
        let expanded = self.pinned || self.session.is_open() || completing_pointer;
        if expanded != self.expanded {
            self.samples.dismiss_transient_ui(&ctx);
            if !expanded {
                self.samples.cancel_pointer_interaction();
                self.clear_search(&ctx);
            }
            self.expanded = expanded;
            ctx.request_repaint();
        }
        // Before the queued keys below, which move through the results.
        self.samples.set_search(self.search.query());
        self.hint_ticker.update(expanded, self.hint_mode(), now);
        self.motion.advance(now, expanded);
        if self.motion.animating() {
            layouts::suppress_tooltips(&ctx);
            ctx.request_repaint_after(Duration::from_millis(8));
        }
        self.samples.set_reveal(self.motion.cards());
        let strip_opacity = self.update_strip(&ctx, now, expanded);
        // Queue after the expansion change above, which clears queued commands.
        for command in commands {
            match command {
                keyboard::Command::Mod(direction) => self.samples.navigate_mod(&ctx, direction),
                keyboard::Command::Category(direction) => {
                    self.samples.navigate_category(&ctx, direction)
                }
                keyboard::Command::Exclusive => self
                    .samples
                    .apply_focused_action(&ctx, layouts::ModAction::Exclusive),
                keyboard::Command::Toggle => self
                    .samples
                    .apply_focused_action(&ctx, layouts::ModAction::Toggle),
                keyboard::Command::Flip => self.samples.flip_focused(&ctx),
                // Settled when the key arrived.
                keyboard::Command::Row(_) | keyboard::Command::Move(_) => {}
            }
        }
        let idle_width = self.idle_width(&ctx);
        let mut visible_regions = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                ui.multiply_opacity(strip_opacity);
                let bounds = ui.max_rect();
                let opening = self.motion.strip();
                let layout = motion::geometry(bounds, opening, idle_width);
                visible_regions.push(layout.base);
                ui.painter().rect_filled(
                    layout.base,
                    0,
                    Color32::from_rgba_unmultiplied(32, 32, 32, base_alpha(self.opacity)),
                );
                // The newest message: in the header while it shows, else
                // above the strip.
                let hotkey = self.settings.hotkey.label();
                let warning = |key: TextKey| text(key).replace("{key}", &hotkey);
                let message: Option<(String, bool)> = self
                    .restore_error
                    .clone()
                    .map(|error| (error, true))
                    .or_else(|| {
                        let notice = self.live.as_ref()?.notice.as_ref()?;
                        Some((notice.text.clone(), notice.warning))
                    })
                    .or_else(|| self.focus_warning.map(|key| (warning(key), true)))
                    .or_else(|| self.hotkey_warning.map(|key| (warning(key), true)));
                if opening > 0.5 {
                    let mut header_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("expanded-header")
                            .max_rect(layout.header.shrink2(egui::vec2(10.0, 8.0))),
                    );
                    header_ui.set_clip_rect(layout.base);
                    header_ui.multiply_opacity(((opening - 0.5) * 2.0).clamp(0.0, 1.0));
                    self.show_header(&mut header_ui, self.opacity, message.as_ref());
                    let mut strip_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("category-rail")
                            .max_rect(layout.rail.shrink2(egui::vec2(10.0, 3.0))),
                    );
                    strip_ui.set_clip_rect(layout.rail.intersect(layout.base));
                    strip_ui.multiply_opacity(((opening - 0.5) * 2.0).clamp(0.0, 1.0));
                    self.samples
                        .show_category_strip(&mut strip_ui, self.opacity);
                }
                if opening < 0.5 {
                    let drag = ui.interact(
                        layout.base,
                        ui.id().with("mini-strip-drag"),
                        egui::Sense::drag(),
                    );
                    if drag.drag_started() {
                        self.start_window_drag(&ctx);
                    }
                    let idle_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            layout.base.center().x,
                            layout.base.top() + IDLE_SIZE.y / 2.0,
                        ),
                        egui::vec2(idle_width, IDLE_SIZE.y),
                    );
                    let mut idle_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("mini-header")
                            .max_rect(idle_rect.shrink2(egui::vec2(10.0, 8.0))),
                    );
                    idle_ui.multiply_opacity((1.0 - opening * 2.0).clamp(0.0, 1.0));
                    self.show_idle(&mut idle_ui);
                }
                if self.motion.cards() > 0.001 {
                    let mut gallery_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("gallery")
                            .max_rect(layout.gallery),
                    );
                    self.samples.show_carousel(&mut gallery_ui, self.opacity);
                    let preview_rect = egui::Rect::from_min_max(
                        bounds.min + egui::vec2(10.0, 10.0),
                        egui::pos2(bounds.right() - 10.0, layout.base.top() - 10.0),
                    );
                    self.samples
                        .show_held_preview(ui, preview_rect, self.opacity);
                    visible_regions.extend_from_slice(self.samples.visible_card_rects());
                }
                if opening <= 0.5
                    && let Some((message, red)) = message
                {
                    let warning = egui::Rect::from_min_size(
                        layout.base.min - egui::vec2(0.0, 40.0),
                        egui::vec2(layout.base.width(), 40.0),
                    );
                    visible_regions.push(warning);
                    ui.painter().rect_filled(warning, 0, Color32::from_gray(32));
                    let mut warning_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(warning.shrink(5.0)));
                    warning_ui.label(RichText::new(message).size(11.0).color(if red {
                        Color32::from_rgb(230, 140, 130)
                    } else {
                        Color32::from_gray(220)
                    }));
                }
            });
        ctx.memory(|memory| {
            for layer in memory.areas().visible_layer_ids() {
                if layer.order == egui::Order::Tooltip {
                    if let Some(rect) = memory.area_rect(layer.id) {
                        visible_regions.push(rect);
                    }
                }
            }
        });
        // The in-game overlay hides once its strip has faded out.  The strip's
        // X can have hidden it just now.
        let present = self
            .live
            .as_ref()
            .is_none_or(|live| expanded || self.motion.animating() || live.strip.shown(now));
        if !present {
            visible_regions.clear();
        }
        match platform::update_input_regions(&ctx, &visible_regions) {
            Ok(()) => self.region_update_failed = false,
            Err(error) => {
                if !self.region_update_failed {
                    tracing::warn!(%error, "Could not update overlay input regions");
                }
                self.region_update_failed = true;
            }
        }
        if self.live.is_some() {
            if !present && self.frames_drawn > 0 && platform::is_visible() {
                tracing::info!("Overlay hidden until Alt+H");
                platform::hide();
            } else if self.frames_drawn == 0 {
                // eframe shows the window after the first frame, so the next
                // one hides it again.
                ctx.request_repaint();
            }
        }
        if self.pointer_completion.finish_frame(pointer_down) {
            ctx.request_repaint();
        }
        // Letters and Space type only while the search is being typed.
        let typing = self.search.typing();
        if let Some(keyboard) = &self.keyboard {
            keyboard.set_typing(typing);
        }
        if let Some(keys) = &mut self.egui_keys {
            keys.set_typing(typing);
        }
        if self.capture.is_some() {
            // Allow the native surface and its initial layout to settle before readback.
            let capture_ready = self.capture_keys.is_empty()
                && self.capture_at.map_or_else(
                    || {
                        self.frames_drawn >= self.capture_frame
                            && (self.capture_frame != 24
                                || (!self.motion.animating()
                                    && !self.samples.animation_pending(now)))
                    },
                    |at| self.started.elapsed() >= at,
                );
            if !self.capture_requested && capture_ready {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                self.capture_requested = true;
            }
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        // The preview only changes mods on screen.  The in-game overlay asks
        // Hestia to make each change.
        let requests = self.samples.take_requests();
        let hotkey_requests = self.samples.take_hotkey_requests();
        if self.live.is_none() && self.sample_hotkeys {
            let answers = hotkey_requests
                .iter()
                .filter_map(|request| match request {
                    FromOverlay::Hotkeys { mod_id } => Some(data::sample_hotkeys(mod_id)),
                    _ => None,
                })
                .collect();
            self.samples.receive_hotkeys(answers);
        }
        if let Some(live) = &mut self.live {
            for request in requests {
                live.link.send_change(live.changes.ask(request, now));
            }
            for request in self.samples.take_gamebanana_requests() {
                live.link.ask(request);
            }
            for request in hotkey_requests {
                live.link.ask(request);
            }
            if let Some(after) = live.changes.next_frame(now) {
                ctx.request_repaint_after(after);
            }
            live.link.report(self.samples.selection());
            live.link.report_typing(self.search.typing());
        }
        self.frames_drawn = self.frames_drawn.saturating_add(1);
    }

    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

/// Register the hotkey and describe a failure for the warning strip.
fn register_hotkey(hotkey: OverlayHotkey) -> Option<TextKey> {
    let error = platform::register_hotkey(hotkey).err()?;
    tracing::warn!(%error, "Could not register the overlay hotkey");
    Some(
        if error.raw_os_error() == Some(HOTKEY_ALREADY_REGISTERED) {
            TextKey::GameOverlayHotkeyTaken
        } else {
            TextKey::GameOverlayHotkeyFailed
        },
    )
}

/// Defer collapsing through the release frame of a gesture begun on the overlay.
/// It does not keep a window open for buttons held elsewhere before it appeared.
#[derive(Default)]
struct PointerCompletion {
    active: bool,
}

/// A/D and the side arrows step through mods or categories, by row.
fn move_in_row(row: layouts::Row, direction: i32) -> keyboard::Command {
    match row {
        layouts::Row::Mods => keyboard::Command::Mod(direction),
        layouts::Row::Categories => keyboard::Command::Category(direction),
    }
}

/// Reads a capture key such as `E`, `Space`, or `Shift+Enter`.
/// `x,y,lines` in a capture run.
fn capture_wheel(value: &str) -> Option<(egui::Pos2, f32)> {
    let mut parts = value.split(',');
    Some((
        egui::pos2(parts.next()?.parse().ok()?, parts.next()?.parse().ok()?),
        parts.next()?.parse::<f32>().ok()?,
    ))
}

/// `hover:x,y` or `click:x,y` in a capture run: whether it clicks, and where.
fn capture_pointer(token: &str) -> Option<(bool, egui::Pos2)> {
    let (click, pos) = match token.strip_prefix("click:") {
        Some(pos) => (true, pos),
        None => (false, token.strip_prefix("hover:")?),
    };
    let (x, y) = pos.split_once(',')?;
    Some((click, egui::pos2(x.parse().ok()?, y.parse().ok()?)))
}

fn push_capture_wheel(input: &mut egui::RawInput, pos: egui::Pos2, delta: f32) {
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, delta),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
}

fn capture_key(token: &str) -> Option<(egui::Key, egui::Modifiers)> {
    let (modifier_names, name) = token.rsplit_once('+').unwrap_or(("", token));
    let mut modifiers = egui::Modifiers::NONE;
    for modifier in modifier_names.split('+').filter(|name| !name.is_empty()) {
        match modifier.to_ascii_lowercase().as_str() {
            "shift" => modifiers.shift = true,
            "ctrl" => modifiers = modifiers | egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            "alt" => modifiers.alt = true,
            _ => return None,
        }
    }
    Some((egui::Key::from_name(name)?, modifiers))
}

/// Presses the hotkey in a capture run.
fn push_hotkey(input: &mut egui::RawInput, hotkey: OverlayHotkey) {
    let Some(key) = hotkey
        .parts()
        .last()
        .and_then(|name| egui::Key::from_name(name))
    else {
        return;
    };
    let modifiers = egui::Modifiers {
        alt: hotkey.alt,
        ctrl: hotkey.ctrl,
        shift: hotkey.shift,
        command: hotkey.ctrl,
        ..egui::Modifiers::NONE
    };
    push_capture_key(input, key, modifiers);
}

/// Presses and releases a key in a capture run.
fn push_capture_key(input: &mut egui::RawInput, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        input.events.push(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        });
    }
}

/// Types text in a capture run like a keyboard does: each character between
/// the press and release of its key, where egui has one.
fn push_capture_text(input: &mut egui::RawInput, text: &str) {
    for character in text.chars() {
        let key = match character {
            ' ' => Some(egui::Key::Space),
            _ => egui::Key::from_name(&character.to_string()),
        };
        let key_event = |pressed| {
            key.map(|key| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        };
        input.events.extend(key_event(true));
        input.events.push(egui::Event::Text(character.to_string()));
        input.events.extend(key_event(false));
    }
}

/// The search field's id, so keys can move the keyboard focus to it.
fn search_id() -> egui::Id {
    egui::Id::new("overlay-preview-search")
}

impl PointerCompletion {
    fn begin_frame(&mut self, was_expanded: bool, pressed_inside: bool) -> bool {
        self.active |= was_expanded && pressed_inside;
        self.active
    }

    fn finish_frame(&mut self, pointer_down: bool) -> bool {
        let completed = self.active && !pointer_down;
        self.active &= pointer_down;
        completed
    }

    fn cancel(&mut self) {
        self.active = false;
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::{
        PointerCompletion, capture_key, keyboard::Command, layouts::Row, move_in_row,
        push_capture_text,
    };

    #[test]
    fn side_steps_follow_the_row() {
        assert_eq!(move_in_row(Row::Mods, -1), Command::Mod(-1));
        assert_eq!(move_in_row(Row::Mods, 1), Command::Mod(1));
        assert_eq!(move_in_row(Row::Categories, -1), Command::Category(-1));
        assert_eq!(move_in_row(Row::Categories, 1), Command::Category(1));
    }

    #[test]
    fn capture_keys_read_names_and_modifiers() {
        use egui::{Key, Modifiers};

        assert_eq!(capture_key("e"), Some((Key::E, Modifiers::NONE)));
        assert_eq!(capture_key("Space"), Some((Key::Space, Modifiers::NONE)));
        assert_eq!(capture_key("Up"), Some((Key::ArrowUp, Modifiers::NONE)));
        assert_eq!(
            capture_key("Shift+Enter"),
            Some((Key::Enter, Modifiers::SHIFT))
        );
        assert_eq!(
            capture_key("ctrl+F"),
            Some((Key::F, Modifiers::CTRL | Modifiers::COMMAND))
        );
        assert_eq!(
            capture_key("Alt+Shift+H"),
            Some((Key::H, Modifiers::ALT | Modifiers::SHIFT))
        );
        assert_eq!(capture_key("Hyper+E"), None);
        assert_eq!(capture_key("Shift+"), None);
        assert_eq!(capture_key("Nope"), None);
    }

    #[test]
    fn capture_text_types_each_character_between_its_key_press_and_release() {
        use egui::{Event, Key, Modifiers, RawInput};

        let key = |key, pressed| Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let text = |text: &str| Event::Text(text.to_owned());
        let mut input = RawInput::default();
        push_capture_text(&mut input, "v W界");
        assert_eq!(
            input.events,
            [
                key(Key::V, true),
                text("v"),
                key(Key::V, false),
                key(Key::Space, true),
                text(" "),
                key(Key::Space, false),
                key(Key::W, true),
                text("W"),
                key(Key::W, false),
                // No key types this one, as with an input method.
                text("界"),
            ]
        );
    }

    #[test]
    fn closing_keeps_click_or_drag_until_its_release_frame() {
        let mut interaction = PointerCompletion::default();
        assert!(interaction.begin_frame(true, true));
        assert!(!interaction.finish_frame(true));
        // The overlay has closed, but the pointer gesture is still in progress.
        assert!(interaction.begin_frame(true, false));
        assert!(!interaction.finish_frame(true));
        // Paint/process the expanded widgets on release, then allow collapse.
        assert!(interaction.begin_frame(true, false));
        assert!(interaction.finish_frame(false));
        assert!(!interaction.begin_frame(true, false));
    }

    #[test]
    fn outside_press_and_escape_do_not_latch_expansion() {
        let mut interaction = PointerCompletion::default();
        assert!(!interaction.begin_frame(false, true));
        assert!(!interaction.begin_frame(true, false));
        assert!(interaction.begin_frame(true, true));
        interaction.cancel();
        assert!(!interaction.begin_frame(false, false));
    }
}

impl OverlayPreview {
    fn apply_transition(&mut self, ctx: &egui::Context, transition: session::Transition) {
        match transition {
            session::Transition::None => return,
            session::Transition::Opened => self.focus_return_pending = false,
            session::Transition::Closed { return_focus } => {
                // Only the Alt+H path takes the keyboard from another window.
                self.focus_return_pending = return_focus && self.keyboard.is_some();
            }
        }
        // Each session starts on the categories row without a search, and a
        // pinned overlay goes back to that once the keyboard is gone.
        self.samples.reset_row();
        self.clear_search(ctx);
        tracing::info!(?transition, session = ?self.session, "Overlay session changed");
    }

    fn clear_search(&mut self, ctx: &egui::Context) {
        self.search.clear();
        ctx.memory_mut(|memory| memory.surrender_focus(search_id()));
    }

    /// Keeps the in-game overlay's strip up while the overlay is in use, and
    /// returns the strip's opacity.  Always 1 in the preview.
    fn update_strip(&mut self, ctx: &egui::Context, now: f64, expanded: bool) -> f32 {
        let Some(live) = &mut self.live else {
            return 1.0;
        };
        // Without the hotkey, the strip is the only way in, so its warning
        // stays.
        if self.hotkey_warning.is_some() && live.strip.shown(now) {
            live.strip.show(now);
        } else if expanded || self.motion.animating() {
            // It stays a few seconds after the overlay closes.
            live.strip.show(now);
        } else if platform::is_visible()
            && ctx.input(|input| input.pointer.hover_pos().is_some() || input.pointer.any_down())
        {
            live.strip.hover(now);
        }
        if let Some(after) = live.strip.next_frame(now) {
            ctx.request_repaint_after(after);
        }
        live.strip.opacity(now)
    }

    /// Takes the settings Hestia sent while the overlay runs.
    fn apply_settings(&mut self, ctx: &egui::Context, settings: Settings, now: f64) {
        let old = std::mem::replace(&mut self.settings, settings);
        LANGUAGE.set(settings.language);
        if let Some(opacity) = settings.opacity {
            self.opacity = clamp_overlay_opacity(opacity);
            if let Some(live) = &mut self.live {
                live.link.received_opacity(self.opacity);
            }
        }
        if settings.hotkey != old.hotkey {
            keyboard::set_hotkey(settings.hotkey);
            if self.keyboard.is_some() {
                self.hotkey_warning = register_hotkey(settings.hotkey);
                if self.hotkey_warning.is_some()
                    && let Some(live) = &mut self.live
                {
                    live.strip.warn(now);
                }
            }
        }
        // Without its button, nothing shows it's pinned.
        if !settings.pin_button {
            self.pinned = false;
        }
        if settings.zoom != old.zoom {
            let zoom = OVERLAY_ZOOM * settings.zoom;
            ctx.set_zoom_factor(zoom);
            platform::resize_canvas(CANVAS_SIZE * zoom);
        }
        // Only on a change, so it doesn't undo a click on the header's
        // button that Hestia hasn't saved yet.
        if self.live.is_some()
            && (settings.gamebanana, settings.all_characters)
                != (old.gamebanana, old.all_characters)
        {
            self.samples
                .set_gamebanana(settings.gamebanana, settings.all_characters);
        }
    }

    /// The idle strip's width, wider for a hotkey with more keys than Alt+H
    /// and narrower without the pin button.
    fn idle_width(&self, ctx: &egui::Context) -> f32 {
        let gap = ctx.global_style().spacing.item_spacing.x;
        let extra =
            keycaps_width(self.settings.hotkey, gap) - keycaps_width(OverlayHotkey::DEFAULT, gap);
        let pin = if self.settings.pin_button {
            0.0
        } else {
            28.0 + gap
        };
        IDLE_SIZE.x + extra.max(0.0) - pin
    }

    /// The in-game overlay's header X: close like Esc, and unpin.
    fn close_live(&mut self, ctx: &egui::Context) {
        self.pinned = false;
        let transition = self.session.escape();
        self.apply_transition(ctx, transition);
        ctx.request_repaint();
    }

    fn hint_mode(&self) -> hints::Mode {
        hints::Mode::new(self.search.typing(), self.search.active())
    }

    fn start_window_drag(&self, ctx: &egui::Context) {
        #[cfg(windows)]
        {
            let _ = ctx;
            // egui-winit gates StartDrag on has_focus() for X11 safety. This
            // Windows overlay usually does not have focus, so use winit's
            // native caption-drag path directly instead of that shared guard.
            if let Some(window) = &self.native_window {
                if let Err(error) = window.drag_window() {
                    tracing::warn!(%error, "Could not drag the overlay window");
                }
            }
        }
        #[cfg(not(windows))]
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    fn show_idle(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
            if let Some(brand) = &self.brand {
                ui.painter().image(
                    brand.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            ui.add_space(8.0);
            for (index, key) in self.settings.hotkey.parts().iter().enumerate() {
                if index > 0 {
                    let (plus_rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 20.0), egui::Sense::hover());
                    ui.painter().text(
                        plus_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "+",
                        egui::FontId::proportional(11.0),
                        content_gray(150, self.opacity),
                    );
                }
                idle_keycap(ui, &key.to_uppercase(), keycap_width(key), self.opacity);
            }
            ui.add(
                egui::Label::new(
                    RichText::new(text(TextKey::GameOverlayToBrowse))
                        .size(11.0)
                        .color(Color32::from_gray(175)),
                )
                .sense(egui::Sense::hover()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close_help = text(if self.live.is_some() {
                    TextKey::GameOverlayHide
                } else {
                    TextKey::GameOverlayClose
                });
                if header_button(ui, lucide_icons::Icon::X, close_help, self.opacity).clicked() {
                    match &mut self.live {
                        Some(live) => {
                            live.strip.dismiss();
                            self.hotkey_warning = None;
                            ui.ctx().request_repaint();
                        }
                        None => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
                    }
                }
                if self.settings.pin_button
                    && header_button(
                        ui,
                        lucide_icons::Icon::Pin,
                        text(TextKey::GameOverlayKeepExpanded),
                        self.opacity,
                    )
                    .clicked()
                {
                    self.pinned = true;
                    ui.ctx().request_repaint();
                }
            });
        });
    }

    fn show_header(&mut self, ui: &mut egui::Ui, opacity: u8, message: Option<&(String, bool)>) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
            let (brand_rect, drag) =
                ui.allocate_exact_size(egui::vec2(78.0, 30.0), egui::Sense::drag());
            if let Some(brand) = &self.brand {
                let logo_rect = egui::Rect::from_center_size(
                    brand_rect.left_center() + egui::vec2(12.0, 0.0),
                    egui::vec2(24.0, 24.0),
                );
                ui.painter().image(
                    brand.id(),
                    logo_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::from_white_alpha(image_alpha(opacity)),
                );
            }
            ui.painter().text(
                brand_rect.left_center() + egui::vec2(31.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "Hestia",
                egui::FontId::proportional(14.0),
                content_color(Color32::from_rgb(210, 189, 156), opacity),
            );
            if drag.drag_started() {
                self.start_window_drag(&ctx);
            }
            let _ = layouts::delayed_tooltip(drag, self.game.clone());
            // The lane holds the search field and the hints.  Reserve exactly
            // the right-hand controls: the 28pt buttons, the 92pt slider, and
            // a 3pt gap before each.  The lane ends at the leftmost control.
            let show_all_button = self.live.is_some() && self.settings.gamebanana;
            let buttons = 2 + usize::from(self.settings.pin_button) + usize::from(show_all_button);
            let controls = buttons as f32 * (28.0 + 3.0)
                + if self.settings.opacity_slider {
                    92.0 + 3.0
                } else {
                    0.0
                };
            let (lane, lane_drag) = ui.allocate_exact_size(
                egui::vec2((ui.available_width() - controls).max(0.0), 30.0),
                egui::Sense::drag(),
            );
            if lane_drag.drag_started() {
                self.start_window_drag(&ctx);
            }
            let hints_rect = self.show_search(ui, lane, opacity);
            // A click can end typing in the field, so the hints follow now.
            let mode = self.hint_mode();
            self.hint_ticker
                .update(self.expanded, mode, ui.input(|input| input.time));
            if let Some((message, red)) = message {
                header_message(ui, hints_rect, message, *red, opacity);
            } else if self.settings.key_hints {
                shortcut_hints(
                    ui,
                    hints_rect,
                    opacity,
                    &self.hint_ticker,
                    self.expanded,
                    mode,
                    self.samples.shortcut_availability(),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if header_button(
                    ui,
                    lucide_icons::Icon::X,
                    text(TextKey::GameOverlayClose),
                    opacity,
                )
                .clicked()
                {
                    if self.live.is_some() {
                        self.close_live(&ctx);
                    } else {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
                let pin_help = text(if self.pinned {
                    TextKey::GameOverlayUnpin
                } else {
                    TextKey::GameOverlayKeepExpanded
                })
                .replace("{key}", &self.settings.hotkey.label());
                if self.settings.pin_button
                    && header_button_state(
                        ui,
                        lucide_icons::Icon::Pin,
                        &pin_help,
                        opacity,
                        self.pinned,
                    )
                    .clicked()
                {
                    self.pinned = !self.pinned;
                    ctx.request_repaint();
                }
                if header_button(
                    ui,
                    lucide_icons::Icon::Maximize2,
                    text(TextKey::GameOverlayOpenHestia),
                    opacity,
                )
                .clicked()
                {
                    let restored = match &self.live {
                        Some(live) => restore::show_host(live.link.host_window()),
                        None => restore::show_main(data::state_path().as_deref()),
                    };
                    match restored {
                        // Hestia has the keyboard now.
                        Ok(()) if self.live.is_some() => {
                            self.restore_error = None;
                            self.pinned = false;
                            let transition = self.session.deactivated();
                            self.apply_transition(&ctx, transition);
                            ctx.request_repaint();
                        }
                        Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
                        Err(error) => {
                            self.restore_error = Some(
                                text(TextKey::GameOverlayCouldNotOpenHestia)
                                    .replace("{error}", &format!("{error:#}")),
                            )
                        }
                    }
                }
                if show_all_button {
                    let show_all = self.samples.show_all_characters();
                    if header_button_state(
                        ui,
                        lucide_icons::Icon::Users,
                        text(TextKey::GameOverlayShowAllCharacters),
                        opacity,
                        show_all,
                    )
                    .clicked()
                    {
                        self.samples.set_show_all_characters(!show_all);
                        self.settings.all_characters = !show_all;
                        if let Some(live) = &mut self.live {
                            live.link.report_all_characters(!show_all);
                        }
                        ctx.request_repaint();
                    }
                }
                if self.settings.opacity_slider {
                    let holding = opacity_slider(ui, &mut self.opacity);
                    // Saved once let go, not at every step of a drag.
                    if let Some(live) = &mut self.live
                        && !holding
                    {
                        live.link.report_opacity(self.opacity);
                    }
                }
                let (_, drag) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(0.0), 30.0),
                    egui::Sense::drag(),
                );
                if drag.drag_started() {
                    self.start_window_drag(&ctx);
                }
            });
        });
    }

    /// The search field, at the start of the hint lane while a search is
    /// typed or shown.  Returns the rest of the lane, for the hints.
    fn show_search(&mut self, ui: &mut egui::Ui, lane: egui::Rect, opacity: u8) -> egui::Rect {
        let active = self.search.active();
        let chip = egui::Rect::from_min_size(
            egui::pos2(lane.left(), lane.center().y - SEARCH_SIZE.y / 2.0),
            egui::vec2(if active { SEARCH_SIZE.x } else { 0.0 }, SEARCH_SIZE.y),
        );
        // Always add the child, so the ids of the widgets after it stay put.
        let mut search_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("overlay-preview-search-lane")
                .max_rect(chip),
        );
        if !active {
            return lane;
        }
        let painter = search_ui.painter().clone();
        // Painted once the field has settled whether it is being typed in.
        let background = painter.add(egui::Shape::Noop);
        painter.text(
            egui::pos2(chip.left() + 11.0, chip.center().y),
            egui::Align2::CENTER_CENTER,
            char::from(lucide_icons::Icon::Search).to_string(),
            egui::FontId::new(12.0, egui::FontFamily::Name("preview-icons".into())),
            content_gray(150, opacity),
        );
        let font = egui::FontId::proportional(12.0);
        let row_height = search_ui.fonts_mut(|fonts| fonts.row_height(&font));
        let field = egui::Rect::from_min_max(
            egui::pos2(chip.left() + 22.0, chip.center().y - row_height / 2.0),
            egui::pos2(chip.right() - 6.0, chip.center().y + row_height / 2.0),
        );
        // Give the field the keyboard before it reads this frame's keys.  It
        // loses it while hidden, for example as the overlay opens.
        if self.search.typing() && !search_ui.memory(|memory| memory.has_focus(search_id())) {
            search_ui.memory_mut(|memory| memory.request_focus(search_id()));
        }
        search_ui.visuals_mut().weak_text_color = Some(content_gray(125, opacity));
        let response = search_ui.put(
            field,
            egui::TextEdit::singleline(self.search.query_mut())
                .id(search_id())
                .frame(egui::Frame::NONE)
                .font(font)
                .text_color(content_gray(235, opacity))
                .hint_text(text(TextKey::GameOverlaySearchHint))
                .char_limit(64)
                .return_key(None)
                .desired_width(field.width()),
        );
        if self.search.typing() {
            // A click elsewhere takes the keyboard, and ends typing like Tab.
            if !search_ui.memory(|memory| memory.has_focus(search_id())) {
                self.search.stop_typing();
            }
        } else if response.has_focus() {
            // A click on the field goes back to typing.
            self.search.start_typing();
        }
        let typing = self.search.typing();
        painter.set(
            background,
            egui::epaint::RectShape::new(
                chip,
                3,
                Color32::from_white_alpha(content_alpha(if typing { 14 } else { 8 }, opacity)),
                egui::Stroke::new(1.0, content_gray(if typing { 150 } else { 70 }, opacity)),
                egui::StrokeKind::Inside,
            ),
        );
        self.samples.set_search(self.search.query());
        lane.with_min_x(chip.right() + 8.0)
    }
}

/// Paints the hints into `rect`, the part of the header lane beside the search
/// field.  The clipped rect stays fixed while a continuous train of hints
/// moves through it.
/// A message in place of the key hints, on up to two lines.
fn header_message(ui: &egui::Ui, rect: egui::Rect, message: &str, red: bool, opacity: u8) {
    let color = content_color(
        if red {
            Color32::from_rgb(230, 140, 130)
        } else {
            Color32::from_gray(220)
        },
        opacity,
    );
    let mut job = egui::text::LayoutJob::simple(
        message.to_owned(),
        egui::FontId::proportional(11.0),
        color,
        (rect.width() - 16.0).max(0.0),
    );
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let position = egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y * 0.5);
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(position, galley, color);
}

fn shortcut_hints(
    ui: &egui::Ui,
    rect: egui::Rect,
    opacity: u8,
    ticker: &hints::Ticker,
    expanded: bool,
    mode: hints::Mode,
    available: layouts::ShortcutAvailability,
) {
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    let entries: Vec<_> = mode
        .sequence()
        .iter()
        .map(|hint| {
            let keys = hint.keys();
            let enabled = hint.enabled(available);
            let group_width = keys.iter().map(|key| hints::key_width(key)).sum::<f32>()
                + 3.0 * (keys.len() - 1) as f32;
            let label = painter.layout_no_wrap(
                hint.label(available).to_owned(),
                egui::FontId::proportional(11.0),
                content_gray(if enabled { 165 } else { 112 }, opacity),
            );
            let span = group_width + 8.0 + label.size().x + 28.0;
            (keys, group_width, label, enabled, span)
        })
        .collect();
    let cycle_width = entries.iter().map(|entry| entry.4).sum();
    let frame = ticker.frame(
        ui.input(|input| input.time),
        ui.style().animation_time > 0.0,
        cycle_width,
    );
    if expanded {
        ui.ctx().request_repaint_after(frame.repaint_after);
    }
    let mut origin = rect.left() + 8.0 - frame.offset;
    // A second copy makes the last hint flow directly into the first without a reset gap.
    for _ in 0..2 {
        for (keys, group_width, label, enabled, span) in &entries {
            if origin + span >= rect.left() && origin < rect.right() {
                let mut x = origin;
                for key in *keys {
                    let key_width = hints::key_width(key);
                    let key_rect = egui::Rect::from_center_size(
                        egui::pos2(x + key_width * 0.5, rect.center().y),
                        egui::vec2(key_width, 20.0),
                    );
                    painter.rect_filled(
                        key_rect,
                        3,
                        content_gray(if *enabled { 43 } else { 36 }, opacity),
                    );
                    painter.text(
                        key_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *key,
                        egui::FontId::proportional(11.0),
                        content_gray(if *enabled { 210 } else { 112 }, opacity),
                    );
                    x += key_width + 3.0;
                }
                painter.galley(
                    egui::pos2(
                        origin + group_width + 8.0,
                        rect.center().y - label.size().y * 0.5,
                    ),
                    label.clone(),
                    Color32::WHITE,
                );
            }
            origin += span;
        }
    }
}

/// A keycap wide enough for its name: ALT is 30, a letter 20.
fn keycap_width(key: &str) -> f32 {
    (9.0 + 7.0 * key.chars().count() as f32).max(20.0)
}

/// The idle strip's keycaps and the pluses between them, with `gap` after
/// each.
fn keycaps_width(hotkey: OverlayHotkey, gap: f32) -> f32 {
    let parts = hotkey.parts();
    let keycaps: f32 = parts.iter().map(|key| keycap_width(key) + gap).sum();
    keycaps + (parts.len() - 1) as f32 * (8.0 + gap)
}

fn idle_keycap(ui: &mut egui::Ui, key: &str, width: f32, opacity: u8) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        3,
        Color32::from_white_alpha(content_alpha(8, opacity)),
        egui::Stroke::new(1.0, Color32::from_white_alpha(content_alpha(36, opacity))),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        key,
        egui::FontId::proportional(10.0),
        content_gray(220, opacity),
    );
}

fn header_button(
    ui: &mut egui::Ui,
    glyph: lucide_icons::Icon,
    help: &str,
    opacity: u8,
) -> egui::Response {
    header_button_state(ui, glyph, help, opacity, false)
}

fn header_button_state(
    ui: &mut egui::Ui,
    glyph: lucide_icons::Icon,
    help: &str,
    opacity: u8,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let emphasis = ui.ctx().animate_bool_with_time(
        response.id.with("emphasis"),
        response.hovered() || response.is_pointer_button_down_on(),
        0.1,
    );
    let accent = Color32::from_rgb(210, 125, 85);
    if selected || emphasis > 0.0 {
        ui.painter().rect_filled(
            rect.shrink(2.0),
            3,
            Color32::from_white_alpha(content_alpha(
                if selected {
                    16
                } else {
                    (12.0 * emphasis) as u8
                },
                opacity,
            )),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        char::from(glyph).to_string(),
        egui::FontId::new(14.0, egui::FontFamily::Name("preview-icons".into())),
        if selected {
            content_color(accent, opacity)
        } else {
            content_gray((120.0 + 115.0 * emphasis) as u8, opacity)
        },
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    layouts::delayed_tooltip(response, help).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn filtered_icon(source: &image::RgbaImage, side: u32) -> egui::ColorImage {
    // Resample premultiplied, linear-light pixels before upload. Minifying the
    // full icon with a single GPU bilinear lookup misses fine flame/border detail.
    let linear = image::ImageBuffer::from_fn(source.width(), source.height(), |x, y| {
        let pixel = source.get_pixel(x, y);
        let alpha = f32::from(pixel[3]) / 255.0;
        let decode = |value: u8| {
            let value = f32::from(value) / 255.0;
            (if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }) * alpha
        };
        image::Rgba([decode(pixel[0]), decode(pixel[1]), decode(pixel[2]), alpha])
    });
    let scaled =
        image::imageops::resize(&linear, side, side, image::imageops::FilterType::Lanczos3);
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for pixel in scaled.pixels() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        for channel in &pixel.0[..3] {
            let value = if alpha > 0.00001 {
                (channel / alpha).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let encoded = if value <= 0.0031308 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            rgba.push((encoded.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        rgba.push((alpha * 255.0).round() as u8);
    }
    egui::ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &rgba)
}

/// Whether the pointer holds the slider.
fn opacity_slider(ui: &mut egui::Ui, opacity: &mut u8) -> bool {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(92.0, 28.0), egui::Sense::click_and_drag());
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() - 38.0, 4.0));
    let previous = *opacity;
    if response.clicked() || response.dragged() {
        response.request_focus();
        if let Some(pointer) = response.interact_pointer_pos() {
            let fraction = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let range = f32::from(OVERLAY_OPACITY_MAX - OVERLAY_OPACITY_MIN);
            *opacity = (f32::from(OVERLAY_OPACITY_MIN) + fraction * range).round() as u8;
        }
    }
    if response.has_focus() {
        ui.input_mut(|input| {
            let modifiers = input.modifiers;
            if input.consume_key(modifiers, egui::Key::ArrowLeft) {
                *opacity = opacity.saturating_sub(1).max(OVERLAY_OPACITY_MIN);
            }
            if input.consume_key(modifiers, egui::Key::ArrowRight) {
                *opacity = opacity.saturating_add(1).min(OVERLAY_OPACITY_MAX);
            }
        });
    }
    if *opacity != previous {
        response.mark_changed();
        ui.ctx().request_repaint();
    }
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            ui.is_enabled(),
            f64::from(*opacity),
            text(TextKey::GameOverlayOpacity),
        )
    });
    let changing =
        response.is_pointer_button_down_on() || response.dragged() || *opacity != previous;
    let mut painter = ui.painter().clone();
    painter.multiply_opacity(opacity_unit(*opacity));
    let fraction = f32::from(*opacity - OVERLAY_OPACITY_MIN)
        / f32::from(OVERLAY_OPACITY_MAX - OVERLAY_OPACITY_MIN);
    let head_x = egui::lerp(track.x_range(), fraction);
    painter.rect_filled(track, 2, Color32::from_gray(60));
    painter.rect_filled(
        egui::Rect::from_min_max(track.min, egui::pos2(head_x, track.max.y)),
        2,
        Color32::from_gray(135),
    );
    let head =
        egui::Rect::from_center_size(egui::pos2(head_x, rect.center().y), egui::vec2(12.0, 12.0));
    painter.rect(
        head,
        8,
        Color32::from_gray(if response.hovered() || response.dragged() {
            78
        } else {
            58
        }),
        egui::Stroke::new(1.0, Color32::from_gray(if changing { 180 } else { 110 })),
        egui::StrokeKind::Inside,
    );
    let holding = response.is_pointer_button_down_on() || response.dragged();
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    holding
}

fn clamp_overlay_opacity(opacity: u8) -> u8 {
    opacity.clamp(OVERLAY_OPACITY_MIN, OVERLAY_OPACITY_MAX)
}

fn opacity_unit(opacity: u8) -> f32 {
    f32::from(clamp_overlay_opacity(opacity)) / 100.0
}

fn image_alpha(opacity: u8) -> u8 {
    (opacity_unit(opacity) * 255.0).round() as u8
}

fn base_opacity_unit(opacity: u8) -> f32 {
    let fraction = f32::from(clamp_overlay_opacity(opacity) - OVERLAY_OPACITY_MIN)
        / f32::from(OVERLAY_OPACITY_MAX - OVERLAY_OPACITY_MIN);
    0.78 + 0.14 * fraction
}

fn content_opacity_unit(opacity: u8) -> f32 {
    0.90 + 0.10 * (opacity_unit(opacity) - 0.50) / 0.44
}

fn content_alpha(base_alpha: u8, opacity: u8) -> u8 {
    (f32::from(base_alpha) * content_opacity_unit(opacity)).round() as u8
}

fn base_alpha(opacity: u8) -> u8 {
    (base_opacity_unit(opacity) * 255.0).round() as u8
}

fn content_gray(value: u8, opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(value, value, value, content_alpha(255, opacity))
}

fn content_color(color: Color32, opacity: u8) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, content_alpha(a, opacity))
}

fn apply_preview_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "preview-icons".into(),
        egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES).into(),
    );
    // Search text can be Chinese, Japanese or Korean.  The bundled font has
    // no Hangul, so Korean comes from Windows.
    fonts.font_data.insert(
        "preview-cjk".into(),
        egui::FontData::from_static(crate::app::CJK_FONT_BYTES).into(),
    );
    let mut fallbacks = vec!["preview-cjk".to_owned()];
    if let Some(korean) = korean_font() {
        fonts.font_data.insert(
            "preview-korean".into(),
            egui::FontData::from_owned(korean).into(),
        );
        fallbacks.push("preview-korean".to_owned());
    }
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .extend(fallbacks.iter().cloned());
    }
    let mut icon_fonts = fonts.families[&egui::FontFamily::Proportional].clone();
    icon_fonts.insert(0, "preview-icons".into());
    fonts
        .families
        .insert(egui::FontFamily::Name("preview-icons".into()), icon_fonts);
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    // Text belongs to its enclosing character row/costume card click target.
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.5;
    style.interaction.tooltip_grace_time = 0.0;
    style.interaction.show_tooltips_only_when_still = true;
    style.spacing.item_spacing = egui::vec2(5.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.scroll.fade.strength = 0.0;
    style.visuals.window_shadow = egui::Shadow::NONE;
    for text_style in [egui::TextStyle::Body, egui::TextStyle::Button] {
        style
            .text_styles
            .insert(text_style, egui::FontId::proportional(13.0));
    }
    style.visuals.override_text_color = Some(Color32::from_gray(232));
    style.visuals.selection.bg_fill = Color32::from_rgb(180, 78, 35);
    style.visuals.window_fill = Color32::from_gray(32);
    style.visuals.widgets.inactive.bg_fill = Color32::from_gray(46);
    style.visuals.widgets.hovered.bg_fill = Color32::from_gray(60);
    style.visuals.widgets.active.bg_fill = Color32::from_gray(74);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::same(7);
    }
    ctx.set_style_of(egui::Theme::Dark, style);
}

#[cfg(windows)]
fn korean_font() -> Option<Vec<u8>> {
    std::fs::read(r"C:\Windows\Fonts\malgun.ttf").ok()
}

#[cfg(not(windows))]
fn korean_font() -> Option<Vec<u8>> {
    None
}
