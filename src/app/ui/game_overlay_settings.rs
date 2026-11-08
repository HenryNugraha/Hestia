#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum HotkeyPicker {
    #[default]
    Idle,
    Listening,
    Refused(TextKey, String),
}

impl HestiaApp {
    fn game_overlay_settings_section(&mut self, ui: &mut Ui, text: TextCatalog) -> bool {
        let mut should_save = false;
        let xxmi = self.selected_game().is_some_and(|game| game.is_xxmi());
        let watched = &self.game_overlay.watched;
        let preview_game = self
            .selected_game()
            .map(|game| game.definition.id.as_str())
            .filter(|id| watched.iter().any(|game| game.id == *id))
            .or_else(|| watched.first().map(|game| game.id.as_str()))
            .map(str::to_owned);
        let prefs = &mut self.state.static_prefs;
        let preview = &mut self.game_overlay.preview;
        let hotkey = prefs.game_overlay_hotkey.parts().join(" + ");
        ui.horizontal(|ui| {
            static_label(
                ui,
                bold(text.settings_game_overlay(), Some(16.0)).underline(),
            );
            let switch_size = egui::vec2(32.0, 16.0);
            let (_, slot) = ui.allocate_space(switch_size);
            let mut switch_ui =
                ui.new_child(egui::UiBuilder::new().max_rect(slot.translate(egui::vec2(0.0, 2.0))));
            if !xxmi {
                switch_ui.disable();
            }
            should_save |=
                toggle_switch_sized(&mut switch_ui, &mut prefs.game_overlay, switch_size)
                    .on_hover_cursor(if xxmi {
                        egui::CursorIcon::PointingHand
                    } else {
                        egui::CursorIcon::NotAllowed
                    })
                    .on_hover_text(
                        text.settings_game_overlay_tooltip()
                            .replace("{key}", &hotkey),
                    )
                    .changed();
        });
        ui.indent("setting_general_game_overlay", |ui| {
            ui.add_enabled_ui(xxmi && prefs.game_overlay, |ui| {
                static_label(ui, text.get(TextKey::SettingsGameOverlayHotkey));
                ui.add_space(-4.0);
                should_save |=
                    game_overlay_hotkey_picker(ui, text, &mut prefs.game_overlay_hotkey, |ui| {
                        if !cfg!(debug_assertions) {
                            return;
                        }
                        if preview.is_some() {
                            if ui
                                .button(icon_text_sized(
                                    Icon::EyeOff,
                                    text.get(TextKey::SettingsGameOverlayStopPreview),
                                    13.0,
                                    13.0,
                                ))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
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
                                egui::Button::new(icon_text_sized(
                                    Icon::Eye,
                                    text.get(TextKey::SettingsGameOverlayPreview),
                                    13.0,
                                    13.0,
                                )),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(text.get(TextKey::SettingsGameOverlayPreviewTooltip))
                            .on_disabled_hover_text(
                                text.get(TextKey::SettingsGameOverlayPreviewUnavailable),
                            )
                            .clicked()
                        {
                            *preview = preview_game;
                        }
                    });
                ui.add_space(8.0);
                should_save |= ui
                    .checkbox(
                        &mut prefs.game_overlay_arrival_strip,
                        text.get(TextKey::SettingsGameOverlayArrivalStrip),
                    )
                    .on_hover_text(
                        text.get(TextKey::SettingsGameOverlayArrivalStripTooltip)
                            .replace("{key}", &hotkey),
                    )
                    .changed();
                ui.add_space(8.0);
                static_label(ui, text.get(TextKey::SettingsGameOverlayCharacterLayout));
                ui.add_space(-4.0);
                should_save |= character_layout_picker(
                    ui,
                    text,
                    &mut prefs.game_overlay_character_layout,
                );
                ui.add_space(8.0);
                static_label(ui, text.get(TextKey::SettingsGameOverlayComponents));
                ui.add_space(-4.0);
                let left_width = settings_column_width(
                    ui,
                    &[
                        text.get(TextKey::SettingsGameOverlayOpacitySlider),
                        text.get(TextKey::SettingsGameOverlayPinButton),
                    ],
                    &[],
                ) + ui.spacing().icon_width
                    + ui.spacing().icon_spacing;
                let rows = [
                    [
                        (
                            &mut prefs.game_overlay_opacity_slider,
                            TextKey::SettingsGameOverlayOpacitySlider,
                            TextKey::SettingsGameOverlayOpacitySliderTooltip,
                        ),
                        (
                            &mut prefs.game_overlay_gamebanana,
                            TextKey::SettingsGameOverlayGameBanana,
                            TextKey::SettingsGameOverlayGameBananaTooltip,
                        ),
                    ],
                    [
                        (
                            &mut prefs.game_overlay_pin_button,
                            TextKey::SettingsGameOverlayPinButton,
                            TextKey::SettingsGameOverlayPinButtonTooltip,
                        ),
                        (
                            &mut prefs.game_overlay_key_hints,
                            TextKey::SettingsGameOverlayKeyHints,
                            TextKey::SettingsGameOverlayKeyHintsTooltip,
                        ),
                    ],
                ];
                for [left, right] in rows {
                    ui.horizontal(|ui| {
                        should_save |=
                            settings_cell(ui, left_width, |ui| settings_checkbox(ui, text, left));
                        should_save |= settings_checkbox(ui, text, right);
                    });
                }
            });
            ui.add_space(1.0);
        });
        ui.add_space(24.0);
        should_save
    }
}

fn character_layout_picker(
    ui: &mut Ui,
    text: TextCatalog,
    layout: &mut crate::model::OverlayCharacterLayout,
) -> bool {
    use crate::model::OverlayCharacterLayout;
    let label = |layout| {
        text.get(match layout {
            OverlayCharacterLayout::Carousel => TextKey::SettingsGameOverlayCharacterLayoutCarousel,
            OverlayCharacterLayout::Grid => TextKey::SettingsGameOverlayCharacterLayoutGrid,
        })
    };
    let before = *layout;
    egui::ComboBox::from_id_salt("settings_game_overlay_character_layout")
        .selected_text(label(*layout))
        .show_ui(ui, |ui| {
            for option in [OverlayCharacterLayout::Carousel, OverlayCharacterLayout::Grid] {
                ui.selectable_value(layout, option, label(option));
            }
        })
        .response
        .on_hover_text(text.get(TextKey::SettingsGameOverlayCharacterLayoutTooltip));
    *layout != before
}

fn settings_cell<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui)
        },
    )
    .inner
}

fn settings_checkbox(
    ui: &mut Ui,
    text: TextCatalog,
    (value, label, tooltip): (&mut bool, TextKey, TextKey),
) -> bool {
    ui.checkbox(value, text.get(label))
        .on_hover_text(text.get(tooltip))
        .changed()
}

fn game_overlay_hotkey_picker(
    ui: &mut Ui,
    text: TextCatalog,
    hotkey: &mut OverlayHotkey,
    after: impl FnOnce(&mut Ui),
) -> bool {
    let id = egui::Id::new("settings_game_overlay_hotkey_picker");
    let mut picker: HotkeyPicker = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let mut changed = false;
    if picker == HotkeyPicker::Listening
        && (!ui.is_enabled() || !game_overlay_hotkey_listening(ui.ctx()))
    {
        picker = HotkeyPicker::Idle;
    }
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
        let keys = if listening {
            text.get(TextKey::SettingsGameOverlayPressKeys).to_owned()
        } else {
            hotkey.parts().join(" + ")
        };
        let response = ui.add(
            egui::Button::new(icon_text_sized(Icon::Keyboard, &keys, 13.0, 13.0))
                .selected(listening),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
        let response = if listening {
            response
        } else {
            response.on_hover_text(text.get(TextKey::SettingsGameOverlayKeyButtonTooltip))
        };
        if response.clicked() {
            picker = if listening {
                HotkeyPicker::Idle
            } else {
                HotkeyPicker::Listening
            };
        } else if listening && response.clicked_elsewhere() {
            picker = HotkeyPicker::Idle;
        }
        if *hotkey != OverlayHotkey::DEFAULT
            && ui
                .button(icon_rich(Icon::RotateCcw, 13.0, ui.visuals().text_color()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(
                    text.get(TextKey::SettingsGameOverlayResetKey)
                        .replace("{key}", &OverlayHotkey::DEFAULT.parts().join(" + ")),
                )
                .clicked()
        {
            *hotkey = OverlayHotkey::DEFAULT;
            changed = true;
            picker = HotkeyPicker::Idle;
        }
        after(ui);
    });
    if let HotkeyPicker::Refused(problem, label) = &picker {
        static_label(
            ui,
            RichText::new(text.get(*problem).replace("{key}", label))
                .size(12.0)
                .color(ui.visuals().error_fg_color),
        );
    }
    let pass = ui.ctx().cumulative_pass_nr();
    ui.data_mut(|data| {
        if picker == HotkeyPicker::Listening {
            data.insert_temp(hotkey_listening_id(), pass);
        } else {
            data.remove::<u64>(hotkey_listening_id());
        }
        data.insert_temp(id, picker);
    });
    changed
}

fn hotkey_listening_id() -> egui::Id {
    egui::Id::new("settings_game_overlay_hotkey_listening")
}

fn game_overlay_hotkey_listening(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<u64>(hotkey_listening_id()))
        .is_some_and(|listened| listened + 1 >= pass)
}

fn take_key_press(ui: &Ui) -> Option<(egui::Key, egui::Modifiers)> {
    ui.input_mut(|input| {
        let index = input.events.iter().position(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } => !matches!(
                key,
                egui::Key::ShiftLeft
                    | egui::Key::ShiftRight
                    | egui::Key::ControlLeft
                    | egui::Key::ControlRight
                    | egui::Key::AltLeft
                    | egui::Key::AltRight
                    | egui::Key::SuperLeft
                    | egui::Key::SuperRight
            ),
            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => true,
            _ => false,
        })?;
        let modifiers = input.modifiers;
        match input.events.remove(index) {
            egui::Event::Key { key, modifiers, .. } => Some((key, modifiers)),
            egui::Event::Copy => Some((egui::Key::C, modifiers)),
            egui::Event::Cut => Some((egui::Key::X, modifiers)),
            egui::Event::Paste(_) => Some((egui::Key::V, modifiers)),
            _ => None,
        }
    })
}

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
