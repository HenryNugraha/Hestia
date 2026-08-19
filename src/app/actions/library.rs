#[derive(Clone, Copy)]
enum ModMutationKind {
    DisableActive,
    EnableIntoActive,
    Delete,
    Rename,
    UpdateExisting,
}

fn push_unique_prefix(prefixes: &mut Vec<String>, prefix: String) {
    if !prefixes
        .iter()
        .any(|existing| xxmi_persist::namespace_prefixes_equal(existing, &prefix))
    {
        prefixes.push(prefix);
    }
}

struct XxmiNamespaceContext {
    shared_prefixes: Vec<String>,
    prefixes_by_root: Vec<(PathBuf, Vec<String>)>,
}

impl HestiaApp {
    fn xxmi_reload_enabled_for_game(&self, game: &GameInstall, trigger: ReloadHotkeyTrigger) -> bool {
        game.is_xxmi()
            && game.apply_mod_changes_in_game
            && self.state.static_prefs.reload_hotkey_triggers.enabled(trigger)
    }

    fn refresh_xxmi_reload_config_for_game(&mut self, game: &GameInstall) {
        if !game.is_xxmi() {
            return;
        }
        match xxmi_persist::ensure_reload_config(
            game,
            self.state.static_prefs.use_default_mods_path,
            game.apply_mod_changes_in_game,
        ) {
            Ok(notices) => {
                for notice in notices {
                    self.report_warn(notice, None);
                }
            }
            Err(err) => {
                self.report_warn(format!("XXMI reload config refresh failed: {err:#}"), None);
            }
        }
        // The folded consent also governs the live-state helper: install it (mirroring the
        // active mods' shown variables) when on, remove it when off.
        self.refresh_live_state_helper_for_game(game);
    }

    fn set_game_reload_preference(&mut self, game_id: &str, enabled: bool) {
        if let Some(game) = self
            .state
            .games
            .iter_mut()
            .find(|game| game.definition.id == game_id)
        {
            game.apply_mod_changes_in_game = enabled;
        }
    }

    fn request_xxmi_reload_setting_change(&mut self, game_id: &str, enable: bool) {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return;
        };
        if !game.is_xxmi() {
            self.set_game_reload_preference(game_id, false);
            return;
        }
        if !enable {
            if self
                .pending_d3dx_foreground_conflict
                .as_ref()
                .is_some_and(|prompt| prompt.game_id == game_id)
            {
                self.pending_d3dx_foreground_conflict = None;
            }
            self.set_game_reload_preference(game_id, false);
            let mut updated = game;
            updated.apply_mod_changes_in_game = false;
            self.refresh_xxmi_reload_config_for_game(&updated);
            return;
        }

        let use_default = self.state.static_prefs.use_default_mods_path;
        match xxmi_persist::reload_config_conflict(&game, use_default) {
            Ok(Some(conflict)) => {
                self.set_game_reload_preference(game_id, false);
                self.pending_d3dx_foreground_conflict = Some(D3dxForegroundConflictPrompt {
                    game_id: game.definition.id.clone(),
                    game_name: game.definition.name.clone(),
                    path: conflict.path,
                    current_value: conflict.current_value,
                });
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.set_game_reload_preference(game_id, false);
                self.report_warn(format!("XXMI reload config check failed: {err:#}"), None);
                return;
            }
        }

        self.set_game_reload_preference(game_id, true);
        let mut updated = game;
        updated.apply_mod_changes_in_game = true;
        self.refresh_xxmi_reload_config_for_game(&updated);
    }

    fn resolve_d3dx_foreground_conflict(&mut self, replace: bool) {
        let Some(prompt) = self.pending_d3dx_foreground_conflict.take() else {
            return;
        };
        if !replace {
            self.set_game_reload_preference(&prompt.game_id, false);
            return;
        }
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == prompt.game_id)
            .cloned()
        else {
            self.set_game_reload_preference(&prompt.game_id, false);
            return;
        };
        // "Replace" is just a consented enable: the normal enable path stashes the user's
        // conflicting foreground value in place and injects Hestia's, reversibly.
        self.set_game_reload_preference(&prompt.game_id, true);
        let mut updated = game;
        updated.apply_mod_changes_in_game = true;
        self.refresh_xxmi_reload_config_for_game(&updated);
    }

    fn mod_action_lock_reason_by_id(
        &self,
        mod_id: &str,
        kind: ModMutationKind,
    ) -> Option<&'static str> {
        let mod_entry = self.state.mods.iter().find(|mod_entry| mod_entry.id == mod_id)?;
        self.mod_action_lock_reason(mod_entry, kind)
    }

    fn mod_action_lock_reason(
        &self,
        mod_entry: &ModEntry,
        kind: ModMutationKind,
    ) -> Option<&'static str> {
        let touches_active_unreal_root = match kind {
            ModMutationKind::DisableActive
            | ModMutationKind::Delete
            | ModMutationKind::Rename
            | ModMutationKind::UpdateExisting => mod_entry.status == ModStatus::Active,
            ModMutationKind::EnableIntoActive => mod_entry.status == ModStatus::Disabled,
        };
        if !touches_active_unreal_root {
            return None;
        }
        let game = self.game_for_mod(mod_entry)?;
        (game.is_unreal_engine() && self.game_process_running(&game))
            .then_some(MODS_LOCKED_BLOCK_REASON)
    }

    fn report_locked_mods(&mut self, toast_summary: Option<&str>) {
        let text = self.text();
        self.report_warn(text.mods_locked_probably_by_game(), toast_summary);
    }

    fn report_skipped_locked_mods(&mut self) {
        let text = self.text();
        self.report_warn(
            text.skipped_locked_mods_probably_by_game(),
            Some(text.mods_locked_probably_by_game()),
        );
    }

    fn mod_action_error_toast<'a>(
        &self,
        err: &anyhow::Error,
        fallback: &'a str,
    ) -> &'a str {
        if Self::looks_like_locked_file_error(err) {
            self.text().mods_locked_probably_by_game()
        } else {
            fallback
        }
    }

    fn looks_like_locked_file_error(err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        })
    }

    fn finish_single_mod_action(&mut self, result: Result<()>, name: &str, action: &str, error_toast: &str) {
        match result {
            Ok(()) => {
                self.log_action(action, name);
                self.set_message_ok(self.text().action_message(action, name));
                self.save_state();
                self.refresh();
            }
            Err(err) => {
                let toast = self.mod_action_error_toast(&err, error_toast);
                self.report_error(err, Some(toast));
            }
        }
    }

    fn finish_batch_mod_action(&mut self, count: usize, action: &str) {
        if count > 0 {
            let text = self.text();
            self.log_action(action, &text.library_mods_count(count));
            self.set_message_ok(text.action_count_message(action, count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
    }

    fn game_for_mod(&self, mod_entry: &ModEntry) -> Option<GameInstall> {
        self.state
            .games
            .iter()
            .find(|game| game.definition.id == mod_entry.game_id)
            .cloned()
    }

    /// One settings-preservation transaction per game/importer. `None` when the feature is
    /// off, the game is not XXMI-backed, or no importer root is discoverable — callers then
    /// run the plain filesystem operation unchanged.
    fn begin_xxmi_persist_tx(&mut self, game: &GameInstall) -> Option<xxmi_persist::PersistTx> {
        if !self.state.static_prefs.preserve_mod_settings || !game.is_xxmi() {
            return None;
        }
        let mut tx = xxmi_persist::begin(game, self.state.static_prefs.use_default_mods_path)?;
        let namespace_context = self.xxmi_namespace_context_for_game(&game.definition.id);
        tx.set_shared_explicit_prefixes(namespace_context.shared_prefixes);
        for (root_path, prefixes) in namespace_context.prefixes_by_root {
            tx.set_explicit_namespace_prefixes_for_root(root_path, prefixes);
        }
        Some(tx)
    }

    fn xxmi_namespace_context_for_game(&mut self, game_id: &str) -> XxmiNamespaceContext {
        let known_mod_ids: HashSet<String> = self
            .state
            .mods
            .iter()
            .map(|mod_entry| mod_entry.id.clone())
            .collect();
        self.xxmi_namespace_cache
            .retain(|mod_id, _| known_mod_ids.contains(mod_id));

        let namespace_inputs: Vec<(String, PathBuf, Option<String>, Option<DateTime<Utc>>)> = self
            .state
            .mods
            .iter()
            .filter(|mod_entry| mod_entry.game_id == game_id)
            .map(|mod_entry| {
                (
                    mod_entry.id.clone(),
                    mod_entry.root_path.clone(),
                    mod_entry.ini_hash.clone(),
                    mod_entry.content_mtime,
                )
            })
            .collect();

        let mut owners: Vec<(String, String)> = Vec::new();
        let mut prefixes_by_root = Vec::new();
        for (mod_id, root_path, ini_hash, content_mtime) in namespace_inputs {
            let prefixes = self.explicit_namespace_prefixes_for_mod_cached(
                &mod_id,
                &root_path,
                &ini_hash,
                content_mtime,
            );
            for prefix in &prefixes {
                owners.push((mod_id.clone(), prefix.clone()));
            }
            prefixes_by_root.push((root_path, prefixes));
        }

        let mut shared = Vec::new();
        for i in 0..owners.len() {
            for j in (i + 1)..owners.len() {
                if owners[i].0 == owners[j].0 {
                    continue;
                }
                if xxmi_persist::namespace_prefixes_overlap(&owners[i].1, &owners[j].1) {
                    push_unique_prefix(&mut shared, owners[i].1.clone());
                    push_unique_prefix(&mut shared, owners[j].1.clone());
                }
            }
        }
        XxmiNamespaceContext {
            shared_prefixes: shared,
            prefixes_by_root,
        }
    }

    fn explicit_namespace_prefixes_for_mod_cached(
        &mut self,
        mod_id: &str,
        root_path: &Path,
        ini_hash: &Option<String>,
        content_mtime: Option<DateTime<Utc>>,
    ) -> Vec<String> {
        if let Some(entry) = self.xxmi_namespace_cache.get(mod_id)
            && entry.root_path == root_path
            && entry.ini_hash == *ini_hash
            && entry.content_mtime == content_mtime
        {
            return entry.prefixes.clone();
        }

        let prefixes = xxmi_persist::explicit_namespace_prefixes_for_mod_root(root_path);
        self.xxmi_namespace_cache.insert(
            mod_id.to_string(),
            XxmiNamespaceCacheEntry {
                root_path: root_path.to_path_buf(),
                ini_hash: ini_hash.clone(),
                content_mtime,
                prefixes: prefixes.clone(),
            },
        );
        prefixes
    }

    /// Commit the settings transaction and send the importer reload hotkey when either
    /// `d3dx_user.ini` changed or the caller changed the live mod set/path.
    fn finish_xxmi_persist_tx(
        &mut self,
        game: &GameInstall,
        tx: Option<xxmi_persist::PersistTx>,
        reload_trigger: Option<ReloadHotkeyTrigger>,
    ) {
        let mut should_reload = reload_trigger.is_some();
        let mut importer_root = None;
        if let Some(tx) = tx {
            importer_root = Some(tx.importer_root().to_path_buf());
            match tx.commit() {
                Ok(outcome) => {
                    for warning in outcome.warnings {
                        self.report_warn(warning, None);
                    }
                    should_reload |= outcome.wrote;
                }
                Err(err) => {
                    self.report_warn(
                        format!("in-game mod settings could not be written: {err:#}"),
                        None,
                    );
                }
            }
        }
        if !should_reload {
            return;
        }
        let Some(reload_trigger) = reload_trigger else {
            return;
        };
        if !self.xxmi_reload_enabled_for_game(game, reload_trigger) {
            return;
        }
        if importer_root
            .or_else(|| {
                xxmi_persist::importer_root_for(game, self.state.static_prefs.use_default_mods_path)
            })
            .is_none()
        {
            return;
        }
        self.send_xxmi_reload_hotkey_if_supported(game);
    }

    fn send_xxmi_reload_hotkey_if_supported(&mut self, game: &GameInstall) {
        if !(game.apply_mod_changes_in_game && self.game_process_running(game)) {
            return;
        }
        let game_id = game.definition.id.clone();
        if self.xxmi_reload_inflight.contains(&game_id) {
            self.xxmi_reload_pending.insert(game_id);
            return;
        }
        self.xxmi_reload_inflight.insert(game_id.clone());
        let log_game_id = game_id.clone();
        let game = game.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        let event_tx = self.xxmi_reload_event_tx.clone();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("hestia-xxmi-reload-{game_id}"))
            .spawn(move || {
                let event = if !xxmi_persist::game_process_running_for_reload(&game) {
                    XxmiReloadEvent::Finished {
                        game_id,
                        message: "skipped: game process is not running".to_string(),
                    }
                } else {
                    match xxmi_persist::send_reload_hotkey_foreground_aware(&game, use_default) {
                        Ok(report) => XxmiReloadEvent::Finished {
                            game_id,
                            message: report.message,
                        },
                        Err(error) => XxmiReloadEvent::Failed {
                            game_id,
                            message: format!("{error:#}"),
                        },
                    }
                };
                let _ = event_tx.send(event);
            })
        {
            self.xxmi_reload_inflight.remove(&log_game_id);
            self.push_log(format!("XXMI reload ({log_game_id}): failed: {err}"));
        }
    }

    fn consume_xxmi_reload_events(&mut self) {
        while let Ok(event) = self.xxmi_reload_event_rx.try_recv() {
            let game_id = match &event {
                XxmiReloadEvent::Finished { game_id, .. }
                | XxmiReloadEvent::Failed { game_id, .. } => game_id.clone(),
            };
            match event {
                XxmiReloadEvent::Finished { game_id, message } => {
                    self.push_log(format!("XXMI reload ({game_id}): {message}"));
                }
                XxmiReloadEvent::Failed { game_id, message } => {
                    self.push_log(format!("XXMI reload ({game_id}): failed: {message}"));
                }
            }
            self.xxmi_reload_inflight.remove(&game_id);
            if self.xxmi_reload_pending.remove(&game_id)
                && let Some(game) = self
                    .state
                    .games
                    .iter()
                    .find(|game| game.definition.id == game_id)
                    .cloned()
            {
                self.send_xxmi_reload_hotkey_if_supported(&game);
            }
        }
    }

    /// Capture before a filesystem mutation. Returns the rollback point for the caller to
    /// use if the mutation fails. A capture error rolls back immediately and aborts the
    /// host operation: proceeding would perform the exact lossy move this feature prevents.
    fn xxmi_capture_step(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &ModEntry,
        mode: xxmi_persist::CaptureMode,
    ) -> Result<Option<xxmi_persist::PersistCheckpoint>> {
        let Some(tx) = tx.as_mut() else {
            return Ok(None);
        };
        let checkpoint = tx.checkpoint();
        match tx.capture(mod_entry, mode) {
            Ok(_) => Ok(Some(checkpoint)),
            Err(err) => {
                tx.rollback(checkpoint);
                Err(err.context("in-game settings could not be captured; mod left untouched"))
            }
        }
    }

    /// Discard the in-memory settings mutation after a failed filesystem step, so the
    /// entries are still present when the transaction commits.
    fn xxmi_rollback_step(
        tx: &mut Option<xxmi_persist::PersistTx>,
        checkpoint: Option<xxmi_persist::PersistCheckpoint>,
    ) {
        if let (Some(tx), Some(checkpoint)) = (tx.as_mut(), checkpoint) {
            tx.rollback(checkpoint);
        }
    }

    /// Mark the provisional settings mutation as final after its filesystem step succeeded.
    fn xxmi_keep_step(
        tx: &mut Option<xxmi_persist::PersistTx>,
        checkpoint: Option<xxmi_persist::PersistCheckpoint>,
    ) {
        if let (Some(tx), Some(checkpoint)) = (tx.as_mut(), checkpoint) {
            tx.keep(checkpoint);
        }
    }

    /// Restore after a successful mutation that made the mod live. Failure never fails the
    /// host operation — the stash on disk is the recovery path.
    fn xxmi_restore_step(tx: &mut Option<xxmi_persist::PersistTx>, mod_entry: &ModEntry) {
        if let Some(tx) = tx.as_mut() {
            let checkpoint = tx.checkpoint();
            if let Err(err) = tx.restore(mod_entry) {
                tx.rollback(checkpoint);
                tx.warn(format!(
                    "in-game settings restore failed for {}: {err:#}",
                    mod_entry.folder_name
                ));
            }
        }
    }

    /// Point the stash at a hidden mod's new prospective prefix after a rename; never
    /// writes live entries for it.
    fn xxmi_rebase_step(tx: &mut Option<xxmi_persist::PersistTx>, mod_entry: &ModEntry) {
        if let Some(tx) = tx.as_mut()
            && let Err(err) = tx.rebase(mod_entry)
        {
            tx.warn(format!(
                "in-game settings rebase failed for {}: {err:#}",
                mod_entry.folder_name
            ));
        }
    }

    fn persisted_xxmi_disable(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &mut ModEntry,
    ) -> Result<()> {
        let checkpoint = Self::xxmi_capture_step(tx, mod_entry, xxmi_persist::CaptureMode::Stash)?;
        match xxmi::disable_mod(mod_entry) {
            Ok(()) => {
                Self::xxmi_keep_step(tx, checkpoint);
                Ok(())
            }
            Err(err) => {
                Self::xxmi_rollback_step(tx, checkpoint);
                Err(err)
            }
        }
    }

    fn persisted_xxmi_enable(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &mut ModEntry,
    ) -> Result<()> {
        xxmi::enable_mod(mod_entry)?;
        Self::xxmi_restore_step(tx, mod_entry);
        Ok(())
    }

    fn persisted_xxmi_archive(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &mut ModEntry,
        game: &GameInstall,
        use_default_path: bool,
    ) -> Result<()> {
        let checkpoint = Self::xxmi_capture_step(tx, mod_entry, xxmi_persist::CaptureMode::Stash)?;
        match xxmi::archive_mod(mod_entry, game, use_default_path) {
            Ok(_) => {
                Self::xxmi_keep_step(tx, checkpoint);
                Ok(())
            }
            Err(err) => {
                Self::xxmi_rollback_step(tx, checkpoint);
                Err(err)
            }
        }
    }

    fn persisted_xxmi_restore_from_archive(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &mut ModEntry,
        game: &GameInstall,
        use_default_path: bool,
    ) -> Result<()> {
        xxmi::restore_mod(mod_entry, game, use_default_path)?;
        Self::xxmi_restore_step(tx, mod_entry);
        Ok(())
    }

    fn persisted_xxmi_recycle(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &ModEntry,
    ) -> Result<()> {
        let checkpoint = Self::xxmi_capture_step(tx, mod_entry, xxmi_persist::CaptureMode::Stash)?;
        match xxmi::send_to_recycle_bin(mod_entry) {
            Ok(()) => {
                Self::xxmi_keep_step(tx, checkpoint);
                Ok(())
            }
            Err(err) => {
                Self::xxmi_rollback_step(tx, checkpoint);
                Err(err)
            }
        }
    }

    fn persisted_xxmi_purge(
        tx: &mut Option<xxmi_persist::PersistTx>,
        mod_entry: &ModEntry,
    ) -> Result<()> {
        let checkpoint = Self::xxmi_capture_step(tx, mod_entry, xxmi_persist::CaptureMode::Purge)?;
        let removal = (|| -> Result<()> {
            if mod_entry.root_path.is_dir() {
                fs::remove_dir_all(&mod_entry.root_path)?;
            } else if mod_entry.root_path.is_file() {
                fs::remove_file(&mod_entry.root_path)?;
            }
            Ok(())
        })();
        match removal {
            Ok(()) => {
                Self::xxmi_keep_step(tx, checkpoint);
                Ok(())
            }
            Err(err) => {
                Self::xxmi_rollback_step(tx, checkpoint);
                Err(err)
            }
        }
    }

    /// One transaction per game for a batch, keyed by game id. Games that do not qualify
    /// still get a `None` slot so the loop can borrow uniformly.
    fn begin_xxmi_persist_tx_batch(
        &mut self,
        games: &[GameInstall],
        member_game_ids: impl Iterator<Item = String>,
    ) -> HashMap<String, Option<xxmi_persist::PersistTx>> {
        let mut per_game = HashMap::new();
        for game_id in member_game_ids {
            if per_game.contains_key(&game_id) {
                continue;
            }
            let tx = games
                .iter()
                .find(|game| game.definition.id == game_id)
                .and_then(|game| self.begin_xxmi_persist_tx(game));
            per_game.insert(game_id, tx);
        }
        per_game
    }

    fn finish_xxmi_persist_tx_batch(
        &mut self,
        games: &[GameInstall],
        per_game: HashMap<String, Option<xxmi_persist::PersistTx>>,
        reload_requests: HashMap<String, ReloadHotkeyTrigger>,
    ) {
        for (game_id, tx) in per_game {
            if let Some(game) = games.iter().find(|game| game.definition.id == game_id) {
                let game = game.clone();
                let request_reload = reload_requests.get(&game.definition.id).copied();
                self.finish_xxmi_persist_tx(&game, tx, request_reload);
            }
        }
    }

    /// Restore for freshly installed mods that arrived carrying a `mod.cfg` — cross-machine
    /// folder copies, recycle-bin undeletes, replaced updates. Only applies where no live
    /// entries already exist under the mod's prefix: live entries are newer than anything
    /// an imported stash can hold.
    fn run_xxmi_persist_import_restore(&mut self, mod_ids: &[String]) {
        if !self.state.static_prefs.preserve_mod_settings {
            return;
        }
        let games = self.state.games.clone();
        let member_game_ids: Vec<String> = self
            .state
            .mods
            .iter()
            .filter(|m| mod_ids.contains(&m.id))
            .map(|m| m.game_id.clone())
            .collect();
        let mut per_game_tx = self.begin_xxmi_persist_tx_batch(&games, member_game_ids.into_iter());
        for mod_entry in self
            .state
            .mods
            .iter()
            .filter(|m| mod_ids.contains(&m.id))
        {
            let Some(tx) = per_game_tx
                .get_mut(&mod_entry.game_id)
                .and_then(|slot| slot.as_mut())
            else {
                continue;
            };
            let checkpoint = tx.checkpoint();
            if let Err(err) = tx.restore_imported(mod_entry) {
                tx.rollback(checkpoint);
                tx.warn(format!(
                    "imported settings restore failed for {}: {err:#}",
                    mod_entry.folder_name
                ));
            }
        }
        self.finish_xxmi_persist_tx_batch(&games, per_game_tx, HashMap::new());
    }

    fn request_xxmi_reload_for_live_mod_ids(
        &mut self,
        mod_ids: &[String],
        trigger: ReloadHotkeyTrigger,
    ) {
        let games = self.state.games.clone();
        let mut per_game = HashMap::new();
        for game_id in self
            .state
            .mods
            .iter()
            .filter(|m| mod_ids.contains(&m.id) && m.status == ModStatus::Active)
            .filter_map(|mod_entry| {
                games
                    .iter()
                    .find(|game| {
                        game.definition.id == mod_entry.game_id
                            && self.xxmi_reload_enabled_for_game(game, trigger)
                    })
                    .map(|game| game.definition.id.clone())
            })
        {
            per_game.entry(game_id).or_insert(trigger);
        }
        for (game_id, trigger) in per_game {
            if let Some(game) = games.iter().find(|game| game.definition.id == game_id) {
                let game = game.clone();
                self.finish_xxmi_persist_tx(&game, None, Some(trigger));
            }
        }
    }

    /// Scan-time settings pass for one game: write baseline stashes for live mods that have
    /// entries but no `mod.cfg` yet (the upgrade path that makes external-rename repair
    /// possible), and reroute any mod whose stash prefix disagrees with its real path. The
    /// baseline never touches `d3dx_user.ini`; reroute writes only on an actual mismatch.
    fn run_xxmi_persist_scan_pass(&mut self, game_id: &str) {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return;
        };
        if !game.is_xxmi() {
            return;
        }
        let use_default = self.state.static_prefs.use_default_mods_path;
        // The d3dx.ini foreground-window grant must be in place before launch for
        // reload delivery to work while Hestia is foreground. Refresh it on scan, and
        // remove Hestia's generated config when the reload option is disabled.
        match xxmi_persist::ensure_reload_config(
            &game,
            use_default,
            game.apply_mod_changes_in_game,
        ) {
            Ok(notices) => {
                for notice in notices {
                    self.report_warn(notice, None);
                }
            }
            Err(err) => {
                self.report_warn(format!("XXMI reload config refresh failed: {err:#}"), None);
            }
        }
        // Regenerate the live-state helper from the freshly-scanned active mod set, so a
        // changed mod roster (enable/disable/update) is reflected in what gets mirrored.
        self.refresh_live_state_helper_for_game(&game);
        if !self.state.static_prefs.preserve_mod_settings {
            return;
        }
        let Some(mut tx) = self.begin_xxmi_persist_tx(&game) else {
            return;
        };
        for mod_entry in self.state.mods.iter().filter(|m| m.game_id == game_id) {
            let checkpoint = tx.checkpoint();
            let result = if xxmi_persist::read_stash(&mod_entry.root_path).is_some() {
                tx.reroute(mod_entry)
            } else {
                tx.baseline(mod_entry)
            };
            if let Err(err) = result {
                tx.rollback(checkpoint);
                tx.warn(format!(
                    "settings scan pass failed for {}: {err:#}",
                    mod_entry.folder_name
                ));
            }
        }
        self.finish_xxmi_persist_tx(&game, Some(tx), None);
    }

    fn disable_mod_by_id(&mut self, mod_id: &str) {
        if self
            .mod_action_lock_reason_by_id(mod_id, ModMutationKind::DisableActive)
            .is_some()
        {
            self.report_locked_mods(Some(self.text().disable_failed()));
            return;
        }
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default = self.state.static_prefs.use_default_mods_path;
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                let result = match game.as_ref().map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => Self::persisted_xxmi_disable(&mut ptx, mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.as_ref().expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                (Some(result), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(ReloadHotkeyTrigger::DisablingMods);
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_disabled(), text.disable_failed());
        }
    }

    fn enable_or_restore_mod_by_id(&mut self, mod_id: &str) {
        if self
            .mod_action_lock_reason_by_id(mod_id, ModMutationKind::EnableIntoActive)
            .is_some()
        {
            self.report_locked_mods(Some(self.text().enable_failed()));
            return;
        }
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let text = self.text();
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name, action, trigger) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => {
                    let result = match game.as_ref().map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => Self::persisted_xxmi_enable(&mut ptx, mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.as_ref().expect("game checked"), use_default_path)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    (
                        Some(result),
                        Some(name),
                        Some(text.action_enabled()),
                        Some(ReloadHotkeyTrigger::EnablingMods),
                    )
                }
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        if game.is_xxmi() {
                            Self::persisted_xxmi_restore_from_archive(&mut ptx, mod_entry, game, use_default_path)?;
                        } else {
                            bail!("archive is not supported for Unreal Engine games");
                        }
                        Ok(())
                    })();
                    (
                        Some(result),
                        Some(name),
                        Some(text.action_unarchived()),
                        Some(ReloadHotkeyTrigger::RestoringMods),
                    )
                }
                _ => (None, None, None, None),
            }
        } else {
            (None, None, None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(trigger)
            .flatten();
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name), Some(action)) = (result, name, action) {
            match result {
                Ok(()) => {
                    self.log_action(action, &name);
                    self.set_message_ok(text.action_message(action, &name));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == text.action_enabled() {
                        text.enable_failed()
                    } else {
                        text.restore_failed()
                    };
                    self.report_error(err, Some(toast));
                }
            }
        }
    }

    fn archive_mod_by_id(&mut self, mod_id: &str) {
        if let Some(snapshot) = self.state.mods.iter().find(|m| m.id == mod_id).cloned() {
            self.clear_mod_image_runtime_state(&snapshot);
        }
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name) = if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                if game.is_xxmi() {
                    Self::persisted_xxmi_archive(&mut ptx, mod_entry, game, use_default_path)?;
                } else {
                    bail!("archive is not supported for Unreal Engine games");
                }
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(ReloadHotkeyTrigger::ArchivingMods);
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_archived(), text.archive_failed());
        }
    }

    fn delete_mod_by_id(&mut self, mod_id: &str) {
        if self
            .mod_action_lock_reason_by_id(mod_id, ModMutationKind::Delete)
            .is_some()
        {
            self.report_locked_mods(Some(self.text().delete_failed()));
            return;
        }
        let result = (|| -> Result<()> {
            let mod_entry = self
                .state
                .mods
                .iter()
                .find(|m| m.id == mod_id)
                .cloned()
                .ok_or_else(|| anyhow!("no mod selected"))?;
            let behavior = self.delete_mod_entry(&mod_entry)?;
            let text = self.text();
            let action = text.delete_action(behavior);
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(text.action_message(action, &mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some(self.text().delete_failed()));
        }
    }

    fn delete_mod_entry(&mut self, mod_entry: &ModEntry) -> Result<DeleteBehavior> {
        let game = self.game_for_mod(mod_entry);
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let behavior = self.delete_mod_entry_with_tx(mod_entry, &mut ptx);
        if let Some(game) = game.as_ref() {
            let request_reload =
                (game.is_xxmi() && behavior.is_ok()).then_some(ReloadHotkeyTrigger::DeletingMods);
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }
        behavior
    }

    /// Both delete behaviors funnel through here — hooking only the recycle path would
    /// leave permanent deletes orphaning `d3dx_user.ini` entries. Recycle captures to a
    /// stash that rides into the bin (so an undelete round-trips); permanent delete purges
    /// the entries without writing a stash the removal is about to destroy.
    fn delete_mod_entry_with_tx(
        &mut self,
        mod_entry: &ModEntry,
        ptx: &mut Option<xxmi_persist::PersistTx>,
    ) -> Result<DeleteBehavior> {
        self.clear_mod_image_runtime_state(mod_entry);
        match self.state.static_prefs.delete_behavior {
            DeleteBehavior::RecycleBin => {
                Self::persisted_xxmi_recycle(ptx, mod_entry)?;
                Ok(DeleteBehavior::RecycleBin)
            }
            DeleteBehavior::Permanent => {
                Self::persisted_xxmi_purge(ptx, mod_entry)?;
                Ok(DeleteBehavior::Permanent)
            }
        }
    }

    fn delete_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_delete_selected();
            return;
        }
        if self
            .selected_mod()
            .and_then(|mod_entry| self.mod_action_lock_reason(mod_entry, ModMutationKind::Delete))
            .is_some()
        {
            self.report_locked_mods(Some(self.text().delete_failed()));
            return;
        }

        let result = (|| -> Result<()> {
            let mod_entry = self.selected_mod().cloned().ok_or_else(|| anyhow!("no mod selected"))?;
            let behavior = self.delete_mod_entry(&mod_entry)?;
            let text = self.text();
            let action = text.delete_action(behavior);
            self.log_action(action, &mod_entry.folder_name);
            self.set_message_ok(text.action_message(action, &mod_entry.folder_name));
            self.save_state();
            self.refresh();
            Ok(())
        })();
        if let Err(err) = result {
            self.report_error(err, Some(self.text().delete_failed()));
        }
    }

    fn disable_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_disable_selected();
            return;
        }
        if self
            .selected_mod()
            .and_then(|mod_entry| self.mod_action_lock_reason(mod_entry, ModMutationKind::DisableActive))
            .is_some()
        {
            self.report_locked_mods(Some(self.text().disable_failed()));
            return;
        }

        let text = self.text();
        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default = self.state.static_prefs.use_default_mods_path;
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            if mod_entry.status == ModStatus::Active {
                let result = match game.as_ref().map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => Self::persisted_xxmi_disable(&mut ptx, mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.as_ref().expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                (Some(result), Some(name))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(ReloadHotkeyTrigger::DisablingMods);
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name)) = (result, name) {
            self.finish_single_mod_action(result, &name, text.action_disabled(), text.disable_failed());
        }
    }

    fn enable_or_restore_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_enable_selected();
            return;
        }
        if self
            .selected_mod()
            .and_then(|mod_entry| self.mod_action_lock_reason(mod_entry, ModMutationKind::EnableIntoActive))
            .is_some()
        {
            self.report_locked_mods(Some(self.text().enable_failed()));
            return;
        }

        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let text = self.text();
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name, action, trigger) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            match mod_entry.status {
                ModStatus::Disabled => {
                    let result = match game.as_ref().map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => Self::persisted_xxmi_enable(&mut ptx, mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.as_ref().expect("game checked"), use_default_path)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    (
                        Some(result),
                        Some(name),
                        Some(text.action_enabled()),
                        Some(ReloadHotkeyTrigger::EnablingMods),
                    )
                }
                ModStatus::Archived => {
                    let result = (|| -> Result<()> {
                        let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                        if game.is_xxmi() {
                            Self::persisted_xxmi_restore_from_archive(&mut ptx, mod_entry, game, use_default_path)?;
                        } else {
                            bail!("archive is not supported for Unreal Engine games");
                        }
                        Ok(())
                    })();
                    (
                        Some(result),
                        Some(name),
                        Some(text.action_unarchived()),
                        Some(ReloadHotkeyTrigger::RestoringMods),
                    )
                }
                _ => (None, None, None, None),
            }
        } else {
            (None, None, None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(trigger)
            .flatten();
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name), Some(action)) = (result, name, action) {
            match result {
                Ok(()) => {
                    self.log_action(action, &name);
                    self.set_message_ok(text.action_message(action, &name));
                    self.save_state();
                    self.refresh();
                }
                Err(err) => {
                    let toast = if action == text.action_enabled() {
                        text.enable_failed()
                    } else {
                        text.restore_failed()
                    };
                    self.report_error(err, Some(toast));
                }
            }
        }
    }

    fn archive_selected_context(&mut self) {
        if !self.selected_mods.is_empty() {
            self.batch_archive_selected();
            return;
        }

        if let Some(snapshot) = self.selected_mod().cloned() {
            self.clear_mod_image_runtime_state(&snapshot);
        }
        let game = self.selected_mod().and_then(|m| self.game_for_mod(m));
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let (result, name) = if let Some(mod_entry) = self.selected_mod_mut() {
            let name = mod_entry.folder_name.clone();
            let result = (|| -> Result<()> {
                let game = game.as_ref().ok_or_else(|| anyhow!("game not selected"))?;
                if game.is_xxmi() {
                    Self::persisted_xxmi_archive(&mut ptx, mod_entry, game, use_default_path)?;
                } else {
                    bail!("archive is not supported for Unreal Engine games");
                }
                Ok(())
            })();
            (Some(result), Some(name))
        } else {
            (None, None)
        };
        if let Some(game) = game.as_ref() {
            let request_reload = (game.is_xxmi()
                && result.as_ref().is_some_and(|result| result.is_ok()))
            .then_some(ReloadHotkeyTrigger::ArchivingMods);
            let game = game.clone();
            self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        }

        if let (Some(result), Some(name)) = (result, name) {
            let text = self.text();
            self.finish_single_mod_action(result, &name, text.action_archived(), text.archive_failed());
        }
    }

    fn batch_update_selected(&mut self) {
        // Single iteration: collect IDs in one pass
        let update_ids: Vec<String> = self.state.mods.iter()
            .filter(|m| {
                self.selected_mods.contains(&m.id)
                    && (matches!(m.update_state, ModUpdateState::UpdateAvailable)
                        || (self.state.static_prefs.modified_update_behavior != ModifiedUpdateBehavior::HideButton
                            && Self::has_modified_update_available(m)))
            })
            .map(|m| m.id.clone())
            .collect();

        let mut count = 0;
        let mut skipped_locked = 0;
        for id in &update_ids {
            if self
                .mod_action_lock_reason_by_id(id, ModMutationKind::UpdateExisting)
                .is_some()
            {
                skipped_locked += 1;
            } else if self.queue_update_apply(id) {
                count += 1;
            }
        }

        if count > 0 {
            self.set_message_ok(self.text().queued_updates(count));
            self.selected_mods.clear();
        }
        if skipped_locked > 0 {
            self.report_skipped_locked_mods();
        }
    }

    fn batch_disable_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        let locked_unreal_game_ids: HashSet<String> = games
            .iter()
            .filter(|game| game.is_unreal_engine() && self.game_process_running(game))
            .map(|game| game.definition.id.clone())
            .collect();
        let mut disabled_count = 0;
        let mut skipped_locked = 0;
        let member_game_ids: Vec<String> = self
            .state
            .mods
            .iter()
            .filter(|m| self.selected_mods.contains(&m.id) && m.status == ModStatus::Active)
            .map(|m| m.game_id.clone())
            .collect();
        let mut per_game_tx = self.begin_xxmi_persist_tx_batch(&games, member_game_ids.into_iter());
        let mut reload_requests = HashMap::new();
        let disable_trigger_enabled = self
            .state
            .static_prefs
            .reload_hotkey_triggers
            .enabled(ReloadHotkeyTrigger::DisablingMods);
        // Single iteration: filter selected mods and disable in one pass
        for mod_entry in self.state.mods.iter_mut() {
            if self.selected_mods.contains(&mod_entry.id) && mod_entry.status == ModStatus::Active {
                let game = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id);
                if locked_unreal_game_ids.contains(&mod_entry.game_id) {
                    skipped_locked += 1;
                    continue;
                }
                let mut fallback_tx = None;
                let ptx = per_game_tx
                    .get_mut(&mod_entry.game_id)
                    .unwrap_or(&mut fallback_tx);
                let result = match game.map(|game| game.definition.backend) {
                    Some(GameBackend::Xxmi) => Self::persisted_xxmi_disable(ptx, mod_entry),
                    Some(GameBackend::UnrealEngine) => {
                        unrealengine::disable_mod(mod_entry, game.expect("game checked"), use_default)
                    }
                    None => Err(anyhow!("game not found")),
                };
                if result.is_ok() {
                    disabled_count += 1;
                    if disable_trigger_enabled
                        && game.is_some_and(|game| {
                            game.is_xxmi() && game.apply_mod_changes_in_game
                        })
                    {
                        reload_requests.insert(
                            mod_entry.game_id.clone(),
                            ReloadHotkeyTrigger::DisablingMods,
                        );
                    }
                }
            }
        }
        self.finish_xxmi_persist_tx_batch(&games, per_game_tx, reload_requests);
        let action = self.text().action_disabled();
        self.finish_batch_mod_action(disabled_count, action);
        if skipped_locked > 0 {
            self.report_skipped_locked_mods();
        }
    }

    fn batch_enable_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        let locked_unreal_game_ids: HashSet<String> = games
            .iter()
            .filter(|game| game.is_unreal_engine() && self.game_process_running(game))
            .map(|game| game.definition.id.clone())
            .collect();
        let mut enabled_count = 0;
        let mut unarchived_count = 0;
        let mut skipped_locked = 0;
        let member_game_ids: Vec<String> = self
            .state
            .mods
            .iter()
            .filter(|m| {
                self.selected_mods.contains(&m.id)
                    && matches!(m.status, ModStatus::Disabled | ModStatus::Archived)
            })
            .map(|m| m.game_id.clone())
            .collect();
        let mut per_game_tx = self.begin_xxmi_persist_tx_batch(&games, member_game_ids.into_iter());
        let mut reload_requests = HashMap::new();
        let enable_trigger_enabled = self
            .state
            .static_prefs
            .reload_hotkey_triggers
            .enabled(ReloadHotkeyTrigger::EnablingMods);
        let restore_trigger_enabled = self
            .state
            .static_prefs
            .reload_hotkey_triggers
            .enabled(ReloadHotkeyTrigger::RestoringMods);
        // Single iteration: process all selected mods in one pass
        for mod_entry in self.state.mods.iter_mut() {
            if self.selected_mods.contains(&mod_entry.id) {
                let game = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id);
                let mut fallback_tx = None;
                let ptx = per_game_tx
                    .get_mut(&mod_entry.game_id)
                    .unwrap_or(&mut fallback_tx);
                if mod_entry.status == ModStatus::Disabled {
                    if locked_unreal_game_ids.contains(&mod_entry.game_id) {
                        skipped_locked += 1;
                        continue;
                    }
                    let result = match game.map(|game| game.definition.backend) {
                        Some(GameBackend::Xxmi) => Self::persisted_xxmi_enable(ptx, mod_entry),
                        Some(GameBackend::UnrealEngine) => {
                            unrealengine::enable_mod(mod_entry, game.expect("game checked"), use_default)
                        }
                        None => Err(anyhow!("game not found")),
                    };
                    if result.is_ok() {
                        enabled_count += 1;
                        if enable_trigger_enabled
                            && game.is_some_and(|game| {
                                game.is_xxmi() && game.apply_mod_changes_in_game
                            })
                        {
                            reload_requests.insert(
                                mod_entry.game_id.clone(),
                                ReloadHotkeyTrigger::EnablingMods,
                            );
                        }
                    }
                } else if mod_entry.status == ModStatus::Archived {
                    if let Some(game_ref) = game {
                        if game_ref.is_xxmi()
                            && Self::persisted_xxmi_restore_from_archive(
                                ptx,
                                mod_entry,
                                game_ref,
                                use_default,
                            )
                            .is_ok()
                        {
                            unarchived_count += 1;
                            if restore_trigger_enabled && game_ref.apply_mod_changes_in_game {
                                reload_requests.insert(
                                    mod_entry.game_id.clone(),
                                    ReloadHotkeyTrigger::RestoringMods,
                                );
                            }
                        }
                    }
                }
            }
        }
        self.finish_xxmi_persist_tx_batch(&games, per_game_tx, reload_requests);
        if enabled_count > 0 {
            let text = self.text();
            let action = text.action_enabled();
            self.log_action(action, &text.library_mods_count(enabled_count));
            self.set_message_ok(text.action_count_message(action, enabled_count));
        }
        if unarchived_count > 0 {
            let text = self.text();
            let action = text.action_unarchived();
            self.log_action(action, &text.library_mods_count(unarchived_count));
            self.set_message_ok(text.action_count_message(action, unarchived_count));
        }
        if enabled_count + unarchived_count > 0 {
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
        if skipped_locked > 0 {
            self.report_skipped_locked_mods();
        }
    }

    fn rename_mod_folder(&mut self, mod_id: &str, new_name: &str) -> Result<()> {
        let game = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| self.game_for_mod(m));
        if self
            .mod_action_lock_reason_by_id(mod_id, ModMutationKind::Rename)
            .is_some()
        {
            bail!("{}", self.text().mods_locked_probably_by_game());
        }
        let mut ptx = game.as_ref().and_then(|game| self.begin_xxmi_persist_tx(game));
        let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) else {
            bail!("mod not found");
        };
        if mod_entry.folder_name == new_name { return Ok(()); }
        let old_path = mod_entry.root_path.clone();
        let new_path = old_path.parent().ok_or_else(|| anyhow!("invalid path"))?.join(new_name);
        if new_path.exists() {
            bail!("destination folder already exists: {}", new_name);
        }
        let request_reload = (game.as_ref().is_some_and(|game| game.is_xxmi())
            && mod_entry.status == ModStatus::Active)
        .then_some(ReloadHotkeyTrigger::RenamingMods);
        // Capture while the mod is still at its old path; the stash lands inside the folder
        // and travels with the rename.
        let checkpoint = Self::xxmi_capture_step(&mut ptx, mod_entry, xxmi_persist::CaptureMode::Stash)?;
        if let Err(err) = fs::rename(&old_path, &new_path) {
            Self::xxmi_rollback_step(&mut ptx, checkpoint);
            return Err(err.into());
        }
        Self::xxmi_keep_step(&mut ptx, checkpoint);
        mod_entry.root_path = new_path;
        mod_entry.folder_name = new_name.to_string();
        mod_entry.updated_at = Utc::now();
        // A live mod gets its settings back under the new prefix; a hidden mod must not
        // have live entries written for it — only its stash anchor moves.
        if mod_entry.status == ModStatus::Active {
            Self::xxmi_restore_step(&mut ptx, mod_entry);
        } else {
            Self::xxmi_rebase_step(&mut ptx, mod_entry);
        }
        let game = game.ok_or_else(|| anyhow!("game not found"))?;
        // Commit settings before the metadata write: a metadata failure must not lose the
        // already-performed settings migration.
        self.finish_xxmi_persist_tx(&game, ptx, request_reload);
        let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) else {
            bail!("mod not found");
        };
        match game.definition.backend {
            GameBackend::Xxmi => xxmi::save_mod_metadata(mod_entry)?,
            GameBackend::UnrealEngine => unrealengine::write_portable_metadata(mod_entry)?,
        }
        Ok(())
    }

    fn perform_mod_rename(&mut self, mod_id: String) {
        let raw = self.mod_detail_edit_name.trim().to_string();
        if raw.is_empty() {
            self.clear_mod_detail_rename();
            return;
        }
        let sanitized = sanitize_folder_name(&raw);
        if sanitized == self.text().imported_mod() || sanitized.chars().all(|c| c == '_') {
            self.clear_mod_detail_rename();
            return;
        }
        if let Err(err) = self.rename_mod_folder(&mod_id, &sanitized) {
            self.report_error(err, Some(self.text().rename_failed()));
        } else {
            let text = self.text();
            self.set_message_ok(text.renamed_to(&sanitized));
            self.log_action(text.action_renamed(), &sanitized);
        }
        self.clear_mod_detail_rename();
        self.refresh();
    }

    fn batch_archive_selected(&mut self) {
        let games = self.state.games.clone();
        let use_default = self.state.static_prefs.use_default_mods_path;
        // Collect mod entries to clear image state (need owned data to avoid borrow conflicts)
        let mods_to_clear: Vec<ModEntry> = self.state.mods
            .iter()
            .filter(|m| self.selected_mods.contains(&m.id) 
                && matches!(m.status, ModStatus::Active | ModStatus::Disabled)
                && games
                    .iter()
                    .find(|game| game.definition.id == m.game_id)
                    .is_some_and(|game| game.is_xxmi()))
            .cloned()
            .collect();
        
        // Clear image states
        for mod_entry in &mods_to_clear {
            self.clear_mod_image_runtime_state(mod_entry);
        }
        
        // Archive mods in a single iteration
        let mut archived_count = 0;
        let mut per_game_tx = self.begin_xxmi_persist_tx_batch(
            &games,
            mods_to_clear.iter().map(|m| m.game_id.clone()),
        );
        let mut reload_requests = HashMap::new();
        let archive_trigger_enabled = self
            .state
            .static_prefs
            .reload_hotkey_triggers
            .enabled(ReloadHotkeyTrigger::ArchivingMods);
        for mod_entry in self.state.mods.iter_mut() {
            if mods_to_clear.iter().any(|m| m.id == mod_entry.id) {
                if let Some(game_ref) = games
                    .iter()
                    .find(|game| game.definition.id == mod_entry.game_id)
                {
                    let mut fallback_tx = None;
                    let ptx = per_game_tx
                        .get_mut(&mod_entry.game_id)
                        .unwrap_or(&mut fallback_tx);
                    if game_ref.is_xxmi()
                        && Self::persisted_xxmi_archive(ptx, mod_entry, game_ref, use_default)
                            .is_ok()
                    {
                        archived_count += 1;
                        if archive_trigger_enabled && game_ref.apply_mod_changes_in_game {
                            reload_requests.insert(
                                mod_entry.game_id.clone(),
                                ReloadHotkeyTrigger::ArchivingMods,
                            );
                        }
                    }
                }
            }
        }
        self.finish_xxmi_persist_tx_batch(&games, per_game_tx, reload_requests);
        let action = self.text().action_archived();
        self.finish_batch_mod_action(archived_count, action);
    }

    fn batch_delete_selected(&mut self) {
        // Single iteration: collect selected mods to delete in one pass
        let mods_to_delete: Vec<ModEntry> = self.state.mods
            .iter()
            .filter(|m| self.selected_mods.contains(&m.id))
            .cloned()
            .collect();
        let mut deleted_count = 0;
        let mut skipped_locked = 0;
        let mut last_err: Option<anyhow::Error> = None;
        let games = self.state.games.clone();
        let mut per_game_tx = self.begin_xxmi_persist_tx_batch(
            &games,
            mods_to_delete.iter().map(|m| m.game_id.clone()),
        );
        let mut reload_requests = HashMap::new();
        for mod_entry in mods_to_delete {
            if self
                .mod_action_lock_reason(&mod_entry, ModMutationKind::Delete)
                .is_some()
            {
                skipped_locked += 1;
                continue;
            }
            let mut fallback_tx = None;
            let ptx = per_game_tx
                .get_mut(&mod_entry.game_id)
                .unwrap_or(&mut fallback_tx);
            match self.delete_mod_entry_with_tx(&mod_entry, ptx) {
                Ok(_) => {
                    deleted_count += 1;
                    if games
                        .iter()
                        .find(|game| game.definition.id == mod_entry.game_id)
                        .is_some_and(|game| {
                            self.xxmi_reload_enabled_for_game(game, ReloadHotkeyTrigger::DeletingMods)
                        })
                    {
                        reload_requests.insert(
                            mod_entry.game_id.clone(),
                            ReloadHotkeyTrigger::DeletingMods,
                        );
                    }
                }
                Err(err) => last_err = Some(err),
            }
        }
        self.finish_xxmi_persist_tx_batch(&games, per_game_tx, reload_requests);
        if deleted_count > 0 {
            let text = self.text();
            let action = text.delete_action(self.state.static_prefs.delete_behavior);
            self.log_action(action, &text.library_mods_count(deleted_count));
            self.set_message_ok(text.action_count_message(action, deleted_count));
            self.save_state();
            self.refresh();
            self.selected_mods.clear();
        }
        if let Some(err) = last_err {
            let toast = self.mod_action_error_toast(&err, self.text().delete_failed());
            self.report_error(err, Some(toast));
        }
        if skipped_locked > 0 {
            self.report_skipped_locked_mods();
        }
    }

    fn toggle_mod_selection(&mut self, mod_id: &str, checked: bool) {
        if checked {
            self.selected_mods.insert(mod_id.to_string());
        } else {
            self.selected_mods.remove(mod_id);
        }
    }
}
