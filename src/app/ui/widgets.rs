fn mode_icon_button(
    ui: &mut Ui,
    current: &mut ViewMode,
    target: ViewMode,
    icon: Icon,
    label: &str,
) {
    let selected = *current == target;
    if nav_rail_button(
        ui,
        icon,
        label,
        selected,
        Color32::from_rgba_premultiplied(180, 78, 35, 242),
        Color32::from_rgba_premultiplied(44, 47, 52, 242),
        Color32::from_rgb(214, 104, 58),
        Color32::from_rgb(69, 74, 81),
        None,
    ) {
        *current = target;
    }
}

fn action_icon_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    tooltip: Option<&str>,
) -> bool {
    nav_rail_button(
        ui,
        icon,
        label,
        selected,
        Color32::from_rgba_premultiplied(66, 70, 76, 242),
        Color32::from_rgba_premultiplied(44, 47, 52, 242),
        Color32::from_rgb(114, 121, 131),
        Color32::from_rgb(69, 74, 81),
        tooltip,
    )
}

fn nav_rail_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    selected_fill: Color32,
    idle_fill: Color32,
    selected_stroke: Color32,
    idle_stroke: Color32,
    tooltip: Option<&str>,
) -> bool {
    let desired_size = Vec2::new(60.0, 72.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    let fill = if selected {
        selected_fill
    } else if response.hovered() {
        idle_fill.gamma_multiply(1.18)
    } else {
        idle_fill
    };
    let stroke = if selected {
        selected_stroke
    } else if response.hovered() {
        Color32::from_rgb(96, 102, 111)
    } else {
        idle_stroke
    };
    let text_color = if selected {
        Color32::from_rgb(234, 237, 241)
    } else if response.hovered() {
        Color32::from_rgb(212, 217, 223)
    } else {
        Color32::from_rgb(188, 193, 199)
    };

    ui.painter().rect(
        rect,
        egui::CornerRadius::same(14),
        fill,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 23.0),
        egui::Align2::CENTER_CENTER,
        icon_char(icon),
        egui::FontId::new(22.0, FontFamily::Name(LUCIDE_FAMILY.into())),
        text_color,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 52.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        text_color,
    );

    response
        .on_hover_text(tooltip.unwrap_or(label))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn titlebar_control_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let button_size = Vec2::new(32.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());
    let hovered = response.hovered();
    let is_close = label == "Close";

    if hovered {
        let (bg, border, fg) = if is_close {
            (
                Color32::from_rgba_premultiplied(220, 53, 69, 200),
                Color32::from_rgba_premultiplied(220, 53, 69, 255),
                Color32::WHITE,
            )
        } else {
            (
                Color32::from_rgba_premultiplied(50, 55, 62, 100),
                Color32::from_rgba_premultiplied(100, 110, 120, 150),
                Color32::from_rgb(225, 229, 233),
            )
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(8),
            bg,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon_char(icon),
            egui::FontId::new(15.0, FontFamily::Name(LUCIDE_FAMILY.into())),
            fg,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon_char(icon),
            egui::FontId::new(15.0, FontFamily::Name(LUCIDE_FAMILY.into())),
            Color32::from_rgb(225, 229, 233),
        );
    }

    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn titlebar_action_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    max_label_lines: usize,
) -> egui::Response {
    titlebar_action_button_with_spinner(ui, icon, label, max_label_lines, false)
}

fn tool_extension_tile_lines(path: &Path) -> [String; 3] {
    let raw = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
        .filter(|ext| !ext.trim().is_empty())
        .unwrap_or_else(|| "file".to_string());
    let truncated = raw.chars().count() > 12;
    let display = if truncated {
        let mut base: String = raw.chars().take(9).collect();
        base.push_str("...");
        base
    } else {
        raw
    };
    let mut lines = [String::new(), String::new(), String::new()];
    let chars: Vec<char> = display.chars().collect();
    for (idx, chunk) in chars.chunks(4).take(3).enumerate() {
        lines[idx] = chunk.iter().collect();
    }
    lines
}

fn paint_tool_extension_tile(
    ui: &mut Ui,
    rect: egui::Rect,
    path: &Path,
    text_color: Color32,
) {
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(8),
        Color32::from_rgb(196, 201, 208),
        egui::Stroke::new(1.0, Color32::from_rgb(150, 157, 166)),
        egui::StrokeKind::Inside,
    );
    let lines = tool_extension_tile_lines(path);
    let visible_lines: Vec<&String> = lines.iter().filter(|line| !line.is_empty()).collect();
    let line_count = visible_lines.len().max(1);
    let font_size = if rect.width() >= 80.0 { 18.0 } else { 9.5 };
    let line_height = if rect.width() >= 80.0 { 20.0 } else { 10.5 };
    let total_height = line_count as f32 * line_height;
    let start_y = rect.center().y - total_height * 0.5 + line_height * 0.5;
    for (idx, line) in visible_lines.iter().enumerate() {
        ui.painter().text(
            egui::pos2(rect.center().x, start_y + idx as f32 * line_height),
            egui::Align2::CENTER_CENTER,
            line,
            egui::FontId::monospace(font_size),
            text_color,
        );
    }
}

fn titlebar_tool_button(
    ui: &mut Ui,
    texture: Option<&egui::TextureHandle>,
    path: &Path,
    tooltip: &str,
    enabled: bool,
    allow_hover_cursor: bool,
) -> egui::Response {
    let desired_size = Vec2::new(46.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let hovered = response.hovered();

    if hovered {
        ui.painter().rect(
            rect.shrink(1.0),
            egui::CornerRadius::same(12),
            Color32::from_rgba_premultiplied(44, 47, 52, 242),
            egui::Stroke::new(1.0, Color32::from_rgb(69, 74, 81)),
            egui::StrokeKind::Inside,
        );
    }

    let icon_size = 24.0;
    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
    if let Some(texture) = texture {
        ui.put(
            icon_rect,
            egui::Image::from_texture(texture)
                .fit_to_exact_size(Vec2::splat(icon_size))
                .corner_radius(egui::CornerRadius::same(6))
                .tint(if enabled {
                    Color32::WHITE
                } else {
                    Color32::from_gray(110)
                }),
        );
    } else {
        paint_tool_extension_tile(
            ui,
            icon_rect,
            path,
            if enabled {
                Color32::from_rgb(58, 62, 68)
            } else {
                Color32::from_gray(110)
            },
        );
    }

    let response = response.on_hover_text(tooltip);
    if allow_hover_cursor {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn titlebar_action_button_with_spinner(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    max_label_lines: usize,
    spinning: bool,
) -> egui::Response {
    let desired_size = Vec2::new(58.0, 64.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    let hovered = response.hovered();

    if hovered {
        ui.painter().rect(
            rect.shrink(1.0),
            egui::CornerRadius::same(12),
            Color32::from_rgba_premultiplied(44, 47, 52, 242),
            egui::Stroke::new(1.0, Color32::from_rgb(69, 74, 81)),
            egui::StrokeKind::Inside,
        );
    }

    let text_color = if hovered {
        Color32::from_rgb(236, 239, 243)
    } else {
        Color32::from_rgb(225, 229, 233)
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 18.0),
        Vec2::new(20.0, 20.0),
    );

    if spinning {
        egui::Spinner::new()
            .size(20.0)
            .color(text_color)
            .paint_at(ui, icon_rect);
    } else {
        ui.painter().text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon_char(icon),
            egui::FontId::new(20.0, FontFamily::Name(LUCIDE_FAMILY.into())),
            text_color,
        );
    }

    let lines: Vec<&str> = label.lines().collect();
    let label_height = max_label_lines.max(1) as f32 * 14.0;
    let start_y = rect.top() + 30.0 + (label_height - lines.len() as f32 * 14.0) * 0.5;
    for (idx, line) in lines.iter().enumerate() {
        ui.painter().text(
            egui::pos2(rect.center().x, start_y + idx as f32 * 14.0 + 7.0),
            egui::Align2::CENTER_CENTER,
            line,
            egui::FontId::proportional(12.0),
            text_color,
        );
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn paint_window_frame(ctx: &egui::Context) {
    if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
        return;
    }

    let rect = ctx.viewport_rect().shrink(WINDOW_INSET as f32 * 0.5);
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect(
        rect,
        egui::CornerRadius::ZERO,
        Color32::TRANSPARENT,
        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(65, 65, 65, 48)),
        egui::StrokeKind::Inside,
    );
}

fn paint_window_background(ctx: &egui::Context) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect_filled(
        ctx.viewport_rect(),
        0.0,
        Color32::from_rgba_premultiplied(24, 26, 29, 242),
    );
}

fn window_drag_strip(ui: &mut Ui, ctx: &egui::Context, height: f32) {
    let width = ui.available_width().max(1.0);
    let (_, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    if response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

fn game_icon_bytes(game_id: &str) -> Option<&'static [u8]> {
    match game_id {
        "wuwa" => Some(include_bytes!("../../asset/game-icon/ww.png")),
        "zzz" => Some(include_bytes!("../../asset/game-icon/zzz.png")),
        "endfield" => Some(include_bytes!("../../asset/game-icon/ef.png")),
        "starrail" => Some(include_bytes!("../../asset/game-icon/hsr.png")),
        "genshin" => Some(include_bytes!("../../asset/game-icon/gi.png")),
        "honkai-impact" => Some(include_bytes!("../../asset/game-icon/hi.png")),
        _ => None,
    }
}

fn game_cover_bytes(game_id: &str) -> Option<&'static [u8]> {
    match game_id {
        "wuwa" => Some(include_bytes!("../../asset/game-cover/ww.jpg")),
        "zzz" => Some(include_bytes!("../../asset/game-cover/zzz.jpg")),
        "endfield" => Some(include_bytes!("../../asset/game-cover/ef.jpg")),
        "starrail" => Some(include_bytes!("../../asset/game-cover/hsr.jpg")),
        "genshin" => Some(include_bytes!("../../asset/game-cover/gi.jpg")),
        "honkai-impact" => Some(include_bytes!("../../asset/game-cover/hi.jpg")),
        _ => None,
    }
}

fn app_icon_bytes() -> &'static [u8] {
    include_bytes!("../../asset/icon.png")
}

fn mod_thumbnail_placeholder_bytes() -> &'static [u8] {
    include_bytes!("../../asset/thumbnail.png")
}

fn youtube_icon_bytes() -> &'static [u8] {
    include_bytes!("../../asset/youtube.png")
}

#[derive(Clone, Copy)]
enum ThumbnailFit {
    Cover,
    Contain,
}

fn paint_thumbnail_image(
    ui: &Ui,
    rect: egui::Rect,
    texture: &egui::TextureHandle,
    fit: ThumbnailFit,
    tint: Color32,
    rounding: impl Into<egui::CornerRadius>,
) {
    let texture_size = texture.size_vec2();
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return;
    }

    let texture_aspect = texture_size.x / texture_size.y;
    let rect_aspect = rect.width() / rect.height();
    let (uv, draw_rect) = match fit {
        ThumbnailFit::Cover => {
            if rect_aspect > texture_aspect {
                let scaled_height = rect.width() / texture_aspect;
                let uv_height_fraction = rect.height() / scaled_height;
                let uv_y_offset = (1.0 - uv_height_fraction) * 0.5;
                (
                    egui::Rect::from_min_max(
                        egui::pos2(0.0, uv_y_offset),
                        egui::pos2(1.0, 1.0 - uv_y_offset),
                    ),
                    rect,
                )
            } else {
                let scaled_width = rect.height() * texture_aspect;
                let uv_width_fraction = rect.width() / scaled_width;
                let uv_x_offset = (1.0 - uv_width_fraction) * 0.5;
                (
                    egui::Rect::from_min_max(
                        egui::pos2(uv_x_offset, 0.0),
                        egui::pos2(1.0 - uv_x_offset, 1.0),
                    ),
                    rect,
                )
            }
        }
        ThumbnailFit::Contain => {
            if rect_aspect > texture_aspect {
                let scaled_width = rect.height() * texture_aspect;
                let x_offset = (rect.width() - scaled_width) * 0.5;
                (
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(x_offset, 0.0),
                        egui::vec2(scaled_width, rect.height()),
                    ),
                )
            } else {
                let scaled_height = rect.width() / texture_aspect;
                let y_offset = (rect.height() - scaled_height) * 0.5;
                (
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(0.0, y_offset),
                        egui::vec2(rect.width(), scaled_height),
                    ),
                )
            }
        }
    };

    egui::Image::from_texture(texture)
        .uv(uv)
        .tint(tint)
        .corner_radius(rounding)
        .paint_at(ui, draw_rect);
}

fn paint_unsafe_overlay(
    ui: &Ui,
    rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    rounding: impl Into<egui::CornerRadius>,
) {
    const UNSAFE_OVERLAY_ZOOM: f32 = 1.4;
    const UNSAFE_OVERLAY_Y_SHIFT: f32 = -0.05;
    let rounding = rounding.into();
    ui.painter().rect_filled(
        rect,
        rounding,
        Color32::from_rgba_premultiplied(12, 12, 14, 180),
    );
    if let Some(texture) = texture {
        let texture_size = texture.size_vec2();
        if texture_size.x > 0.0 && texture_size.y > 0.0 {
            let texture_aspect = texture_size.x / texture_size.y;
            let rect_aspect = rect.width() / rect.height();
            let draw_rect = if rect_aspect > texture_aspect {
                let scaled_width = rect.height() * texture_aspect;
                let x_offset = (rect.width() - scaled_width) * 0.5;
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(x_offset, 0.0),
                    egui::vec2(scaled_width, rect.height()),
                )
            } else {
                let scaled_height = rect.width() / texture_aspect;
                let y_offset = (rect.height() - scaled_height) * 0.5;
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(0.0, y_offset),
                    egui::vec2(rect.width(), scaled_height),
                )
            };
            let zoomed_size = draw_rect.size() * UNSAFE_OVERLAY_ZOOM;
            let zoomed_center = draw_rect.center()
                + egui::vec2(0.0, -draw_rect.height() * UNSAFE_OVERLAY_Y_SHIFT);
            let zoomed_rect = egui::Rect::from_center_size(zoomed_center, zoomed_size);
            ui.painter()
                .with_clip_rect(rect)
                .image(
                    texture.id(),
                    zoomed_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::from_white_alpha(180),
                );
            ui.painter().rect_stroke(
                rect,
                rounding,
                egui::Stroke::new(0.0, Color32::TRANSPARENT),
                egui::StrokeKind::Middle,
            );
        }
    }
}

fn paint_game_icon(
    ui: &mut Ui,
    textures: &HashMap<String, egui::TextureHandle>,
    game_id: &str,
    size: f32,
    tint: Color32,
    sense: Sense,
) -> egui::Response {
    if let Some(texture) = textures.get(game_id) {
        ui.add(
            egui::Image::from_texture(texture)
                .fit_to_exact_size(Vec2::splat(size))
                .corner_radius(egui::CornerRadius::same(6))
                .tint(tint)
                .sense(sense),
        )
    } else {
        ui.allocate_response(Vec2::splat(size), sense)
    }
}

fn paint_tool_icon(
    ui: &mut Ui,
    textures: &HashMap<String, egui::TextureHandle>,
    tool_id: &str,
    path: &Path,
    size: f32,
    tint: Color32,
    sense: Sense,
) -> egui::Response {
    if let Some(texture) = textures.get(tool_id) {
        ui.add(
            egui::Image::from_texture(texture)
                .fit_to_exact_size(Vec2::splat(size))
                .corner_radius(egui::CornerRadius::same(8))
                .tint(tint)
                .sense(sense),
        )
    } else {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
        paint_tool_extension_tile(ui, rect, path, tint);
        response
    }
}

fn game_switcher_button(
    ui: &mut Ui,
    textures: &HashMap<String, egui::TextureHandle>,
    game_id: Option<&str>,
) -> egui::Response {
    let desired_size = Vec2::new(132.0, 120.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    if response.hovered() {
        ui.painter().rect_filled(
            rect.shrink(1.0),
            12.0,
            Color32::from_rgba_premultiplied(44, 47, 52, 242),
        );
    }
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        egui::CornerRadius::same(12),
        egui::Stroke::new(1.0, Color32::from_rgb(69, 74, 81)),
        egui::StrokeKind::Inside,
    );

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 10.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    if let Some(game_id) = game_id {
        let _ = paint_game_icon(
            &mut child,
            textures,
            game_id,
            TITLEBAR_GAME_ICON_SIZE,
            Color32::WHITE,
            Sense::hover(),
        );
    }
    child.add_space(4.0);
    let arrow_size = Vec2::new(12.0, TITLEBAR_GAME_ICON_SIZE);
    let (arrow_rect, _) = child.allocate_exact_size(arrow_size, Sense::hover());
    child.painter().text(
        egui::pos2(arrow_rect.center().x - 8.0, arrow_rect.center().y),
        egui::Align2::CENTER_CENTER,
        icon_char(Icon::ChevronDown),
        egui::FontId::new(24.0, FontFamily::Name(LUCIDE_FAMILY.into())),
        Color32::from_rgb(188, 193, 199),
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn game_grid_card(
    ui: &mut Ui,
    textures: &HashMap<String, egui::TextureHandle>,
    game_id: &str,
    label: &str,
    selected: bool,
) -> egui::Response {
    let desired_size = Vec2::new(272.0, 344.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    let fill = if selected {
        Color32::from_rgba_premultiplied(66, 70, 76, 242)
    } else if response.hovered() {
        Color32::from_rgba_premultiplied(44, 47, 52, 242)
    } else {
        Color32::from_rgba_premultiplied(31, 33, 37, 242)
    };
    let stroke = if selected {
        Color32::from_rgb(140, 146, 154)
    } else {
        Color32::from_rgb(69, 74, 81)
    };

    ui.painter().rect(
        rect.shrink(1.0),
        egui::CornerRadius::same(16),
        fill,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 8.0)))
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    let _ = paint_game_icon(
        &mut child,
        textures,
        game_id,
        GAME_SWITCHER_GRID_ICON_SIZE,
        Color32::WHITE,
        Sense::hover(),
    );
    child.add_space(4.0);
    child.label(RichText::new(label).size(20.0).strong());

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn install_resize_handles(ctx: &egui::Context) {
    if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
        return;
    }

    let rect = ctx.viewport_rect();
    let thickness = 6.0;
    let corner = 12.0;

    resize_handle_area(
        ctx,
        "north",
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + thickness)),
        egui::ResizeDirection::North,
        egui::CursorIcon::ResizeVertical,
    );
    resize_handle_area(
        ctx,
        "south",
        egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - thickness), rect.max),
        egui::ResizeDirection::South,
        egui::CursorIcon::ResizeVertical,
    );
    resize_handle_area(
        ctx,
        "west",
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + thickness, rect.max.y)),
        egui::ResizeDirection::West,
        egui::CursorIcon::ResizeHorizontal,
    );
    resize_handle_area(
        ctx,
        "east",
        egui::Rect::from_min_max(egui::pos2(rect.max.x - thickness, rect.min.y), rect.max),
        egui::ResizeDirection::East,
        egui::CursorIcon::ResizeHorizontal,
    );
    resize_handle_area(
        ctx,
        "north_west",
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + corner, rect.min.y + corner)),
        egui::ResizeDirection::NorthWest,
        egui::CursorIcon::ResizeNwSe,
    );
    resize_handle_area(
        ctx,
        "north_east",
        egui::Rect::from_min_max(
            egui::pos2(rect.max.x - corner, rect.min.y),
            egui::pos2(rect.max.x, rect.min.y + corner),
        ),
        egui::ResizeDirection::NorthEast,
        egui::CursorIcon::ResizeNeSw,
    );
    resize_handle_area(
        ctx,
        "south_west",
        egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - corner),
            egui::pos2(rect.min.x + corner, rect.max.y),
        ),
        egui::ResizeDirection::SouthWest,
        egui::CursorIcon::ResizeNeSw,
    );
    resize_handle_area(
        ctx,
        "south_east",
        egui::Rect::from_min_max(
            egui::pos2(rect.max.x - corner, rect.max.y - corner),
            rect.max,
        ),
        egui::ResizeDirection::SouthEast,
        egui::CursorIcon::ResizeNwSe,
    );
}

fn resize_handle_area(
    ctx: &egui::Context,
    id_suffix: &str,
    rect: egui::Rect,
    direction: egui::ResizeDirection,
    cursor: egui::CursorIcon,
) {
    egui::Area::new(egui::Id::new(("resize_handle", id_suffix)))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
            if response.hovered() || response.dragged() {
                ui.output_mut(|output| output.cursor_icon = cursor);
            }
            if response.drag_started() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        });
}

fn larger_checkbox(ui: &mut Ui, checked: bool) -> egui::Response {
    let size = Vec2::new(24.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let bg_color = if response.hovered() {
        Color32::from_rgba_premultiplied(50, 55, 62, 242)
    } else {
        Color32::from_rgba_premultiplied(40, 44, 50, 242)
    };
    let border_color = if checked {
        Color32::from_rgb(140, 146, 154)
    } else {
        Color32::from_rgb(80, 86, 94)
    };

    ui.painter().rect(
        rect,
        egui::CornerRadius::same(6),
        bg_color,
        egui::Stroke::new(1.5, border_color),
        egui::StrokeKind::Inside,
    );
    if checked {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "✓",
            egui::FontId::new(16.0, FontFamily::Proportional),
            Color32::from_rgb(140, 146, 154),
        );
    }

    response
}

fn static_label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Label::new(text).selectable(false))
        .on_hover_cursor(egui::CursorIcon::Default)
}

fn toggle_switch(ui: &mut Ui, value: &mut bool) -> egui::Response {
    let size = Vec2::new(32.0, 16.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let radius = rect.height() / 2.0;
    let padding = 3.0;
    let mut fill = if *value {
        Color32::from_rgb(62, 165, 98)
    } else {
        Color32::from_rgb(196, 82, 82)
    };
    if response.hovered() {
        fill = fill.gamma_multiply(1.08);
    }

    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, Color32::from_rgb(52, 56, 62)),
        egui::StrokeKind::Inside,
    );

    let knob_radius = radius - padding;
    let knob_x = if *value {
        rect.right() - padding - knob_radius
    } else {
        rect.left() + padding + knob_radius
    };
    let knob_center = egui::pos2(knob_x, rect.center().y);
    ui.painter()
        .circle_filled(knob_center, knob_radius, Color32::from_gray(240));
    ui.painter().circle_stroke(
        knob_center,
        knob_radius,
        egui::Stroke::new(1.0, Color32::from_rgb(56, 60, 66)),
    );

    response
}
