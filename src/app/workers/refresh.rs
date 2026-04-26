fn spawn_selected_game_refresh_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<RefreshRequest>,
    tx: WorkerTx<RefreshEvent>,
) {
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            let RefreshRequest {
                game_id,
                games,
                use_default_mods_path,
                existing_mods,
            } = request;
            let game_id_for_worker = game_id.clone();
            let result = handle
                .spawn_blocking(move || -> Result<Vec<ModEntry>> {
                    let mut temp_state = AppState::default();
                    temp_state.games = games;
                    temp_state.use_default_mods_path = use_default_mods_path;
                    temp_state.mods = existing_mods;
                    xxmi::refresh_state(&mut temp_state, Some(&game_id_for_worker))?;
                    Ok(temp_state
                        .mods
                        .into_iter()
                        .filter(|m| m.game_id == game_id_for_worker)
                        .collect())
                })
                .await;
            match result {
                Ok(Ok(mods)) => {
                    let _ = tx.send(RefreshEvent::Ready { game_id, mods });
                }
                Ok(Err(err)) => {
                    let _ = tx.send(RefreshEvent::Failed {
                        game_id,
                        error: format!("{err:#}"),
                    });
                }
                Err(err) => {
                    let _ = tx.send(RefreshEvent::Failed {
                        game_id,
                        error: format!("refresh join failed: {err}"),
                    });
                }
            }
        }
    });
}

