/// What the in-game overlay's hotkey picker is doing.  Kept in egui's
/// memory while Settings is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum HotkeyPicker {
    #[default]
    Idle,
    /// Waiting for the keys.
    Listening,
    /// The keys pressed last, like "Alt+Space", can't open the overlay.
    Refused(TextKey, String),
}

impl HestiaApp {
    /// Settings > General > In-game overlay.  Returns whether a setting
    /// changed.
    fn game_overlay_settings_section(&mut self, ui: &mut Ui, text: TextCatalog) -> bool {
        let mut should_save = false;
        // The preview shows the game picked in Hestia, or else the first one
        // with the overlay.
        let watched = &self.game_overlay.watched;
        let preview_game = self
            .selected_game()
            .map(|game| game.definition.id.as_str())
            .filter(|id| watched.iter().any(|game| game.id == *id))
            .or_else(|| watched.first().map(|game| game.id.as_str()))
            .map(str::to_owned);
        let prefs = &mut self.state.static_prefs;
        let preview = &mut self.game_overlay.preview;
        static_label(
            ui,
            bold(text.settings_game_overlay(), Some(16.0)).underline(),
        );
        ui.indent("setting_general_game_overlay", |ui| {
            let hotkey = prefs.game_overlay_hotkey.label();
            should_save |= ui
                .checkbox(
                    &mut prefs.game_overlay,
                    text.get(TextKey::SettingsGameOverlayEnable),
                )
                .on_hover_text(
                    text.settings_game_overlay_tooltip()
                        .replace("{key}", &hotkey),
                )
                .changed();
            ui.add_enabled_ui(prefs.game_overlay, |ui| {
                ui.add_space(4.0);
                should_save |= game_overlay_hotkey_picker(ui, text, &mut prefs.game_overlay_hotkey);
                ui.add_space(4.0);
                should_save |= ui
                    .checkbox(
                        &mut prefs.game_overlay_arrival_strip,
                        text.get(TextKey::SettingsGameOverlayArrivalStrip),
                    )
                    .on_hover_text(text.get(TextKey::SettingsGameOverlayArrivalStripTooltip))
                    .changed();
                should_save |= ui
                    .checkbox(
                        &mut prefs.game_overlay_close_strip,
                        text.get(TextKey::SettingsGameOverlayCloseStrip),
                    )
                    .on_hover_text(
                        text.get(TextKey::SettingsGameOverlayCloseStripTooltip)
                            .replace("{key}", &hotkey),
                    )
                    .changed();
                should_save |= ui
                    .checkbox(
                        &mut prefs.game_overlay_gamebanana,
                        text.get(TextKey::SettingsGameOverlayGameBanana),
                    )
                    .changed();
                ui.indent("setting_general_game_overlay_gamebanana", |ui| {
                    ui.add_enabled_ui(prefs.game_overlay_gamebanana, |ui| {
                        should_save |= ui
                            .checkbox(
                                &mut prefs.game_overlay_all_characters,
                                text.get(TextKey::SettingsGameOverlayAllCharacters),
                            )
                            .changed();
                    });
                });
                should_save |= ui
                    .checkbox(
                        &mut prefs.game_overlay_key_hints,
                        text.get(TextKey::SettingsGameOverlayKeyHints),
                    )
                    .changed();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    static_label(ui, text.get(TextKey::GameOverlayOpacity));
                    let mut opacity = prefs
                        .game_overlay_opacity
                        .unwrap_or(crate::overlay_preview::DEFAULT_OVERLAY_OPACITY);
                    let response = ui.add(
                        egui::Slider::new(
                            &mut opacity,
                            crate::overlay_preview::OVERLAY_OPACITY_MIN
                                ..=crate::overlay_preview::OVERLAY_OPACITY_MAX,
                        )
                        .suffix("%"),
                    );
                    // A running overlay follows the drag.  Saved once let go.
                    if response.changed() {
                        prefs.game_overlay_opacity = Some(opacity);
                    }
                    should_save |=
                        response.drag_stopped() || (response.changed() && !response.dragged());
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if preview.is_some() {
                        if ui
                            .button(text.get(TextKey::SettingsGameOverlayStopPreview))
                            .clicked()
                        {
                            *preview = None;
                        }
                        static_label(
                            ui,
                            RichText::new(
                                text.get(TextKey::SettingsGameOverlayPreviewHint)
                                    .replace("{key}", &hotkey),
                            )
                            .weak(),
                        );
                    } else if ui
                        .add_enabled(
                            preview_game.is_some(),
                            egui::Button::new(text.get(TextKey::SettingsGameOverlayPreview)),
                        )
                        .on_hover_text(text.get(TextKey::SettingsGameOverlayPreviewTooltip))
                        .on_disabled_hover_text(
                            text.get(TextKey::SettingsGameOverlayPreviewUnavailable),
                        )
                        .clicked()
                    {
                        *preview = preview_game;
                    }
                });
            });
            ui.add_space(1.0);
        });
        ui.add_space(24.0);
        should_save
    }
}

/// "Open with [Alt + H] Change Reset".  Change listens for the next keys,
/// and Esc stops listening.  Returns whether the hotkey changed.
fn game_overlay_hotkey_picker(ui: &mut Ui, text: TextCatalog, hotkey: &mut OverlayHotkey) -> bool {
    let id = egui::Id::new("settings_game_overlay_hotkey_picker");
    let mut picker: HotkeyPicker = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let mut changed = false;
    if picker == HotkeyPicker::Listening
        && let Some((key, modifiers)) = take_key_press(ui)
    {
        picker = if key == egui::Key::Escape && modifiers.is_none() {
            HotkeyPicker::Idle
        } else {
            match pressed_hotkey(key, modifiers) {
                Ok(pressed) if pressed == *hotkey => HotkeyPicker::Idle,
                Ok(pressed) if crate::overlay_preview::hotkey_taken(pressed) => {
                    HotkeyPicker::Refused(TextKey::SettingsGameOverlayKeyTaken, pressed.label())
                }
                Ok(pressed) => {
                    *hotkey = pressed;
                    changed = true;
                    HotkeyPicker::Idle
                }
                Err((problem, label)) => HotkeyPicker::Refused(
                    match problem {
                        OverlayHotkeyProblem::NotAllowed => {
                            TextKey::SettingsGameOverlayKeyNotAllowed
                        }
                        OverlayHotkeyProblem::Reserved => TextKey::SettingsGameOverlayKeyReserved,
                    },
                    label,
                ),
            }
        };
    }
    let listening = picker == HotkeyPicker::Listening;
    ui.horizontal(|ui| {
        static_label(ui, text.get(TextKey::SettingsGameOverlayOpenWith));
        let keys = if listening {
            RichText::new(text.get(TextKey::SettingsGameOverlayPressKeys)).italics()
        } else {
            RichText::new(hotkey.parts().join(" + ")).strong()
        };
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 2))
            .corner_radius(3)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| static_label(ui, keys));
        let button = if listening {
            text.cancel()
        } else {
            text.get(TextKey::SettingsGameOverlayChangeKey)
        };
        if ui.button(button).clicked() {
            picker = if listening {
                HotkeyPicker::Idle
            } else {
                HotkeyPicker::Listening
            };
        }
        if ui
            .add_enabled(
                *hotkey != OverlayHotkey::DEFAULT,
                egui::Button::new(text.get(TextKey::SettingsGameOverlayResetKey)),
            )
            .clicked()
        {
            *hotkey = OverlayHotkey::DEFAULT;
            changed = true;
            picker = HotkeyPicker::Idle;
        }
    });
    if let HotkeyPicker::Refused(problem, label) = &picker {
        static_label(
            ui,
            RichText::new(text.get(*problem).replace("{key}", label))
                .size(12.0)
                .color(ui.visuals().error_fg_color),
        );
    }
    ui.data_mut(|data| data.insert_temp(id, picker));
    changed
}

/// The first key press this frame, taken out of the input so nothing else
/// acts on it.
fn take_key_press(ui: &Ui) -> Option<(egui::Key, egui::Modifiers)> {
    ui.input_mut(|input| {
        let index = input.events.iter().position(|event| {
            matches!(
                event,
                egui::Event::Key {
                    pressed: true,
                    repeat: false,
                    ..
                }
            )
        })?;
        match input.events.remove(index) {
            egui::Event::Key { key, modifiers, .. } => Some((key, modifiers)),
            _ => None,
        }
    })
}

/// The hotkey for a key press, or why it can't be one and the keys' name.
fn pressed_hotkey(
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Result<OverlayHotkey, (OverlayHotkeyProblem, String)> {
    let label = [
        (modifiers.ctrl, "Ctrl"),
        (modifiers.alt, "Alt"),
        (modifiers.shift, "Shift"),
        (true, key.name()),
    ]
    .into_iter()
    .filter(|&(held, _)| held)
    .map(|(_, name)| name)
    .collect::<Vec<_>>()
    .join("+");
    let code = OverlayHotkey::key_code(key.name())
        .ok_or((OverlayHotkeyProblem::NotAllowed, label.clone()))?;
    OverlayHotkey::new(modifiers.ctrl, modifiers.alt, modifiers.shift, code)
        .map_err(|problem| (problem, label))
}
