fn spawn_hotkey_customization_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<HotkeyCustomizationRequest>,
    tx: WorkerTx<HotkeyCustomizationEvent>,
) {
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            let tx = tx.clone();
            let result = handle
                .spawn_blocking(move || process_hotkey_customization_request(request))
                .await;
            match result {
                Ok(event) => {
                    let _ = tx.send(event);
                }
                Err(err) => {
                    let _ = tx.send(HotkeyCustomizationEvent::Failed {
                        mod_id: None,
                        message: format!("hotkey customization worker join failed: {err}"),
                    });
                }
            }
        }
    });
}

fn process_hotkey_customization_request(
    request: HotkeyCustomizationRequest,
) -> HotkeyCustomizationEvent {
    match request {
        HotkeyCustomizationRequest::LoadValues {
            game,
            use_default,
            entry,
        } => match xxmi_persist::read_mod_variables(&game, use_default, &entry) {
            Ok(values) => HotkeyCustomizationEvent::ValuesLoaded {
                mod_id: entry.id,
                ini_hash: entry.ini_hash,
                values,
            },
            Err(err) => HotkeyCustomizationEvent::Failed {
                mod_id: Some(entry.id),
                message: format!("hotkey values could not be read: {err:#}"),
            },
        },
        HotkeyCustomizationRequest::SetValue {
            game,
            use_default,
            entry,
            ini_rel_path,
            var_name,
            value,
            key_spec,
            steps,
            reload_after_write,
        } => {
            if let Some(live_steps) = steps {
                for sent_index in 0..live_steps {
                    match xxmi_persist::send_mod_hotkey_foreground_aware(
                        &game,
                        use_default,
                        &key_spec,
                    ) {
                        Ok(report) if report.message.starts_with("sent ") => {}
                        Ok(report) => {
                            return HotkeyCustomizationEvent::Failed {
                                mod_id: Some(entry.id),
                                message: format!(
                                    "XXMI mod customization ({} / {}): skipped live change after {sent_index} of {live_steps} hotkey(s): {}",
                                    game.definition.id, entry.folder_name, report.message
                                ),
                            };
                        }
                        Err(err) => {
                            return HotkeyCustomizationEvent::Failed {
                                mod_id: Some(entry.id),
                                message: format!(
                                    "XXMI mod customization ({} / {}): live change failed after {sent_index} of {live_steps} hotkey(s): {err:#}",
                                    game.definition.id, entry.folder_name
                                ),
                            };
                        }
                    }
                    if sent_index + 1 < live_steps {
                        std::thread::sleep(Duration::from_millis(80));
                    }
                }
                let status = if live_steps == 0 {
                    "unchanged".to_string()
                } else {
                    format!("sent {live_steps} hotkey(s)")
                };
                return HotkeyCustomizationEvent::ValueFinished {
                    game_id: game.definition.id,
                    folder_name: entry.folder_name,
                    var_name,
                    value,
                    status,
                };
            }

            match xxmi_persist::set_mod_variable(
                &game,
                use_default,
                &entry,
                &ini_rel_path,
                &var_name,
                &value,
            ) {
                Ok(wrote) => {
                    let mut status = match wrote {
                        false => "unchanged".to_string(),
                        true => "wrote".to_string(),
                    };
                    if reload_after_write
                        && wrote
                        && xxmi_persist::game_process_running_for_reload(&game)
                    {
                        match xxmi_persist::send_reload_hotkey_foreground_aware(&game, use_default) {
                            Ok(report) => status = format!("{status}; {}", report.message),
                            Err(err) => status = format!("{status}; reload failed: {err:#}"),
                        }
                    }
                    HotkeyCustomizationEvent::ValueFinished {
                        game_id: game.definition.id,
                        folder_name: entry.folder_name,
                        var_name,
                        value,
                        status,
                    }
                }
                Err(err) => HotkeyCustomizationEvent::Failed {
                    mod_id: Some(entry.id),
                    message: format!(
                        "XXMI mod customization ({} / {}): {var_name} = {value} failed: {err:#}",
                        game.definition.id, entry.folder_name
                    ),
                },
            }
        }
        HotkeyCustomizationRequest::Clear {
            game,
            use_default,
            entry,
            live,
            reload_after_clear,
        } => {
            let result = if live {
                xxmi_persist::clear_mod_variables_live_by_reload_rename(
                    &game,
                    use_default,
                    &entry,
                )
                .map(|report| {
                    let status = if report.cleared { "cleared" } else { "unchanged" };
                    (status.to_string(), report.message)
                })
            } else {
                xxmi_persist::clear_mod_variables(&game, use_default, &entry).map(|cleared| {
                    let mut message = if cleared {
                        "removed".to_string()
                    } else {
                        "unchanged".to_string()
                    };
                    if reload_after_clear
                        && cleared
                        && xxmi_persist::game_process_running_for_reload(&game)
                    {
                        match xxmi_persist::send_reload_hotkey_foreground_aware(&game, use_default)
                        {
                            Ok(report) => message = format!("{message}; {}", report.message),
                            Err(err) => message = format!("{message}; reload failed: {err:#}"),
                        }
                    }
                    ("cleared".to_string(), message)
                })
            };
            match result {
                Ok((status, message)) => HotkeyCustomizationEvent::ClearFinished {
                    mod_id: entry.id,
                    game_id: game.definition.id,
                    folder_name: entry.folder_name,
                    status,
                    message,
                },
                Err(err) => HotkeyCustomizationEvent::Failed {
                    mod_id: Some(entry.id),
                    message: format!(
                        "XXMI mod customization ({} / {}): clear failed: {err:#}",
                        game.definition.id, entry.folder_name
                    ),
                },
            }
        }
        HotkeyCustomizationRequest::RunCommand {
            game,
            use_default,
            mod_id,
            key_spec,
            label,
        } => match xxmi_persist::send_mod_hotkey_foreground_aware(&game, use_default, &key_spec) {
            Ok(report) => HotkeyCustomizationEvent::CommandFinished {
                game_id: game.definition.id,
                label,
                message: report.message,
            },
            Err(err) => HotkeyCustomizationEvent::Failed {
                mod_id: Some(mod_id),
                message: format!("XXMI mod hotkey ({}): {err:#}", game.definition.id),
            },
        },
    }
}
