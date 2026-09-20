//! Disposable native costume-switching preview. Activation changes stay in memory.
//! Launch with `hestia --overlay-preview`; normal startup never enters this module.

mod data;
mod layouts;
mod restore;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::{Color32, RichText, ViewportCommand};

const OVERLAY_OPACITY_MIN: u8 = 50;
const OVERLAY_OPACITY_MAX: u8 = 94;
const DEFAULT_OVERLAY_OPACITY: u8 = 78;
const EXPANDED_SIZE: egui::Vec2 = egui::vec2(560.0, 392.0);
const IDLE_SIZE: egui::Vec2 = egui::vec2(280.0, 48.0);
const CAROUSEL_HEIGHT: f32 = 266.0;
const HEADER_HEIGHT: f32 = 46.0;

pub fn run() -> anyhow::Result<()> {
    let catalog = data::load_catalog();
    let game = catalog.game.clone();
    let capture = std::env::var_os("HESTIA_OVERLAY_PREVIEW_CAPTURE").map(PathBuf::from);
    let pinned = std::env::var_os("HESTIA_OVERLAY_PREVIEW_PINNED").is_some();
    let capture_alt = capture.is_some() && std::env::var_os("HESTIA_OVERLAY_PREVIEW_ALT").is_some();
    let capture_alt_release = capture.as_ref().and_then(|_| {
        std::env::var("HESTIA_OVERLAY_PREVIEW_ALT_RELEASE_FRAME")
            .ok()?
            .parse()
            .ok()
    });
    let opacity = std::env::var("HESTIA_OVERLAY_PREVIEW_OPACITY")
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .map(clamp_overlay_opacity)
        .unwrap_or(DEFAULT_OVERLAY_OPACITY);
    // Optional in-process input for native screenshot checks, never sent to Windows/the game.
    let capture_click = capture.as_ref().and_then(|_| {
        let value = std::env::var("HESTIA_OVERLAY_PREVIEW_CLICK").ok()?;
        let (x, y) = value.split_once(',')?;
        Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
    });
    let capture_text = capture
        .as_ref()
        .and_then(|_| std::env::var("HESTIA_OVERLAY_PREVIEW_TEXT").ok());
    let capture_wheel = capture.as_ref().and_then(|_| {
        let value = std::env::var("HESTIA_OVERLAY_PREVIEW_WHEEL").ok()?;
        let mut parts = value.split(',');
        Some((
            egui::pos2(parts.next()?.parse().ok()?, parts.next()?.parse().ok()?),
            parts.next()?.parse::<f32>().ok()?,
        ))
    });
    let app_icon = eframe::icon_data::from_png_bytes(include_bytes!("asset/icon.png"))?;
    let brand_source =
        image::RgbaImage::from_raw(app_icon.width, app_icon.height, app_icon.rgba.clone())
            .expect("valid embedded app icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hestia — Overlay preview")
            .with_inner_size(if pinned { EXPANDED_SIZE } else { IDLE_SIZE })
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_icon(app_icon),
        // The prototype deliberately does not load or alter the normal renderer preference.
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Hestia overlay preview",
        options,
        Box::new(move |cc| {
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
            let alt_monitor = if capture.is_none() {
                Some(AltMonitor::new(cc.egui_ctx.clone())?)
            } else {
                None
            };
            Ok(Box::new(OverlayPreview {
                game,
                brand: None,
                brand_source,
                brand_pixels: 0,
                restore_error: None,
                samples: layouts::Layouts::new(catalog),
                opacity,
                pinned,
                expanded: pinned,
                suppress_alt_until_release: false,
                capture_alt,
                capture_alt_release,
                alt_monitor,
                capture,
                capture_click,
                capture_hold: capture_text.is_none()
                    && std::env::var_os("HESTIA_OVERLAY_PREVIEW_HOLD").is_some(),
                capture_text,
                capture_wheel,
                capture_requested: false,
                frames_drawn: 0,
                started: Instant::now(),
            }))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

struct OverlayPreview {
    game: String,
    brand: Option<egui::TextureHandle>,
    brand_source: image::RgbaImage,
    brand_pixels: u32,
    restore_error: Option<String>,
    samples: layouts::Layouts,
    opacity: u8,
    pinned: bool,
    expanded: bool,
    suppress_alt_until_release: bool,
    capture_alt: bool,
    capture_alt_release: Option<u32>,
    alt_monitor: Option<AltMonitor>,
    capture: Option<PathBuf>,
    capture_click: Option<egui::Pos2>,
    capture_hold: bool,
    capture_text: Option<String>,
    capture_wheel: Option<(egui::Pos2, f32)>,
    capture_requested: bool,
    frames_drawn: u32,
    started: Instant,
}

impl eframe::App for OverlayPreview {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        if self.capture.is_some() && self.capture_alt {
            input.modifiers.alt = self
                .capture_alt_release
                .is_none_or(|frame| self.frames_drawn < frame);
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
                        button: egui::PointerButton::Primary,
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
        if self.frames_drawn >= 20 {
            if let Some((pos, delta)) = self.capture_wheel.take() {
                input.events.push(egui::Event::PointerMoved(pos));
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, delta),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                });
            }
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
        let brand_pixels = (28.0 * ctx.pixels_per_point()).round().max(1.0) as u32;
        if self.brand_pixels != brand_pixels {
            let icon_image = filtered_icon(&self.brand_source, brand_pixels);
            self.brand =
                Some(ctx.load_texture("hestia-brand", icon_image, egui::TextureOptions::LINEAR));
            self.brand_pixels = brand_pixels;
        }
        let alt = if self.capture.is_some() {
            ctx.input(|input| input.modifiers.alt)
        } else {
            self.alt_monitor
                .as_ref()
                .is_some_and(|monitor| monitor.held(&ctx))
        };
        if !alt {
            self.suppress_alt_until_release = false;
        }
        let escape = ctx.input_mut(|input| {
            let modifiers = input.modifiers;
            input.consume_key(modifiers, egui::Key::Escape)
        });
        if escape {
            self.pinned = false;
            self.suppress_alt_until_release = alt;
        }
        let expanded = self.pinned || (alt && !self.suppress_alt_until_release);
        if expanded != self.expanded {
            let size = if expanded { EXPANDED_SIZE } else { IDLE_SIZE };
            if let Some(outer) = ctx.input(|input| input.viewport().outer_rect) {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
                    outer.center().x - size.x / 2.0,
                    outer.bottom() - size.y,
                )));
            }
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            self.expanded = expanded;
            ctx.request_repaint();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                let bounds = ui.max_rect();
                if self.expanded {
                    let gallery = egui::Rect::from_min_size(
                        bounds.min,
                        egui::vec2(bounds.width(), CAROUSEL_HEIGHT),
                    );
                    let header = egui::Rect::from_min_size(
                        egui::pos2(bounds.left(), bounds.top() + CAROUSEL_HEIGHT),
                        egui::vec2(bounds.width(), HEADER_HEIGHT),
                    );
                    let strip = egui::Rect::from_min_max(
                        egui::pos2(bounds.left(), header.bottom()),
                        bounds.max,
                    );
                    // Artwork is drawn directly over the game. Only the bottom strip has a base.
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(header.min, bounds.max),
                        0,
                        Color32::from_rgba_unmultiplied(32, 32, 32, base_alpha(self.opacity)),
                    );
                    let mut gallery_ui = ui.new_child(egui::UiBuilder::new().max_rect(gallery));
                    let mut header_ui = ui.new_child(
                        egui::UiBuilder::new().max_rect(header.shrink2(egui::vec2(10.0, 8.0))),
                    );
                    self.show_header(&mut header_ui, self.opacity);
                    let mut strip_ui = ui.new_child(
                        egui::UiBuilder::new().max_rect(strip.shrink2(egui::vec2(10.0, 3.0))),
                    );
                    self.samples
                        .show_category_strip(&mut strip_ui, self.opacity);
                    // Header inputs consume their keys first; category changes update the
                    // carousel in the same frame.
                    self.samples.show_carousel(&mut gallery_ui, self.opacity);
                } else {
                    let drag =
                        ui.interact(bounds, ui.id().with("mini-strip-drag"), egui::Sense::drag());
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.painter().rect_filled(
                        bounds,
                        0,
                        Color32::from_rgba_unmultiplied(32, 32, 32, base_alpha(self.opacity)),
                    );
                    let mut idle_ui = ui.new_child(
                        egui::UiBuilder::new().max_rect(bounds.shrink2(egui::vec2(10.0, 8.0))),
                    );
                    self.show_idle(&mut idle_ui);
                }
                if let Some(error) = &self.restore_error {
                    let warning =
                        egui::Rect::from_min_size(bounds.min, egui::vec2(bounds.width(), 40.0));
                    ui.painter().rect_filled(warning, 0, Color32::from_gray(32));
                    let mut warning_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(warning.shrink(5.0)));
                    warning_ui.label(
                        RichText::new(error)
                            .size(11.0)
                            .color(Color32::from_rgb(230, 140, 130)),
                    );
                }
            });
        if self.capture.is_some() {
            // Allow the native surface and its initial layout to settle before readback.
            if !self.capture_requested && self.frames_drawn >= 24 {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                self.capture_requested = true;
            }
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        self.frames_drawn = self.frames_drawn.saturating_add(1);
    }

    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

impl OverlayPreview {
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
            ui.add(
                egui::Label::new(RichText::new("ALT").strong().size(11.0))
                    .sense(egui::Sense::hover()),
            );
            ui.add(
                egui::Label::new(
                    RichText::new("hold to browse")
                        .size(11.0)
                        .color(Color32::from_gray(175)),
                )
                .sense(egui::Sense::hover()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if header_button(ui, lucide_icons::Icon::X, "Close overlay", self.opacity).clicked()
                {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
                if header_button(
                    ui,
                    lucide_icons::Icon::Maximize2,
                    "Keep expanded",
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

    fn show_header(&mut self, ui: &mut egui::Ui, opacity: u8) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
            let (brand_rect, drag) =
                ui.allocate_exact_size(egui::vec2(88.0, 30.0), egui::Sense::drag());
            if let Some(brand) = &self.brand {
                let logo_rect = egui::Rect::from_center_size(
                    brand_rect.left_center() + egui::vec2(14.0, 0.0),
                    egui::vec2(28.0, 28.0),
                );
                ui.painter().image(
                    brand.id(),
                    logo_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::from_white_alpha(image_alpha(opacity)),
                );
            }
            ui.painter().text(
                brand_rect.left_center() + egui::vec2(36.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "Hestia",
                egui::FontId::proportional(16.0),
                content_color(Color32::from_rgb(210, 189, 156), opacity),
            );
            if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            let game = ui.add(
                egui::Label::new(
                    RichText::new(&self.game)
                        .size(11.0)
                        .color(content_gray(155, opacity)),
                )
                .sense(egui::Sense::drag()),
            );
            if game.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if header_button(ui, lucide_icons::Icon::X, "Close overlay", opacity).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                let pin_icon = if self.pinned {
                    lucide_icons::Icon::PinOff
                } else {
                    lucide_icons::Icon::Pin
                };
                let pin_help = if self.pinned {
                    "Unpin · return to hold Alt"
                } else {
                    "Keep expanded"
                };
                if header_button(ui, pin_icon, pin_help, opacity).clicked() {
                    self.pinned = !self.pinned;
                    ctx.request_repaint();
                }
                if header_button(
                    ui,
                    lucide_icons::Icon::Maximize2,
                    "Restore main Hestia window",
                    opacity,
                )
                .clicked()
                {
                    let profile = data::state_path();
                    match restore::show_main(profile.as_deref()) {
                        Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
                        Err(error) => {
                            self.restore_error = Some(format!("Could not open Hestia: {error:#}"))
                        }
                    }
                }
                opacity_slider(ui, &mut self.opacity);
                let (_, drag) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(0.0), 30.0),
                    egui::Sense::drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            });
        });
    }
}

fn header_button(
    ui: &mut egui::Ui,
    glyph: lucide_icons::Icon,
    help: &str,
    opacity: u8,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let emphasis = ui.ctx().animate_bool_with_time(
        response.id.with("emphasis"),
        response.hovered() || response.is_pointer_button_down_on(),
        0.1,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        char::from(glyph).to_string(),
        egui::FontId::new(14.0, egui::FontFamily::Name("preview-icons".into())),
        content_gray((120.0 + 115.0 * emphasis) as u8, opacity),
    );
    response
        .on_hover_text(help)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
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

fn opacity_slider(ui: &mut egui::Ui, opacity: &mut u8) {
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
    response
        .widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*opacity), "Opacity"));
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
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
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

/// Read the physical key off the UI thread; repaint only when it changes.
/// No synthetic input, focus changes, or global hotkey registration are involved.
struct AltMonitor {
    #[cfg(windows)]
    held: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(windows)]
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(windows)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AltMonitor {
    fn new(ctx: egui::Context) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            use std::sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            };
            use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
            let held = Arc::new(AtomicBool::new(false));
            let stop = Arc::new(AtomicBool::new(false));
            let thread_held = held.clone();
            let thread_stop = stop.clone();
            let thread = std::thread::Builder::new()
                .name("hestia-overlay-alt".into())
                .spawn(move || {
                    while !thread_stop.load(Ordering::Relaxed) {
                        let down = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) < 0 };
                        if thread_held.swap(down, Ordering::Relaxed) != down {
                            ctx.request_repaint();
                        }
                        std::thread::sleep(Duration::from_millis(16));
                    }
                })?;
            Ok(Self {
                held,
                stop,
                thread: Some(thread),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = ctx;
            Ok(Self {})
        }
    }

    fn held(&self, ctx: &egui::Context) -> bool {
        #[cfg(windows)]
        {
            let _ = ctx;
            self.held.load(std::sync::atomic::Ordering::Relaxed)
        }
        #[cfg(not(windows))]
        ctx.input(|input| input.modifiers.alt)
    }
}

impl Drop for AltMonitor {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
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
