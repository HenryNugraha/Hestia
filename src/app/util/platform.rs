fn install_lucide_font(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        LUCIDE_FAMILY.to_string(),
        FontData::from_static(LUCIDE_FONT_BYTES).into(),
    );

    for (name, path) in [
        ("segoe_ui", "C:\\Windows\\Fonts\\segoeui.ttf"),
        ("segoe_ui_bold", "C:\\Windows\\Fonts\\segoeuib.ttf"),
    ] {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(name.to_string(), FontData::from_owned(bytes).into());
        }
    }

    if fonts.font_data.contains_key("segoe_ui") {
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe_ui".to_owned());
    }
    if fonts.font_data.contains_key("segoe_ui_bold") {
        fonts.families.insert(
            FontFamily::Name("Bold".into()),
            vec!["segoe_ui_bold".to_owned()],
        );
    }

    for (font_name, font_path) in [
        ("malgun", "C:\\Windows\\Fonts\\malgun.ttf"),
        ("msyh", "C:\\Windows\\Fonts\\msyh.ttc"),
        ("msgothic", "C:\\Windows\\Fonts\\msgothic.ttc"),
        ("yugothr", "C:\\Windows\\Fonts\\YuGothR.ttc"),
    ] {
        if let Ok(bytes) = fs::read(font_path) {
            fonts.font_data.insert(
                font_name.to_string(),
                FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push(font_name.to_string());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push(font_name.to_string());
            if let Some(bold_family) = fonts.families.get_mut(&FontFamily::Name("Bold".into())) {
                bold_family.push(font_name.to_string());
            }
        }
    }

    fonts
        .families
        .entry(FontFamily::Name(LUCIDE_FAMILY.into()))
        .or_default()
        .push(LUCIDE_FAMILY.to_string());
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(Color32::from_rgb(228, 231, 235));
    style.visuals.panel_fill = Color32::from_rgba_premultiplied(24, 26, 29, 242);
    style.visuals.window_fill = Color32::from_rgba_premultiplied(24, 26, 29, 242);
    style.visuals.faint_bg_color = Color32::from_rgba_premultiplied(36, 39, 43, 242);
    style.visuals.extreme_bg_color = Color32::from_rgba_premultiplied(20, 22, 25, 242);
    style.visuals.code_bg_color = Color32::from_rgba_premultiplied(31, 33, 37, 242);
    style.visuals.widgets.noninteractive.bg_fill =
        Color32::from_rgba_premultiplied(31, 33, 37, 242);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgba_premultiplied(44, 47, 52, 242);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(58, 62, 68, 242);
    style.visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(71, 76, 83, 242);
    style.visuals.widgets.open.bg_fill = Color32::from_rgba_premultiplied(51, 55, 61, 242);
    style.visuals.selection.bg_fill = Color32::from_rgba_premultiplied(180, 78, 35, 242);
    style.visuals.window_stroke.color = Color32::from_rgb(63, 67, 73);
    style.visuals.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(56, 60, 66);
    style.visuals.widgets.inactive.bg_stroke.color = Color32::from_rgb(69, 74, 81);
    style.visuals.widgets.hovered.bg_stroke.color = Color32::from_rgb(92, 98, 107);
    style.visuals.widgets.active.bg_stroke.color = Color32::from_rgb(114, 121, 131);
    style.visuals.widgets.open.bg_stroke.color = Color32::from_rgb(92, 98, 107);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.visuals.window_corner_radius = egui::CornerRadius::same(14);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(12);
    style.visuals.window_shadow = eframe::epaint::Shadow::NONE;
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(12);
    let visuals = style.visuals.clone();
    ctx.set_style(style);
    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);
}

fn prepare_initial_window_placement(
    cc: &eframe::CreationContext<'_>,
    state: &AppState,
) {
    if !state.window_maximized {
        return;
    }

    let Ok(window_handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return;
    };

    let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    apply_startup_window_background(hwnd);
    let mut placement = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT::default();
    placement.length =
        std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>() as u32;

    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut placement).is_err()
        {
            return;
        }
    }

    if let (Some([x, y]), Some([w, h])) = (state.window_pos, state.window_size) {
        placement.rcNormalPosition = RECT {
            left: x.round() as i32,
            top: y.round() as i32,
            right: (x + w).round() as i32,
            bottom: (y + h).round() as i32,
        };
    }
    placement.showCmd = windows::Win32::UI::WindowsAndMessaging::SW_SHOWMAXIMIZED.0 as u32;

    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPlacement(hwnd, &placement);
    }
}

fn apply_startup_window_background(hwnd: HWND) {
    static STARTUP_BACKGROUND_BRUSH: Lazy<isize> = Lazy::new(|| unsafe {
        windows::Win32::Graphics::Gdi::CreateSolidBrush(windows::Win32::Foundation::COLORREF(
            0x1A1A1A,
        ))
        .0 as isize
    });

    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GCLP_HBRBACKGROUND,
            *STARTUP_BACKGROUND_BRUSH,
        );
    }
}

fn load_exe_icon_color_image(path: &Path, size: u32) -> Option<egui::ColorImage> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, GetDC, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};

    if !path.is_file() {
        return None;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut icon = HICON::default();
    let extracted = unsafe {
        ExtractIconExW(
            PCWSTR(wide.as_ptr()),
            0,
            Some(&mut icon),
            None,
            1,
        )
    };
    if extracted == 0 || icon.0.is_null() {
        return None;
    }
    let screen_dc = unsafe { GetDC(None) };
    if screen_dc.0.is_null() {
        unsafe {
            let _ = DestroyIcon(icon);
        }
        return None;
    }

    let mem_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
    if mem_dc.0.is_null() {
        unsafe {
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyIcon(icon);
        }
        return None;
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size as i32,
        biHeight: -(size as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            Some(screen_dc),
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
    };
    let Ok(bitmap) = bitmap else {
        unsafe {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyIcon(icon);
        }
        return None;
    };
    if bits.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyIcon(icon);
        }
        return None;
    }

    let previous = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    let pixel_count = usize::try_from(size).ok()?.saturating_mul(usize::try_from(size).ok()?);
    unsafe {
        std::ptr::write_bytes(bits, 0, pixel_count.saturating_mul(4));
    }
    let drawn = unsafe {
        DrawIconEx(
            mem_dc,
            0,
            0,
            icon,
            size as i32,
            size as i32,
            0,
            None,
            DI_NORMAL,
        )
    }
    .is_ok();

    let rgba = if drawn {
        let bgra = unsafe { std::slice::from_raw_parts(bits as *const u8, pixel_count * 4) };
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        Some(rgba)
    } else {
        None
    };

    unsafe {
        let _ = SelectObject(mem_dc, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        let _ = DestroyIcon(icon);
    }

    let rgba = rgba?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [usize::try_from(size).ok()?, usize::try_from(size).ok()?],
        &rgba,
    ))
}

fn open_external_url(url: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = OsStr::new(url).encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        bail!("failed to open browser");
    }
    Ok(())
}

fn open_in_explorer(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("path does not exist");
    }
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|err| anyhow!("failed to open explorer: {err}"))?;
    Ok(())
}

fn status_color(status: &ModStatus) -> Color32 {
    match status {
        ModStatus::Active => Color32::from_rgb(102, 196, 132),
        ModStatus::Disabled => Color32::from_rgb(217, 174, 76),
        ModStatus::Archived => Color32::from_gray(130),
    }
}

fn pick_most_recent_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for path in paths {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let should_replace = best
            .as_ref()
            .map(|(best_time, _)| modified > *best_time)
            .unwrap_or(true);
        if should_replace {
            best = Some((modified, path.clone()));
        }
    }
    best.map(|(_, path)| path)
}

fn load_xxmi_config() -> Option<(PathBuf, serde_json::Value)> {
    let mut candidates = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(appdata)
                .join("XXMI Launcher")
                .join("XXMI Launcher Config.json"),
        );
    }
    if let Some(localappdata) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(localappdata)
                .join("XXMI Launcher")
                .join("XXMI Launcher Config.json"),
        );
    }
    for path in candidates {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        return Some((path, value));
    }
    None
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 * 900.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    } else if bytes_per_sec < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB/s", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    }
}

fn xxmi_launcher_exe_candidates(config_path: &Path) -> Vec<PathBuf> {
    let Some(root) = config_path.parent() else {
        return Vec::new();
    };
    let mut candidates = vec![
        root.join("Resources").join("Bin").join("XXMI Launcher.exe"),
        root.join("Resources").join("Bin").join("XXMI-Launcher.exe"),
    ];
    candidates.retain(|path| path.is_file());
    candidates
}

fn xxmi_game_exe_candidates(config: &serde_json::Value, xxmi_code: &str) -> Vec<PathBuf> {
    let Some(importer) = config
        .get("Importers")
        .and_then(|value| value.get(xxmi_code))
        .and_then(|value| value.get("Importer"))
    else {
        return Vec::new();
    };

    let Some(game_folder) = importer.get("game_folder").and_then(|value| value.as_str()) else {
        return Vec::new();
    };
    if game_folder.is_empty() {
        return Vec::new();
    }
    let root = PathBuf::from(game_folder);

    let exe_names: Vec<_> = importer
        .get("game_exe_names")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .collect();
    if exe_names.is_empty() {
        return Vec::new();
    }

    let folder_names: Vec<_> = importer
        .get("game_folder_names")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .collect();

    let mut candidates = Vec::new();
    for exe_name in &exe_names {
        candidates.push(root.join(exe_name));
        for folder_name in &folder_names {
            candidates.push(root.join(folder_name).join(exe_name));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn resolve_last_selected_game(state: &AppState) -> Option<usize> {
    let target = state.last_selected_game_id.as_ref()?;
    state
        .games
        .iter()
        .position(|game| &game.definition.id == target)
}

fn shared_blocking_http_client() -> Result<&'static reqwest::blocking::Client> {
    static CLIENT: Lazy<Option<reqwest::blocking::Client>> = Lazy::new(|| {
        reqwest::blocking::Client::builder()
            .user_agent(gamebanana::USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()
            .ok()
    });
    CLIENT
        .as_ref()
        .ok_or_else(|| anyhow!("failed to initialize shared HTTP client"))
}
