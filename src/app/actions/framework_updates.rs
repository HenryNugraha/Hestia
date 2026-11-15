const FRAMEWORK_WATCH_ONLY_CHECK_COOLDOWN: Duration = Duration::from_secs(1800);

#[derive(Clone)]
struct FrameworkWatch {
    mod_id: String,
    game_id: String,
    page_id: u64,
    is_tool: bool,
    signature: frameworks::FrameworkSignature,
    copy_ids: Vec<u64>,
    other_ids: Vec<u64>,
}

struct FrameworkPageReport {
    mod_id: String,
    game_id: String,
    page_id: u64,
    is_tool: bool,
    new_copies: Vec<gamebanana::ModFile>,
    other_ids: Vec<u64>,
    known_copy_files: Vec<gamebanana::ModFile>,
    page_gone: bool,
}

#[derive(Clone, Default, PartialEq)]
enum FrameworkCheckScope {
    #[default]
    CheckedPagesOnly,
    AllPagesOfGame(String),
    AllPages,
    Pages(HashSet<(String, u64, bool)>),
}

impl FrameworkCheckScope {
    fn includes(&self, game_id: &str, page_id: u64, is_tool: bool) -> bool {
        match self {
            Self::CheckedPagesOnly => false,
            Self::AllPagesOfGame(scope_game_id) => scope_game_id == game_id,
            Self::AllPages => true,
            Self::Pages(pages) => pages.contains(&(game_id.to_string(), page_id, is_tool)),
        }
    }
}

enum FrameworkLadderStep {
    SetState {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        file_id: u64,
        state: FrameworkCopyState,
    },
    Download {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        file_id: u64,
    },
    QuietFetch {
        mod_id: String,
        game_id: String,
        page_id: u64,
        is_tool: bool,
        file: Box<gamebanana::ModFile>,
        installed: frameworks::InstalledFolder,
    },
    AskNewer {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        installed_file_id: u64,
        installed_page_id: u64,
    },
}

enum FrameworkThaliaVerdict {
    Newer(u64),
    NotNewer,
    CannotTell,
}

fn framework_copy_state_from_file_names(
    installed_file_name: Option<&str>,
    arriving_file_name: &str,
) -> FrameworkCopyState {
    let Some(installed_file_name) = installed_file_name else {
        return FrameworkCopyState::Undecided;
    };
    match frameworks::arriving_file_name_is_newer(installed_file_name, arriving_file_name) {
        Some(true) => FrameworkCopyState::Newer,
        Some(false) => FrameworkCopyState::NotNewer,
        None => FrameworkCopyState::AskThalia,
    }
}

fn framework_file_can_download(file: &gamebanana::ModFile) -> bool {
    file.download_url
        .as_deref()
        .is_some_and(|url| !url.is_empty())
}

fn framework_covered_file_ids(
    mods: &[ModEntry],
    game_id: &str,
    page_id: u64,
    is_tool: bool,
) -> HashSet<u64> {
    mods.iter()
        .filter(|entry| entry.game_id == game_id)
        .filter_map(mod_framework_record)
        .filter_map(|record| record.page(page_id, is_tool))
        .flat_map(|page| page.copies.iter().map(|copy| copy.file_id))
        .collect()
}

fn profile_without_framework_copies<'a>(
    profile: &gamebanana::ProfileResponse,
    covered: &HashSet<u64>,
    file_sets: impl IntoIterator<Item = &'a FileSetRecipe>,
) -> Option<gamebanana::ProfileResponse> {
    if covered.is_empty() {
        return None;
    }
    let tracked: HashSet<u64> = file_sets.into_iter().flat_map(installed_file_ids).collect();
    let hidden: HashSet<u64> = covered.difference(&tracked).copied().collect();
    let hides_a_file = profile
        .files
        .iter()
        .chain(&profile.archived_files)
        .any(|file| hidden.contains(&file.id));
    if !hides_a_file {
        return None;
    }
    let mut filtered = profile.clone();
    filtered.files.retain(|file| !hidden.contains(&file.id));
    filtered
        .archived_files
        .retain(|file| !hidden.contains(&file.id));
    filtered
        .file_listings
        .retain(|file_id, _| !hidden.contains(file_id));
    Some(filtered)
}

fn framework_has_undecided_copy(mod_entry: &ModEntry) -> bool {
    mod_framework_record(mod_entry)
        .filter(|record| record.follows_outfit_pages())
        .is_some_and(|record| {
            record
                .copies_in(FrameworkCopyState::Undecided)
                .any(|(page, _)| !page.gone)
        })
}

fn framework_page_for_actions(mod_entry: &ModEntry) -> Option<(u64, bool)> {
    let record = mod_framework_record(mod_entry).filter(|record| record.follows_outfit_pages())?;
    let origin = record.came_from?;
    record
        .page(origin.page_id, origin.is_tool)
        .is_none_or(|page| !page.gone)
        .then_some((origin.page_id, origin.is_tool))
}

fn mod_page_for_actions(mod_entry: &ModEntry) -> Option<(u64, bool)> {
    mod_entry
        .source
        .as_ref()
        .and_then(|source| source.gamebanana.as_ref())
        .filter(|link| link.mod_id > 0)
        .map(|link| (link.mod_id, gamebanana::is_tool_url(&link.url)))
        .or_else(|| framework_page_for_actions(mod_entry))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameworkCardNotice {
    Held,
    UndecidedCopy,
}

fn framework_card_notice(mod_entry: &ModEntry) -> Option<FrameworkCardNotice> {
    if !mod_is_managed_framework(mod_entry) {
        return None;
    }
    let held = mod_entry
        .source
        .as_ref()
        .is_some_and(|source| source.ignore_update_always);
    if held {
        Some(FrameworkCardNotice::Held)
    } else if framework_has_undecided_copy(mod_entry) {
        Some(FrameworkCardNotice::UndecidedCopy)
    } else {
        None
    }
}

impl HestiaApp {
    fn framework_watches_for_check(
        &self,
        items: &[UpdateCheckItem],
        scope: &FrameworkCheckScope,
    ) -> Vec<FrameworkWatch> {
        let checked_pages: HashSet<(&str, u64, bool)> = items
            .iter()
            .map(|item| (item.1.as_str(), item.2, item.5))
            .collect();
        let mut watches = Vec::new();
        for entry in &self.state.mods {
            let Some(record) = mod_framework_record(entry) else {
                continue;
            };
            if record.pages.is_empty() {
                continue;
            }
            let watched_beyond_checked_pages = entry.status != ModStatus::Archived;
            let facts = mod_known_folder_facts(entry);
            let signature = frameworks::FrameworkSignature::new(
                std::iter::once(record.name.as_str())
                    .chain(record.namespaces.iter().map(String::as_str)),
                facts
                    .iter()
                    .flat_map(|facts| facts.ini_names.iter().map(String::as_str)),
            );
            for page in &record.pages {
                let checked =
                    checked_pages.contains(&(entry.game_id.as_str(), page.page_id, page.is_tool));
                let in_scope = watched_beyond_checked_pages
                    && !page.gone
                    && scope.includes(&entry.game_id, page.page_id, page.is_tool);
                if !checked && !in_scope {
                    continue;
                }
                watches.push(FrameworkWatch {
                    mod_id: entry.id.clone(),
                    game_id: entry.game_id.clone(),
                    page_id: page.page_id,
                    is_tool: page.is_tool,
                    signature: signature.clone(),
                    copy_ids: page.copies.iter().map(|copy| copy.file_id).collect(),
                    other_ids: page.other_file_ids.clone(),
                });
            }
        }
        watches
    }

    fn framework_scope_for_linked_check(
        &mut self,
        target_game_id: Option<&str>,
        force: bool,
        allow_network: bool,
    ) -> FrameworkCheckScope {
        if !allow_network {
            return FrameworkCheckScope::CheckedPagesOnly;
        }
        let schedule_key = target_game_id.unwrap_or_default().to_string();
        let checked_recently = self
            .frameworks
            .last_watch_only_check
            .get(&schedule_key)
            .is_some_and(|last| last.elapsed() < FRAMEWORK_WATCH_ONLY_CHECK_COOLDOWN);
        if checked_recently && !force {
            return FrameworkCheckScope::CheckedPagesOnly;
        }
        self.frameworks
            .last_watch_only_check
            .insert(schedule_key, Instant::now());
        match target_game_id {
            Some(game_id) => FrameworkCheckScope::AllPagesOfGame(game_id.to_string()),
            None => FrameworkCheckScope::AllPages,
        }
    }

    fn dispatch_update_check(
        &mut self,
        items: Vec<UpdateCheckItem>,
        framework_scope: FrameworkCheckScope,
    ) {
        let framework_watches = self.framework_watches_for_check(&items, &framework_scope);
        let fetch_watch_only_pages = framework_scope != FrameworkCheckScope::CheckedPagesOnly;
        if items.is_empty() && (!fetch_watch_only_pages || framework_watches.is_empty()) {
            return;
        }
        let peer_items = self.build_update_check_peer_context();
        self.update_check_active_items = items.clone();
        self.frameworks.active_check_scope = framework_scope;
        if self
            .update_check_tx
            .send(UpdateCheckRequest {
                generation: self.update_check_generation,
                items,
                peer_items,
                framework_watches,
                fetch_watch_only_pages,
            })
            .is_ok()
        {
            self.update_check_inflight = true;
        }
    }

    fn fetch_wanted_framework_pages(&mut self) {
        if self.frameworks.pages_to_fetch.is_empty()
            || self.update_check_inflight
            || self.folder_batch_job.is_some()
        {
            return;
        }
        let pages = std::mem::take(&mut self.frameworks.pages_to_fetch);
        self.dispatch_update_check(Vec::new(), FrameworkCheckScope::Pages(pages));
    }

    fn apply_framework_page_reports(&mut self, reports: Vec<FrameworkPageReport>) {
        if reports.is_empty() {
            return;
        }
        for report in reports {
            let FrameworkPageReport {
                mod_id,
                game_id,
                page_id,
                is_tool,
                new_copies,
                other_ids,
                known_copy_files,
                page_gone,
            } = report;
            let Some(mod_entry) = self
                .state
                .mods
                .iter_mut()
                .find(|entry| entry.id == mod_id && entry.game_id == game_id)
            else {
                continue;
            };
            let Some(record) = mod_entry
                .source
                .as_mut()
                .and_then(|source| source.framework.as_mut())
            else {
                continue;
            };
            let state_for_new_copy = if record.follows_outfit_pages() {
                FrameworkCopyState::Unjudged
            } else {
                FrameworkCopyState::NotNewer
            };
            let Some(page) = record
                .pages
                .iter_mut()
                .find(|page| page.page_id == page_id && page.is_tool == is_tool)
            else {
                continue;
            };
            let mut changed = page.gone != page_gone;
            page.gone = page_gone;
            if !page_gone {
                for file in &new_copies {
                    if !page.has_seen(file.id) {
                        page.remember(framework_copy_from_file(file, state_for_new_copy));
                        changed = true;
                    }
                }
                for file_id in other_ids {
                    if !page.has_seen(file_id) {
                        page.remember_other(file_id);
                        changed = true;
                    }
                }
                for file in &known_copy_files {
                    if let Some(copy) = page.copies.iter_mut().find(|copy| copy.file_id == file.id)
                        && copy.md5.is_none()
                        && file.md5.is_some()
                    {
                        copy.md5 = file.md5.clone();
                        changed = true;
                    }
                }
                let files = self
                    .frameworks
                    .page_files
                    .entry((game_id, page_id, is_tool))
                    .or_default();
                for file in new_copies.into_iter().chain(known_copy_files) {
                    files.insert(file.id, file);
                }
                self.frameworks
                    .thalia_ready_pages
                    .insert((mod_id, page_id, is_tool));
            }
            if changed {
                let _ = xxmi::save_mod_metadata(mod_entry);
            }
        }
        self.frameworks.ladder_dirty = true;
    }

    fn advance_framework_updates(&mut self) {
        self.forget_abandoned_framework_jobs();
        self.fetch_wanted_framework_pages();
        if self.frameworks.ladder_dirty {
            self.frameworks.ladder_dirty = false;
            self.advance_framework_ladder();
        }
        self.process_framework_switch_ons();
    }

    fn forget_abandoned_framework_jobs(&mut self) {
        if self.frameworks.splits.is_empty() && self.frameworks.copy_questions.is_empty() {
            return;
        }
        let inflight = &self.install_inflight;
        self.frameworks
            .splits
            .retain(|job_id, _| inflight.contains_key(job_id));
        let abandoned_questions: Vec<u64> = self
            .frameworks
            .copy_questions
            .keys()
            .filter(|job_id| !inflight.contains_key(job_id))
            .copied()
            .collect();
        for job_id in abandoned_questions {
            self.frameworks.copy_questions.remove(&job_id);
            self.withdraw_framework_questions(job_id);
        }
    }

    fn framework_work_pending(&self) -> bool {
        self.frameworks.ladder_dirty
            || !self.frameworks.pending_switch_ons.is_empty()
            || (!self.frameworks.pages_to_fetch.is_empty() && !self.update_check_inflight)
    }

    fn framework_dialog_open(&self) -> bool {
        !self.frameworks.questions.is_empty() || self.frameworks.undecided_dialog.is_some()
    }

    fn forget_framework_fetch_attempts(&mut self, mod_id: &str) {
        self.frameworks
            .quiet_fetch_attempted
            .retain(|(attempted_mod_id, _)| attempted_mod_id != mod_id);
        self.frameworks
            .quiet_fetch_inflight
            .retain(|(fetching_mod_id, _)| fetching_mod_id != mod_id);
        self.frameworks
            .swap_download_attempted
            .retain(|(attempted_mod_id, _)| attempted_mod_id != mod_id);
        self.frameworks
            .newer_asks_inflight
            .retain(|(asking_mod_id, _, _)| asking_mod_id != mod_id);
    }

    fn advance_framework_ladder(&mut self) {
        let mut steps = Vec::new();
        let mut games_ready_to_install: HashMap<&str, bool> = HashMap::new();
        for entry in &self.state.mods {
            if entry.status == ModStatus::Archived {
                continue;
            }
            let Some(source) = entry.source.as_ref() else {
                continue;
            };
            let Some(record) = source
                .framework
                .as_ref()
                .filter(|record| record.follows_outfit_pages())
            else {
                continue;
            };
            if source.ignore_update_always {
                continue;
            }
            let ready_to_install = *games_ready_to_install
                .entry(entry.game_id.as_str())
                .or_insert_with(|| self.game_can_download_mods(&entry.game_id));
            if !ready_to_install {
                continue;
            }
            for page in record.pages.iter().filter(|page| !page.gone) {
                self.framework_page_ladder_steps(entry, record, page, &mut steps);
            }
        }
        let mut changed_mod_ids: HashSet<String> = HashSet::new();
        for step in steps {
            match step {
                FrameworkLadderStep::SetState {
                    mod_id,
                    page_id,
                    is_tool,
                    file_id,
                    state,
                } => {
                    if self.set_framework_copy_state(&mod_id, page_id, is_tool, file_id, state) {
                        changed_mod_ids.insert(mod_id);
                    }
                }
                FrameworkLadderStep::Download {
                    mod_id,
                    page_id,
                    is_tool,
                    file_id,
                } => self.start_framework_copy_download(&mod_id, page_id, is_tool, file_id),
                FrameworkLadderStep::QuietFetch {
                    mod_id,
                    game_id,
                    page_id,
                    is_tool,
                    file,
                    installed,
                } => {
                    let fetch_key = (mod_id.clone(), file.id);
                    let request = FrameworkRequest::QuietFetch {
                        mod_id,
                        game_id,
                        page_id,
                        is_tool,
                        file,
                        installed,
                        cache_limit_bytes: self.cache_limit_bytes.load(Ordering::Relaxed),
                    };
                    if self.frameworks.request_tx.send(request).is_ok() {
                        self.frameworks
                            .quiet_fetch_attempted
                            .insert(fetch_key.clone());
                        self.frameworks.quiet_fetch_inflight.insert(fetch_key);
                    }
                }
                FrameworkLadderStep::AskNewer {
                    mod_id,
                    page_id,
                    is_tool,
                    installed_file_id,
                    installed_page_id,
                } => {
                    let page_key = (mod_id.clone(), page_id, is_tool);
                    let request = FrameworkRequest::AskNewer {
                        mod_id,
                        page_id,
                        is_tool,
                        installed_file_id,
                        installed_page_id,
                    };
                    if self.frameworks.request_tx.send(request).is_ok() {
                        self.frameworks.newer_asks_inflight.insert(page_key);
                    }
                }
            }
        }
        if changed_mod_ids.is_empty() {
            return;
        }
        for mod_entry in &mut self.state.mods {
            if changed_mod_ids.contains(&mod_entry.id) {
                let _ = xxmi::save_mod_metadata(mod_entry);
            }
        }
        self.frameworks.ladder_dirty = true;
        self.save_state();
    }

    fn framework_page_ladder_steps(
        &self,
        entry: &ModEntry,
        record: &FrameworkRecord,
        page: &crate::model::FrameworkPage,
        steps: &mut Vec<FrameworkLadderStep>,
    ) {
        let installed = record.installed_copy();
        let page_files =
            self.frameworks
                .page_files
                .get(&(entry.game_id.clone(), page.page_id, page.is_tool));
        let set_state = |file_id: u64, state: FrameworkCopyState| FrameworkLadderStep::SetState {
            mod_id: entry.id.clone(),
            page_id: page.page_id,
            is_tool: page.is_tool,
            file_id,
            state,
        };
        let mut copies_for_thalia = Vec::new();
        for copy in &page.copies {
            match copy.state {
                FrameworkCopyState::Installed
                | FrameworkCopyState::NotNewer
                | FrameworkCopyState::Undecided => {}
                FrameworkCopyState::Newer => steps.push(FrameworkLadderStep::Download {
                    mod_id: entry.id.clone(),
                    page_id: page.page_id,
                    is_tool: page.is_tool,
                    file_id: copy.file_id,
                }),
                FrameworkCopyState::AskThalia => copies_for_thalia.push(copy.file_id),
                FrameworkCopyState::Unjudged => {
                    let same_checksum = copy
                        .md5
                        .as_deref()
                        .is_some_and(|md5| record.judged_checksum(md5));
                    if same_checksum {
                        steps.push(set_state(copy.file_id, FrameworkCopyState::NotNewer));
                        continue;
                    }
                    let small_enough_to_fetch =
                        copy.file_size > 0 && copy.file_size <= frameworks::QUIET_FETCH_LIMIT_BYTES;
                    if !small_enough_to_fetch {
                        let state = framework_copy_state_from_file_names(
                            installed.map(|(_, installed)| installed.file_name.as_str()),
                            &copy.file_name,
                        );
                        steps.push(set_state(copy.file_id, state));
                        continue;
                    }
                    let fetch_key = (entry.id.clone(), copy.file_id);
                    if self.frameworks.quiet_fetch_inflight.contains(&fetch_key)
                        || self.frameworks.quiet_fetch_attempted.contains(&fetch_key)
                    {
                        continue;
                    }
                    let file = page_files
                        .and_then(|files| files.get(&copy.file_id))
                        .filter(|file| framework_file_can_download(file));
                    if let Some(file) = file {
                        steps.push(FrameworkLadderStep::QuietFetch {
                            mod_id: entry.id.clone(),
                            game_id: entry.game_id.clone(),
                            page_id: page.page_id,
                            is_tool: page.is_tool,
                            file: Box::new(file.clone()),
                            installed: framework_installed_folder(entry),
                        });
                    }
                }
            }
        }
        if copies_for_thalia.is_empty() {
            return;
        }
        let page_key = (entry.id.clone(), page.page_id, page.is_tool);
        if !self.frameworks.thalia_ready_pages.contains(&page_key)
            || self.frameworks.newer_asks_inflight.contains(&page_key)
        {
            return;
        }
        let comparable = installed.filter(|(installed_page, _)| {
            thalia::THALIA_UPDATE_MATCHING_ENABLED
                && !page.is_tool
                && !installed_page.is_tool
                && !installed_page.gone
        });
        match comparable {
            Some((installed_page, installed_copy)) => steps.push(FrameworkLadderStep::AskNewer {
                mod_id: entry.id.clone(),
                page_id: page.page_id,
                is_tool: page.is_tool,
                installed_file_id: installed_copy.file_id,
                installed_page_id: installed_page.page_id,
            }),
            None => steps.extend(
                copies_for_thalia
                    .into_iter()
                    .map(|file_id| set_state(file_id, FrameworkCopyState::Undecided)),
            ),
        }
    }

    fn set_framework_copy_state(
        &mut self,
        mod_id: &str,
        page_id: u64,
        is_tool: bool,
        file_id: u64,
        state: FrameworkCopyState,
    ) -> bool {
        self.state
            .mods
            .iter_mut()
            .find(|entry| entry.id == mod_id)
            .and_then(|entry| entry.source.as_mut())
            .and_then(|source| source.framework.as_mut())
            .and_then(|record| {
                record
                    .pages
                    .iter_mut()
                    .find(|page| page.page_id == page_id && page.is_tool == is_tool)
            })
            .is_some_and(|page| page.set_state(file_id, state))
    }

    fn start_framework_copy_download(
        &mut self,
        mod_id: &str,
        page_id: u64,
        is_tool: bool,
        file_id: u64,
    ) {
        let attempt_key = (mod_id.to_string(), file_id);
        if self
            .frameworks
            .swap_download_attempted
            .contains(&attempt_key)
        {
            return;
        }
        let already_arriving = self.frameworks.downloads.values().any(|download| {
            download.mod_id.as_deref() == Some(mod_id)
                && download.page_id == page_id
                && download.is_tool == is_tool
                && download.file_id == file_id
        });
        if already_arriving {
            return;
        }
        let Some(entry) = self.state.mods.iter().find(|entry| entry.id == mod_id) else {
            return;
        };
        let Some(record) = mod_framework_record(entry) else {
            return;
        };
        let game_id = entry.game_id.clone();
        let expected_namespaces: std::collections::BTreeSet<String> =
            record.namespaces.iter().cloned().collect();
        let Some(file) = self
            .frameworks
            .page_files
            .get(&(game_id.clone(), page_id, is_tool))
            .and_then(|files| files.get(&file_id))
            .filter(|file| framework_file_can_download(file))
            .cloned()
        else {
            return;
        };
        if !self.game_can_download_mods(&game_id) {
            return;
        }
        let task_id = self.next_background_job_id();
        self.frameworks.swap_download_attempted.insert(attempt_key);
        self.frameworks.downloads.insert(
            task_id,
            FrameworkDownload {
                game_id: game_id.clone(),
                page_id,
                is_tool,
                file_id,
                mod_id: Some(mod_id.to_string()),
                expected_namespaces,
            },
        );
        self.queue_browse_download_with_resolution(
            game_id,
            page_id,
            file.clone(),
            vec![file],
            Some(task_id),
            false,
            None,
            None,
            ModInstallState::Enabled,
            false,
            None,
            None,
        );
    }

    fn framework_quiet_fetched(
        &mut self,
        mod_id: &str,
        page: (u64, bool),
        file_id: u64,
        compared_ini_hash: Option<&str>,
        inside_check: Option<frameworks::InsideCheck>,
    ) {
        let (page_id, is_tool) = page;
        let fetch_key = (mod_id.to_string(), file_id);
        self.frameworks.quiet_fetch_inflight.remove(&fetch_key);
        self.frameworks.ladder_dirty = true;
        let Some(inside_check) = inside_check else {
            return;
        };
        let Some(entry) = self.state.mods.iter_mut().find(|entry| entry.id == mod_id) else {
            return;
        };
        if entry.ini_hash.as_deref() != compared_ini_hash {
            self.frameworks.quiet_fetch_attempted.remove(&fetch_key);
            return;
        }
        let Some(record) = entry
            .source
            .as_mut()
            .and_then(|source| source.framework.as_mut())
        else {
            return;
        };
        if record.copy_state(page_id, is_tool, file_id) != Some(FrameworkCopyState::Unjudged) {
            return;
        }
        let installed_file_name = record
            .installed_copy()
            .map(|(_, installed)| installed.file_name.clone());
        let Some(page) = record
            .pages
            .iter_mut()
            .find(|page| page.page_id == page_id && page.is_tool == is_tool)
        else {
            return;
        };
        match inside_check {
            frameworks::InsideCheck::Newer => {
                page.set_state(file_id, FrameworkCopyState::Newer);
            }
            frameworks::InsideCheck::SameOrOlder => {
                page.set_state(file_id, FrameworkCopyState::NotNewer);
            }
            frameworks::InsideCheck::DifferentFramework => page.remember_other(file_id),
            frameworks::InsideCheck::CannotTell => {
                let arriving_file_name = page
                    .copy(file_id)
                    .map(|copy| copy.file_name.clone())
                    .unwrap_or_default();
                page.set_state(
                    file_id,
                    framework_copy_state_from_file_names(
                        installed_file_name.as_deref(),
                        &arriving_file_name,
                    ),
                );
            }
        }
        let _ = xxmi::save_mod_metadata(entry);
        self.save_state();
    }

    fn framework_newer_answered(
        &mut self,
        mod_id: &str,
        page: (u64, bool),
        compared_file_id: u64,
        answer: FrameworkNewerAnswer,
    ) {
        let (page_id, is_tool) = page;
        let page_key = (mod_id.to_string(), page_id, is_tool);
        self.frameworks.newer_asks_inflight.remove(&page_key);
        self.frameworks.ladder_dirty = true;
        let verdict = match answer {
            FrameworkNewerAnswer::TryLater => {
                self.frameworks.thalia_ready_pages.remove(&page_key);
                return;
            }
            FrameworkNewerAnswer::CannotTell => FrameworkThaliaVerdict::CannotTell,
            FrameworkNewerAnswer::Answered(response) => match (response.action, response.id) {
                (thalia::UpdateMatchAction::Update, Some(file_id)) => {
                    FrameworkThaliaVerdict::Newer(file_id)
                }
                (thalia::UpdateMatchAction::Current, _) => FrameworkThaliaVerdict::NotNewer,
                _ => FrameworkThaliaVerdict::CannotTell,
            },
        };
        let Some(entry) = self.state.mods.iter_mut().find(|entry| entry.id == mod_id) else {
            return;
        };
        let Some(record) = entry
            .source
            .as_mut()
            .and_then(|source| source.framework.as_mut())
        else {
            return;
        };
        let still_compared = record
            .installed_copy()
            .is_some_and(|(_, installed)| installed.file_id == compared_file_id);
        if !still_compared {
            return;
        }
        let Some(page) = record
            .pages
            .iter_mut()
            .find(|page| page.page_id == page_id && page.is_tool == is_tool)
        else {
            return;
        };
        let asked_about = |copy: &FrameworkCopy| copy.state == FrameworkCopyState::AskThalia;
        let newer_file_id = match verdict {
            FrameworkThaliaVerdict::Newer(file_id) => page
                .copy(file_id)
                .is_some_and(asked_about)
                .then_some(file_id),
            _ => None,
        };
        let mut changed = false;
        for copy in page.copies.iter_mut().filter(|copy| asked_about(copy)) {
            copy.state = match (&verdict, newer_file_id) {
                (_, Some(file_id)) if copy.file_id == file_id => FrameworkCopyState::Newer,
                (_, Some(_)) | (FrameworkThaliaVerdict::NotNewer, None) => {
                    FrameworkCopyState::NotNewer
                }
                (FrameworkThaliaVerdict::Newer(_) | FrameworkThaliaVerdict::CannotTell, None) => {
                    FrameworkCopyState::Undecided
                }
            };
            changed = true;
        }
        if changed {
            let _ = xxmi::save_mod_metadata(entry);
            self.save_state();
        }
    }

    fn process_framework_switch_ons(&mut self) {
        if self.frameworks.pending_switch_ons.is_empty() {
            return;
        }
        let busy = !self.browse_download_queue.is_empty()
            || !self.browse_download_inflight.is_empty()
            || !self.install_queue.is_empty()
            || !self.install_inflight.is_empty()
            || !self.pending_install_finalize.is_empty()
            || !self.thalia_install_bundles.is_empty()
            || self.refresh_inflight;
        if busy {
            return;
        }
        let pending = std::mem::take(&mut self.frameworks.pending_switch_ons);
        let mut still_waiting = Vec::new();
        for (game_id, mod_id) in pending {
            let game_uses_frameworks = self
                .state
                .games
                .iter()
                .any(|game| game.definition.id == game_id && game.is_xxmi());
            if !game_uses_frameworks {
                continue;
            }
            if !self.framework_facts_are_current(&game_id) {
                still_waiting.push((game_id, mod_id));
                continue;
            }
            self.request_missing_frameworks(&game_id, &mod_id);
        }
        self.frameworks.pending_switch_ons = still_waiting;
    }

    fn framework_namespace_is_expected(&self, game_id: &str, namespace: &str) -> bool {
        self.frameworks.downloads.values().any(|download| {
            download.game_id == game_id && download.expected_namespaces.contains(namespace)
        }) || self.frameworks.arrivals.values().any(|arrival| {
            arrival.game_id == game_id
                && arrival
                    .namespaces
                    .iter()
                    .any(|arriving| arriving == namespace)
        })
    }

    fn framework_names_missing_for(
        &self,
        game_id: &str,
        page_id: u64,
        facts: &frameworks::FolderFacts,
    ) -> Vec<String> {
        let mut missing = facts.called_but_not_declared();
        if let Some(loader_namespaces) = self.frameworks.loader_namespaces.get(game_id) {
            missing.retain(|name| !loader_namespaces.contains(name));
        }
        for other in self
            .state
            .mods
            .iter()
            .filter(|other| other.game_id == game_id && other.status != ModStatus::Archived)
        {
            if missing.is_empty() {
                break;
            }
            if let Some(other_facts) = mod_known_folder_facts(other) {
                missing.retain(|name| !other_facts.declares(name));
            }
        }
        missing.retain(|name| {
            !self
                .frameworks
                .pages_without
                .contains(&(game_id.to_string(), page_id, name.clone()))
                && !self.framework_namespace_is_expected(game_id, name)
        });
        missing.into_iter().collect()
    }

    fn request_missing_frameworks(&mut self, game_id: &str, mod_id: &str) {
        let Some(entry) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id && entry.game_id == game_id)
        else {
            return;
        };
        if entry.status != ModStatus::Active || mod_framework_record(entry).is_some() {
            return;
        }
        let Some(page_id) = entry
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .filter(|link| link.mod_id != 0 && !gamebanana::is_tool_url(&link.url))
            .map(|link| link.mod_id)
        else {
            return;
        };
        let Some(facts) = mod_known_folder_facts(entry) else {
            return;
        };
        let names = self.framework_names_missing_for(game_id, page_id, &facts);
        let inflight_key = (game_id.to_string(), mod_id.to_string());
        if names.is_empty()
            || self
                .frameworks
                .missing_fetches_inflight
                .contains(&inflight_key)
            || !self.game_can_download_mods(game_id)
        {
            return;
        }
        let request = FrameworkRequest::FindMissing {
            game_id: game_id.to_string(),
            mod_id: mod_id.to_string(),
            page_id,
            names,
        };
        if self.frameworks.request_tx.send(request).is_ok() {
            self.frameworks
                .missing_fetches_inflight
                .insert(inflight_key);
        }
    }

    fn framework_missing_found(
        &mut self,
        game_id: &str,
        mod_id: &str,
        page_id: u64,
        names: Vec<String>,
        found: Option<Vec<(String, gamebanana::ModFile)>>,
    ) {
        self.frameworks
            .missing_fetches_inflight
            .remove(&(game_id.to_string(), mod_id.to_string()));
        let Some(found) = found else {
            return;
        };
        for name in names {
            if !found.iter().any(|(found_name, _)| *found_name == name) {
                self.frameworks
                    .pages_without
                    .insert((game_id.to_string(), page_id, name));
            }
        }
        let still_missing = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id && entry.game_id == game_id)
            .filter(|entry| entry.status == ModStatus::Active)
            .and_then(mod_known_folder_facts)
            .map(|facts| self.framework_names_missing_for(game_id, page_id, &facts))
            .unwrap_or_default();
        let mut files: Vec<(gamebanana::ModFile, std::collections::BTreeSet<String>)> = Vec::new();
        for (name, file) in found {
            if !still_missing.contains(&name) || !framework_file_can_download(&file) {
                continue;
            }
            match files.iter_mut().find(|(known, _)| known.id == file.id) {
                Some((_, namespaces)) => {
                    namespaces.insert(name);
                }
                None => files.push((file, std::iter::once(name).collect())),
            }
        }
        if files.is_empty() || !self.game_can_download_mods(game_id) {
            return;
        }
        let text = self.text();
        for (file, expected_namespaces) in files {
            let task_id = self.next_background_job_id();
            self.frameworks.downloads.insert(
                task_id,
                FrameworkDownload {
                    game_id: game_id.to_string(),
                    page_id,
                    is_tool: false,
                    file_id: file.id,
                    mod_id: None,
                    expected_namespaces,
                },
            );
            self.log_action(text.framework_fetched_action(), &file.file_name);
            self.queue_browse_download_with_resolution(
                game_id.to_string(),
                page_id,
                file.clone(),
                vec![file],
                Some(task_id),
                false,
                None,
                None,
                ModInstallState::Enabled,
                false,
                None,
                None,
            );
        }
    }

    fn framework_undecided_copy(&self, mod_id: &str) -> Option<(String, u64, bool, FrameworkCopy)> {
        let entry = self.state.mods.iter().find(|entry| entry.id == mod_id)?;
        let record = mod_framework_record(entry).filter(|record| record.follows_outfit_pages())?;
        record
            .copies_in(FrameworkCopyState::Undecided)
            .filter(|(page, _)| !page.gone)
            .max_by_key(|(_, copy)| copy.file_id)
            .map(|(page, copy)| {
                (
                    record.name.clone(),
                    page.page_id,
                    page.is_tool,
                    copy.clone(),
                )
            })
    }

    fn open_framework_undecided_dialog(&mut self, mod_id: &str) {
        if self.framework_undecided_copy(mod_id).is_some() {
            self.frameworks.undecided_dialog = Some(mod_id.to_string());
        }
    }

    fn set_framework_held(&mut self, mod_id: &str, held: bool) {
        let Some(entry) = self
            .state
            .mods
            .iter_mut()
            .find(|entry| entry.id == mod_id && mod_is_managed_framework(entry))
        else {
            return;
        };
        let Some(source) = entry.source.as_mut() else {
            return;
        };
        if source.ignore_update_always == held {
            return;
        }
        source.ignore_update_always = held;
        let _ = xxmi::save_mod_metadata(entry);
        if held {
            if self.frameworks.undecided_dialog.as_deref() == Some(mod_id) {
                self.frameworks.undecided_dialog = None;
            }
        } else {
            self.frameworks.ladder_dirty = true;
        }
        self.save_state();
    }

    fn open_page_in_browse(&mut self, ctx: &egui::Context, page_id: u64, is_tool: bool) {
        if is_tool {
            ctx.open_url(egui::OpenUrl::new_tab(gamebanana::browser_url_typed(
                page_id, true,
            )));
        } else {
            self.open_linked_mod_in_browse(page_id);
        }
    }

    fn answer_framework_undecided_copy(&mut self, mod_id: &str, use_that_copy: bool) {
        self.frameworks.undecided_dialog = None;
        let chosen = self
            .framework_undecided_copy(mod_id)
            .filter(|_| use_that_copy)
            .map(|(_, page_id, is_tool, copy)| (page_id, is_tool, copy.file_id));
        let Some(entry) = self.state.mods.iter_mut().find(|entry| entry.id == mod_id) else {
            return;
        };
        let game_id = entry.game_id.clone();
        let Some(record) = entry
            .source
            .as_mut()
            .and_then(|source| source.framework.as_mut())
        else {
            return;
        };
        for page in &mut record.pages {
            let page_key = (page.page_id, page.is_tool);
            for copy in &mut page.copies {
                if copy.state != FrameworkCopyState::Undecided {
                    continue;
                }
                let is_chosen = chosen == Some((page_key.0, page_key.1, copy.file_id));
                copy.state = if is_chosen {
                    FrameworkCopyState::Newer
                } else {
                    FrameworkCopyState::NotNewer
                };
            }
        }
        let _ = xxmi::save_mod_metadata(entry);
        if let Some((page_id, is_tool, file_id)) = chosen {
            self.frameworks
                .swap_download_attempted
                .remove(&(mod_id.to_string(), file_id));
            let page_key = (game_id, page_id, is_tool);
            let file_is_known = self
                .frameworks
                .page_files
                .get(&page_key)
                .and_then(|files| files.get(&file_id))
                .is_some_and(framework_file_can_download);
            if !file_is_known {
                self.frameworks.pages_to_fetch.insert(page_key);
            }
        }
        self.frameworks.ladder_dirty = true;
        self.save_state();
    }
}

#[cfg(test)]
mod framework_decision_tests {
    use super::*;
    use crate::model::FrameworkPage;

    fn page_file(id: u64, file_name: &str, md5: Option<&str>) -> gamebanana::ModFile {
        gamebanana::ModFile {
            id,
            file_name: file_name.to_string(),
            file_size: 1,
            date_added: 0,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some(format!("https://example.com/{file_name}")),
            is_archived: false,
            md5: md5.map(str::to_string),
        }
    }

    fn known_copy(
        file_id: u64,
        file_name: &str,
        md5: Option<&str>,
        state: FrameworkCopyState,
    ) -> FrameworkCopy {
        framework_copy_from_file(&page_file(file_id, file_name, md5), state)
    }

    fn outfit_page(page_id: u64, copies: Vec<FrameworkCopy>) -> FrameworkPage {
        FrameworkPage {
            page_id,
            copies,
            ..FrameworkPage::default()
        }
    }

    fn framework_record(pages: Vec<FrameworkPage>) -> FrameworkRecord {
        FrameworkRecord {
            name: "NeonRookieFX".to_string(),
            namespaces: vec!["neonrookiefx".to_string()],
            came_from: pages.first().map(|page| FrameworkOrigin {
                page_id: page.page_id,
                is_tool: page.is_tool,
            }),
            pages,
            ..FrameworkRecord::default()
        }
    }

    fn framework_mod(id: &str, game_id: &str, record: FrameworkRecord) -> ModEntry {
        ModEntry {
            id: id.to_string(),
            game_id: game_id.to_string(),
            folder_name: record.name.clone(),
            root_path: PathBuf::from(format!("C:/mods/{id}")),
            status: ModStatus::Active,
            metadata: Default::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: Some(ModSourceData {
                framework: Some(record),
                ..ModSourceData::default()
            }),
            update_state: Default::default(),
        }
    }

    fn tracking(files: &[&gamebanana::ModFile]) -> FileSetRecipe {
        FileSetRecipe {
            selected_file_ids: files.iter().map(|file| file.id).collect(),
            selected_file_names: files.iter().map(|file| file.file_name.clone()).collect(),
            selected_files_meta: files
                .iter()
                .map(|file| tracked_file_meta_from_mod_file(file))
                .collect(),
            ..FileSetRecipe::default()
        }
    }

    fn is_swap(outcome: &FrameworkArrivalOutcome) -> bool {
        matches!(outcome, FrameworkArrivalOutcome::Swap)
    }

    fn skipped_as(outcome: &FrameworkArrivalOutcome) -> Option<FrameworkCopyState> {
        match outcome {
            FrameworkArrivalOutcome::Skip(state) => Some(*state),
            _ => None,
        }
    }

    #[test]
    fn file_names_decide_only_when_both_carry_a_version_and_they_differ() {
        assert_eq!(
            framework_copy_state_from_file_names(None, "neonrookiefx_v3.zip"),
            FrameworkCopyState::Undecided
        );
        assert_eq!(
            framework_copy_state_from_file_names(
                Some("neonrookiefx_v2_c99f1.zip"),
                "neonrookiefx_v3_0a1b2.zip"
            ),
            FrameworkCopyState::Newer
        );
        assert_eq!(
            framework_copy_state_from_file_names(
                Some("neonrookiefx_v2.1.zip"),
                "NeonRookieFX_v2.zip"
            ),
            FrameworkCopyState::NotNewer
        );
        assert_eq!(
            framework_copy_state_from_file_names(
                Some("neonrookiefx_v2.zip"),
                "neonrookiefx_v2_c99f1.zip"
            ),
            FrameworkCopyState::AskThalia
        );
        assert_eq!(
            framework_copy_state_from_file_names(Some("neonrookiefx.zip"), "neonrookiefx_v3.zip"),
            FrameworkCopyState::AskThalia
        );
    }

    #[test]
    fn an_arriving_copy_with_a_judged_checksum_is_skipped_before_names_are_read() {
        let record = framework_record(vec![outfit_page(
            100,
            vec![
                known_copy(
                    1,
                    "neonrookiefx_v2.zip",
                    Some("AAAA"),
                    FrameworkCopyState::Installed,
                ),
                known_copy(
                    2,
                    "neonrookiefx_v1.zip",
                    Some("bbbb"),
                    FrameworkCopyState::NotNewer,
                ),
                known_copy(
                    3,
                    "neonrookiefx_x.zip",
                    Some("cccc"),
                    FrameworkCopyState::Undecided,
                ),
            ],
        )]);

        let same_as_mine = page_file(50, "neonrookiefx_v9.zip", Some("aaaa"));
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &record,
                &same_as_mine
            )),
            Some(FrameworkCopyState::NotNewer)
        );
        let same_as_an_older_one = page_file(51, "neonrookiefx_v9.zip", Some("BBBB"));
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &record,
                &same_as_an_older_one
            )),
            Some(FrameworkCopyState::NotNewer)
        );
        let same_as_an_undecided_one = page_file(52, "neonrookiefx_v9.zip", Some("cccc"));
        assert!(is_swap(&framework_outcome_from_checksum_and_names(
            &record,
            &same_as_an_undecided_one
        )));
    }

    #[test]
    fn an_arriving_copy_swaps_only_on_a_higher_version_in_its_file_name() {
        let record = framework_record(vec![outfit_page(
            100,
            vec![known_copy(
                1,
                "neonrookiefx_v2.zip",
                None,
                FrameworkCopyState::Installed,
            )],
        )]);

        assert!(is_swap(&framework_outcome_from_checksum_and_names(
            &record,
            &page_file(50, "neonrookiefx_v3.zip", None)
        )));
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &record,
                &page_file(51, "neonrookiefx_v1.zip", None)
            )),
            Some(FrameworkCopyState::NotNewer)
        );
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &record,
                &page_file(52, "neonrookiefx_v2_c99f1.zip", None)
            )),
            Some(FrameworkCopyState::AskThalia)
        );
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &record,
                &page_file(53, "shaders.zip", Some(""))
            )),
            Some(FrameworkCopyState::AskThalia)
        );

        let nothing_installed = framework_record(vec![outfit_page(
            100,
            vec![known_copy(
                1,
                "neonrookiefx_v2.zip",
                None,
                FrameworkCopyState::NotNewer,
            )],
        )]);
        assert_eq!(
            skipped_as(&framework_outcome_from_checksum_and_names(
                &nothing_installed,
                &page_file(50, "neonrookiefx_v3.zip", None)
            )),
            Some(FrameworkCopyState::Undecided)
        );
    }

    #[test]
    fn covered_files_are_the_known_copies_on_that_page_of_that_game() {
        let mut tool_page = outfit_page(
            100,
            vec![known_copy(
                9,
                "tool_copy.zip",
                None,
                FrameworkCopyState::NotNewer,
            )],
        );
        tool_page.is_tool = true;
        let mut neonrookie_page = outfit_page(
            100,
            vec![
                known_copy(
                    1,
                    "neonrookiefx_v2.zip",
                    None,
                    FrameworkCopyState::Installed,
                ),
                known_copy(2, "neonrookiefx_v1.zip", None, FrameworkCopyState::NotNewer),
            ],
        );
        neonrookie_page.other_file_ids = vec![7];
        let mods = vec![
            framework_mod(
                "neonrookie",
                "endfield",
                framework_record(vec![
                    neonrookie_page,
                    outfit_page(
                        200,
                        vec![known_copy(
                            3,
                            "elsewhere.zip",
                            None,
                            FrameworkCopyState::Unjudged,
                        )],
                    ),
                    tool_page,
                ]),
            ),
            framework_mod(
                "efmittl",
                "endfield",
                framework_record(vec![outfit_page(
                    100,
                    vec![known_copy(
                        4,
                        "efmittl.zip",
                        None,
                        FrameworkCopyState::Undecided,
                    )],
                )]),
            ),
            framework_mod(
                "other-game",
                "zzz",
                framework_record(vec![outfit_page(
                    100,
                    vec![known_copy(
                        5,
                        "other_game.zip",
                        None,
                        FrameworkCopyState::Installed,
                    )],
                )]),
            ),
        ];

        assert_eq!(
            framework_covered_file_ids(&mods, "endfield", 100, false),
            HashSet::from([1, 2, 4])
        );
        assert_eq!(
            framework_covered_file_ids(&mods, "endfield", 100, true),
            HashSet::from([9])
        );
        assert!(framework_covered_file_ids(&mods, "endfield", 300, false).is_empty());
    }

    #[test]
    fn covered_files_leave_the_page_the_outfits_are_checked_against() {
        let outfit_file = page_file(10, "outfit.zip", None);
        let framework_file = page_file(11, "neonrookiefx_v2.zip", None);
        let mut old_framework_file = page_file(12, "neonrookiefx_v1.zip", None);
        old_framework_file.is_archived = true;
        let mut profile = gamebanana::ProfileResponse {
            id: 100,
            files: vec![outfit_file.clone(), framework_file.clone()],
            archived_files: vec![old_framework_file.clone()],
            ..Default::default()
        };
        profile
            .file_listings
            .insert(10, vec!["Outfit/outfit.ini".to_owned()]);
        profile
            .file_listings
            .insert(11, vec!["NeonRookieFX/NeonRookieFX.ini".to_owned()]);
        let outfit_alone = tracking(&[&outfit_file]);
        let covered = HashSet::from([11, 12]);

        assert!(
            profile_without_framework_copies(&profile, &HashSet::new(), [&outfit_alone]).is_none()
        );
        assert!(
            profile_without_framework_copies(&profile, &HashSet::from([99]), [&outfit_alone])
                .is_none()
        );

        let filtered =
            profile_without_framework_copies(&profile, &covered, [&outfit_alone]).unwrap();
        assert_eq!(
            filtered
                .files
                .iter()
                .map(|file| file.id)
                .collect::<Vec<_>>(),
            vec![10]
        );
        assert!(filtered.archived_files.is_empty());
        assert_eq!(
            filtered.file_listings.keys().copied().collect::<Vec<_>>(),
            vec![10]
        );

        let outfit_still_tracking_it = tracking(&[&outfit_file, &framework_file]);
        let filtered =
            profile_without_framework_copies(&profile, &covered, [&outfit_still_tracking_it])
                .unwrap();
        assert_eq!(
            filtered
                .files
                .iter()
                .map(|file| file.id)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
        assert!(filtered.archived_files.is_empty());

        let outfit_tracking_both = tracking(&[&outfit_file, &framework_file, &old_framework_file]);
        assert!(
            profile_without_framework_copies(&profile, &covered, [&outfit_tracking_both]).is_none()
        );
    }

    #[test]
    fn a_covered_file_leaves_what_an_outfit_tracks_unless_it_is_all_the_outfit_tracks() {
        let outfit_file = page_file(10, "outfit.zip", None);
        let framework_file = page_file(11, "neonrookiefx_v2.zip", None);
        let covered = HashSet::from([11]);

        let mut both = tracking(&[&outfit_file, &framework_file]);
        assert!(remove_covered_files_from_file_set(&mut both, &covered));
        assert_eq!(both.selected_file_ids, vec![10]);
        assert_eq!(both.selected_file_names, vec!["outfit.zip"]);
        assert_eq!(
            both.selected_files_meta
                .iter()
                .map(|meta| meta.file_id)
                .collect::<Vec<_>>(),
            vec![10]
        );

        let mut outfit_only = tracking(&[&outfit_file]);
        assert!(!remove_covered_files_from_file_set(
            &mut outfit_only,
            &covered
        ));
        assert_eq!(outfit_only, tracking(&[&outfit_file]));

        let mut framework_only = tracking(&[&framework_file]);
        assert!(!remove_covered_files_from_file_set(
            &mut framework_only,
            &covered
        ));
        assert_eq!(framework_only, tracking(&[&framework_file]));
    }

    #[test]
    fn a_framework_card_shows_only_a_hold_or_an_undecided_copy() {
        let installed = || {
            known_copy(
                1,
                "neonrookiefx_v2.zip",
                None,
                FrameworkCopyState::Installed,
            )
        };
        let undecided = || known_copy(2, "neonrookiefx_x.zip", None, FrameworkCopyState::Undecided);

        let quiet = framework_mod(
            "quiet",
            "endfield",
            framework_record(vec![outfit_page(
                100,
                vec![
                    installed(),
                    known_copy(3, "neonrookiefx_v3.zip", None, FrameworkCopyState::Newer),
                    known_copy(
                        4,
                        "neonrookiefx_v1.zip",
                        None,
                        FrameworkCopyState::AskThalia,
                    ),
                ],
            )]),
        );
        assert!(framework_card_notice(&quiet).is_none());

        let mut asking = framework_mod(
            "asking",
            "endfield",
            framework_record(vec![outfit_page(100, vec![installed(), undecided()])]),
        );
        assert!(framework_card_notice(&asking) == Some(FrameworkCardNotice::UndecidedCopy));

        asking.source.as_mut().unwrap().ignore_update_always = true;
        assert!(framework_card_notice(&asking) == Some(FrameworkCardNotice::Held));

        let mut gone_page = outfit_page(200, vec![undecided()]);
        gone_page.gone = true;
        let on_a_gone_page = framework_mod(
            "gone",
            "endfield",
            framework_record(vec![outfit_page(100, vec![installed()]), gone_page]),
        );
        assert!(framework_card_notice(&on_a_gone_page).is_none());

        let mut own_page_record =
            framework_record(vec![outfit_page(100, vec![installed(), undecided()])]);
        own_page_record.own_page = true;
        let mut own_page = framework_mod("own-page", "endfield", own_page_record);
        own_page.source.as_mut().unwrap().ignore_update_always = true;
        assert!(framework_card_notice(&own_page).is_none());

        let mut left_record =
            framework_record(vec![outfit_page(100, vec![installed(), undecided()])]);
        left_record.left_as_found = true;
        assert!(framework_card_notice(&framework_mod("left", "endfield", left_record)).is_none());
    }

    #[test]
    fn page_actions_use_the_page_the_installed_copy_came_from() {
        let installed = || {
            known_copy(
                1,
                "neonrookiefx_v2.zip",
                None,
                FrameworkCopyState::Installed,
            )
        };
        let mut from_an_outfit_page = framework_mod(
            "neonrookie",
            "endfield",
            framework_record(vec![
                outfit_page(100, vec![installed()]),
                outfit_page(200, Vec::new()),
            ]),
        );
        assert_eq!(
            framework_page_for_actions(&from_an_outfit_page),
            Some((100, false))
        );
        assert_eq!(
            mod_page_for_actions(&from_an_outfit_page),
            Some((100, false))
        );

        from_an_outfit_page
            .source
            .as_mut()
            .unwrap()
            .framework
            .as_mut()
            .unwrap()
            .pages[0]
            .gone = true;
        assert_eq!(framework_page_for_actions(&from_an_outfit_page), None);
        assert_eq!(mod_page_for_actions(&from_an_outfit_page), None);

        let mut dropped_in = framework_record(vec![outfit_page(100, vec![installed()])]);
        dropped_in.came_from = None;
        assert_eq!(
            framework_page_for_actions(&framework_mod("dropped", "endfield", dropped_in)),
            None
        );

        let mut own_page_record = framework_record(vec![outfit_page(100, vec![installed()])]);
        own_page_record.own_page = true;
        assert_eq!(
            framework_page_for_actions(&framework_mod("own-page", "endfield", own_page_record)),
            None
        );
    }

    #[test]
    fn the_first_look_checks_every_active_mod_and_later_looks_only_the_newly_active() {
        let ids = |names: &[&str]| -> HashSet<String> {
            names.iter().map(|name| name.to_string()).collect()
        };
        let sorted = |mut mod_ids: Vec<String>| {
            mod_ids.sort();
            mod_ids
        };
        let active = ids(&["si", "tangtang"]);

        assert_eq!(
            sorted(active_mods_not_checked_yet(None, &active)),
            vec!["si", "tangtang"]
        );
        assert_eq!(
            active_mods_not_checked_yet(Some(&ids(&["tangtang", "switched-off-since"])), &active),
            vec!["si"]
        );
        assert!(active_mods_not_checked_yet(Some(&active), &active).is_empty());
    }
}
