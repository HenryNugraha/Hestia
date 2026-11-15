#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameworkVerdict {
    Regular,
    Framework,
}

enum FrameworkDecision {
    Verdict(FrameworkVerdict),
    Wait,
    AskThalia { page_id: u64, file_id: u64 },
    AskUser,
}

enum FrameworkArrivalOutcome {
    Swap,
    Skip(FrameworkCopyState),
    AskUser,
}

struct FrameworkReview {
    pending: PendingImport,
    scans: Vec<frameworks::CandidateScan>,
    user_verdict: Option<FrameworkVerdict>,
}

struct FrameworkSplitPart {
    candidate_index: usize,
    movable: Option<frameworks::MovablePart>,
}

struct FrameworkSplit {
    pending: PendingImport,
    scans: Vec<frameworks::CandidateScan>,
    parts: Vec<FrameworkSplitPart>,
    source: FrameworkArrivalSource,
    parent_is_framework_download: bool,
}

struct FrameworkWaitingArrival {
    pending: PendingImport,
    scan: frameworks::CandidateScan,
    source: FrameworkArrivalSource,
}

struct FrameworkCopyQuestion {
    pending: PendingImport,
    scan: frameworks::CandidateScan,
    source: FrameworkArrivalSource,
    existing_mod_id: String,
}

#[derive(Clone)]
enum FrameworkQuestion {
    IsFramework { job_id: u64, name: String },
    UseArrivingCopy { job_id: u64, name: String },
}

impl FrameworkQuestion {
    fn job_id(&self) -> u64 {
        match self {
            Self::IsFramework { job_id, .. } | Self::UseArrivingCopy { job_id, .. } => *job_id,
        }
    }
}

struct FrameworkBatchJob {
    game_id: String,
    page: Option<(u64, bool)>,
    has_models: bool,
}

#[derive(Clone)]
struct FrameworkArrivalSource {
    page: Option<(u64, bool)>,
    file: Option<gamebanana::ModFile>,
    embedded: bool,
    thalia_name: Option<String>,
    bundle_job: bool,
}

#[derive(Clone)]
struct FrameworkArrival {
    game_id: String,
    name: String,
    namespaces: Vec<String>,
    page: Option<(u64, bool)>,
    file: Option<gamebanana::ModFile>,
    swap_target: Option<String>,
}

struct FrameworkOwnPageArrival {
    game_id: String,
    candidates: Vec<(String, Vec<String>)>,
}

#[derive(Clone)]
struct FrameworkDownload {
    game_id: String,
    page_id: u64,
    is_tool: bool,
    file_id: u64,
    mod_id: Option<String>,
    expected_namespaces: std::collections::BTreeSet<String>,
}

fn active_mods_not_checked_yet(
    active_at_last_check: Option<&HashSet<String>>,
    active: &HashSet<String>,
) -> Vec<String> {
    active
        .iter()
        .filter(|mod_id| active_at_last_check.is_none_or(|checked| !checked.contains(*mod_id)))
        .cloned()
        .collect()
}

struct FrameworkJobFile {
    page_id: u64,
    is_tool: bool,
    file: gamebanana::ModFile,
}

struct FrameworkClassifyAnswer {
    file_type: Option<thalia::UpdateMatchFileType>,
    name: Option<String>,
}

struct FrameworkRuntime {
    request_tx: WorkerTx<FrameworkRequest>,
    event_rx: WorkerRx<FrameworkEvent>,
    reviews: HashMap<u64, FrameworkReview>,
    splits: HashMap<u64, FrameworkSplit>,
    waiting_arrivals: HashMap<u64, FrameworkWaitingArrival>,
    copy_questions: HashMap<u64, FrameworkCopyQuestion>,
    questions: VecDeque<FrameworkQuestion>,
    inspected: HashSet<u64>,
    session_calls: HashMap<String, std::collections::BTreeSet<String>>,
    batch_jobs: HashMap<u64, FrameworkBatchJob>,
    bundle_frameworks: HashSet<u64>,
    arrivals: HashMap<u64, FrameworkArrival>,
    own_page_arrivals: HashMap<u64, FrameworkOwnPageArrival>,
    downloads: HashMap<u64, FrameworkDownload>,
    classify_answers: HashMap<(u64, u64), FrameworkClassifyAnswer>,
    classify_inflight: HashSet<(u64, u64)>,
    page_files: HashMap<(String, u64, bool), HashMap<u64, gamebanana::ModFile>>,
    loader_namespaces: HashMap<String, std::collections::BTreeSet<String>>,
    open_facts_requests: HashMap<String, u32>,
    warm_games: HashSet<String>,
    quiet_fetch_attempted: HashSet<(String, u64)>,
    quiet_fetch_inflight: HashSet<(String, u64)>,
    newer_asks_inflight: HashSet<(String, u64, bool)>,
    thalia_ready_pages: HashSet<(String, u64, bool)>,
    swap_download_attempted: HashSet<(String, u64)>,
    pages_to_fetch: HashSet<(String, u64, bool)>,
    active_check_scope: FrameworkCheckScope,
    last_watch_only_check: HashMap<String, Instant>,
    pages_without: HashSet<(String, u64, String)>,
    active_snapshot: HashMap<String, HashSet<String>>,
    pending_switch_ons: Vec<(String, String)>,
    missing_fetches_inflight: HashSet<(String, String)>,
    ladder_dirty: bool,
    undecided_dialog: Option<String>,
}

impl FrameworkRuntime {
    fn new(request_tx: WorkerTx<FrameworkRequest>, event_rx: WorkerRx<FrameworkEvent>) -> Self {
        Self {
            request_tx,
            event_rx,
            reviews: HashMap::new(),
            splits: HashMap::new(),
            waiting_arrivals: HashMap::new(),
            copy_questions: HashMap::new(),
            questions: VecDeque::new(),
            inspected: HashSet::new(),
            session_calls: HashMap::new(),
            batch_jobs: HashMap::new(),
            bundle_frameworks: HashSet::new(),
            arrivals: HashMap::new(),
            own_page_arrivals: HashMap::new(),
            downloads: HashMap::new(),
            classify_answers: HashMap::new(),
            classify_inflight: HashSet::new(),
            page_files: HashMap::new(),
            loader_namespaces: HashMap::new(),
            open_facts_requests: HashMap::new(),
            warm_games: HashSet::new(),
            quiet_fetch_attempted: HashSet::new(),
            quiet_fetch_inflight: HashSet::new(),
            newer_asks_inflight: HashSet::new(),
            thalia_ready_pages: HashSet::new(),
            swap_download_attempted: HashSet::new(),
            pages_to_fetch: HashSet::new(),
            active_check_scope: FrameworkCheckScope::default(),
            last_watch_only_check: HashMap::new(),
            pages_without: HashSet::new(),
            active_snapshot: HashMap::new(),
            pending_switch_ons: Vec::new(),
            missing_fetches_inflight: HashSet::new(),
            ladder_dirty: false,
            undecided_dialog: None,
        }
    }

    fn batch_memory_is_empty(&self) -> bool {
        self.inspected.is_empty()
            && self.session_calls.is_empty()
            && self.batch_jobs.is_empty()
            && self.bundle_frameworks.is_empty()
            && self.arrivals.is_empty()
            && self.own_page_arrivals.is_empty()
    }

    fn namespaces_arriving(
        &self,
        game_id: &str,
        namespaces: &std::collections::BTreeSet<String>,
    ) -> bool {
        self.arrivals.values().any(|arrival| {
            arrival.game_id == game_id
                && arrival
                    .namespaces
                    .iter()
                    .any(|namespace| namespaces.contains(namespace))
        }) || self.own_page_arrivals.values().any(|arrival| {
            arrival.game_id == game_id
                && arrival
                    .candidates
                    .iter()
                    .flat_map(|(_, candidate_namespaces)| candidate_namespaces)
                    .any(|namespace| namespaces.contains(namespace))
        })
    }
}

fn mod_framework_record(mod_entry: &ModEntry) -> Option<&FrameworkRecord> {
    mod_entry
        .source
        .as_ref()
        .and_then(|source| source.framework.as_ref())
}

fn mod_is_managed_framework(mod_entry: &ModEntry) -> bool {
    mod_framework_record(mod_entry).is_some_and(FrameworkRecord::follows_outfit_pages)
}

fn mod_known_folder_facts(mod_entry: &ModEntry) -> Option<Arc<frameworks::FolderFacts>> {
    frameworks::known_folder_facts(
        &mod_entry.root_path,
        mod_entry.ini_hash.as_deref(),
        mod_entry.content_mtime,
    )
}

fn framework_installed_folder(mod_entry: &ModEntry) -> frameworks::InstalledFolder {
    frameworks::InstalledFolder {
        mod_id: mod_entry.id.clone(),
        root_path: mod_entry.root_path.clone(),
        ini_hash: mod_entry.ini_hash.clone(),
        content_mtime: mod_entry.content_mtime,
        recorded_framework: mod_framework_record(mod_entry).is_some(),
    }
}

fn framework_copy_from_file(
    file: &gamebanana::ModFile,
    state: FrameworkCopyState,
) -> FrameworkCopy {
    FrameworkCopy {
        file_id: file.id,
        file_name: file.file_name.clone(),
        file_size: file.file_size,
        md5: file.md5.clone(),
        state,
    }
}

fn add_framework_namespaces(record: &mut FrameworkRecord, namespaces: &[String]) {
    for namespace in namespaces {
        if !record.namespaces.contains(namespace) {
            record.namespaces.push(namespace.clone());
        }
    }
}

fn framework_outcome_from_checksum_and_names(
    record: &FrameworkRecord,
    file: &gamebanana::ModFile,
) -> FrameworkArrivalOutcome {
    if file
        .md5
        .as_deref()
        .is_some_and(|md5| record.judged_checksum(md5))
    {
        return FrameworkArrivalOutcome::Skip(FrameworkCopyState::NotNewer);
    }
    let Some((_, installed)) = record.installed_copy() else {
        return FrameworkArrivalOutcome::Skip(FrameworkCopyState::Undecided);
    };
    match frameworks::arriving_file_name_is_newer(&installed.file_name, &file.file_name) {
        Some(true) => FrameworkArrivalOutcome::Swap,
        Some(false) => FrameworkArrivalOutcome::Skip(FrameworkCopyState::NotNewer),
        None => FrameworkArrivalOutcome::Skip(FrameworkCopyState::AskThalia),
    }
}

fn remove_covered_files_from_file_set(
    file_set: &mut FileSetRecipe,
    covered: &HashSet<u64>,
) -> bool {
    let kept: Vec<usize> = file_set
        .selected_file_ids
        .iter()
        .enumerate()
        .filter(|(_, file_id)| !covered.contains(file_id))
        .map(|(index, _)| index)
        .collect();
    if kept.is_empty() || kept.len() == file_set.selected_file_ids.len() {
        return false;
    }
    let names_follow_ids = file_set.selected_file_names.len() == file_set.selected_file_ids.len();
    if names_follow_ids {
        file_set.selected_file_names = kept
            .iter()
            .map(|index| file_set.selected_file_names[*index].clone())
            .collect();
    }
    file_set.selected_file_ids = kept
        .iter()
        .map(|index| file_set.selected_file_ids[*index])
        .collect();
    file_set
        .selected_files_meta
        .retain(|meta| !covered.contains(&meta.file_id));
    true
}

fn usable_framework_folder_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let name = sanitize_folder_name(trimmed);
    importing::validate_install_folder_name(&name)
        .is_ok()
        .then_some(name)
}

impl HestiaApp {
    fn framework_install_context(&self, game_id: &str) -> Option<Arc<frameworks::InstallContext>> {
        let game = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)?;
        if !game.is_xxmi() {
            return None;
        }
        let installed = self
            .state
            .mods
            .iter()
            .filter(|entry| entry.game_id == game_id && entry.status != ModStatus::Archived)
            .map(framework_installed_folder)
            .collect();
        Some(Arc::new(frameworks::InstallContext { installed }))
    }

    fn framework_review_arrived(
        &mut self,
        pending: PendingImport,
        scans: Vec<frameworks::CandidateScan>,
    ) {
        let job_id = pending.job_id;
        self.frameworks.inspected.insert(job_id);
        let Some(game_id) = self
            .install_inflight
            .get(&job_id)
            .map(|job| job.game_id.clone())
        else {
            return;
        };
        let own_download = self.frameworks.downloads.contains_key(&job_id);
        if scans.is_empty() || scans.len() != pending.inspection.candidates.len() {
            if own_download {
                self.finish_framework_download_without_copy(job_id);
            } else {
                self.review_pending_import(pending);
            }
            return;
        }
        let page = self
            .pending_browse_install_meta
            .get(&job_id)
            .map(|meta| (meta.mod_id, meta.is_tool));
        let calls = self
            .frameworks
            .session_calls
            .entry(game_id.clone())
            .or_default();
        for scan in scans.iter().filter(|scan| !scan.framework_shaped()) {
            calls.extend(scan.facts.called_but_not_declared());
        }
        self.frameworks.batch_jobs.insert(
            job_id,
            FrameworkBatchJob {
                game_id,
                page,
                has_models: scans
                    .iter()
                    .any(|scan| scan.facts.has_models || scan.facts.has_textures),
            },
        );
        if own_download {
            self.resolve_framework_download(pending, scans);
            return;
        }
        let needs_review = scans
            .iter()
            .any(|scan| scan.framework_shaped() || !scan.movable_parts.is_empty());
        if !needs_review {
            self.review_pending_import(pending);
            return;
        }
        self.frameworks.reviews.insert(
            job_id,
            FrameworkReview {
                pending,
                scans,
                user_verdict: None,
            },
        );
        self.advance_framework_reviews();
    }

    fn advance_frameworks(&mut self) {
        self.advance_framework_reviews();
        self.advance_waiting_framework_arrivals();
        self.forget_finished_framework_batch();
        self.advance_framework_updates();
    }

    fn advance_framework_reviews(&mut self) {
        if self.frameworks.reviews.is_empty() {
            return;
        }
        let mut job_ids: Vec<u64> = self.frameworks.reviews.keys().copied().collect();
        job_ids.sort_unstable();
        for job_id in job_ids {
            if !self.install_inflight.contains_key(&job_id) {
                self.frameworks.reviews.remove(&job_id);
                self.withdraw_framework_questions(job_id);
                continue;
            }
            let Some(review) = self.frameworks.reviews.get(&job_id) else {
                continue;
            };
            let mut verdicts = Vec::with_capacity(review.scans.len());
            let mut waiting = false;
            let mut thalia_question = None;
            let mut user_question = None;
            for index in 0..review.scans.len() {
                match self.framework_candidate_decision(review, index) {
                    FrameworkDecision::Verdict(verdict) => verdicts.push(verdict),
                    FrameworkDecision::Wait => waiting = true,
                    FrameworkDecision::AskThalia { page_id, file_id } => {
                        thalia_question = Some((page_id, file_id));
                    }
                    FrameworkDecision::AskUser => {
                        user_question = review
                            .pending
                            .inspection
                            .candidates
                            .get(index)
                            .map(|candidate| candidate.label.clone());
                    }
                }
            }
            let asked = self
                .frameworks
                .questions
                .iter()
                .any(|question| matches!(question, FrameworkQuestion::IsFramework { job_id: asked, .. } if *asked == job_id));
            match user_question {
                Some(name) if !asked => {
                    self.frameworks
                        .questions
                        .push_back(FrameworkQuestion::IsFramework { job_id, name });
                }
                Some(_) => {}
                None if asked => self.withdraw_framework_questions(job_id),
                None => {}
            }
            if let Some(key) = thalia_question
                && self.frameworks.classify_inflight.insert(key)
                && self
                    .frameworks
                    .request_tx
                    .send(FrameworkRequest::Classify {
                        page_id: key.0,
                        file_id: key.1,
                    })
                    .is_err()
            {
                self.frameworks.classify_inflight.remove(&key);
                self.frameworks.classify_answers.insert(
                    key,
                    FrameworkClassifyAnswer {
                        file_type: None,
                        name: None,
                    },
                );
            }
            let undecided = waiting
                || thalia_question.is_some()
                || self
                    .frameworks
                    .questions
                    .iter()
                    .any(|question| question.job_id() == job_id);
            if undecided {
                continue;
            }
            if let Some(review) = self.frameworks.reviews.remove(&job_id) {
                self.resolve_framework_review(review, verdicts);
            }
        }
    }

    fn forget_finished_framework_batch(&mut self) {
        if !self.frameworks.downloads.is_empty() {
            let tasks = &self.state.tasks;
            self.frameworks.downloads.retain(|task_id, _| {
                tasks
                    .iter()
                    .any(|task| task.id == *task_id && !task.status.is_terminal())
            });
        }
        if self.frameworks.batch_memory_is_empty() {
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
            let inflight = &self.install_inflight;
            let finalizing = &self.pending_install_finalize;
            self.frameworks.arrivals.retain(|job_id, _| {
                inflight.contains_key(job_id) || finalizing.contains_key(job_id)
            });
            self.frameworks.own_page_arrivals.retain(|job_id, _| {
                inflight.contains_key(job_id) || finalizing.contains_key(job_id)
            });
            return;
        }
        self.frameworks.inspected.clear();
        self.frameworks.session_calls.clear();
        self.frameworks.batch_jobs.clear();
        self.frameworks.bundle_frameworks.clear();
        self.frameworks.arrivals.clear();
        self.frameworks.own_page_arrivals.clear();
    }

    fn framework_job_update_target(&self, job_id: u64) -> Option<Option<&ModEntry>> {
        let meta = self.pending_browse_install_meta.get(&job_id)?;
        if let Some(target_id) = meta.update_target_mod_id.as_deref() {
            return Some(self.state.mods.iter().find(|entry| entry.id == target_id));
        }
        let folder_name = meta.update_folder_name.as_deref()?;
        Some(
            self.state
                .mods
                .iter()
                .find(|entry| entry.game_id == meta.game_id && entry.folder_name == folder_name),
        )
    }

    fn framework_job_group(&self, job_id: u64) -> Option<(String, Option<(u64, bool)>)> {
        if let Some(meta) = self.pending_browse_install_meta.get(&job_id) {
            return Some((meta.game_id.clone(), Some((meta.mod_id, meta.is_tool))));
        }
        self.install_inflight
            .get(&job_id)
            .or_else(|| self.install_queue.iter().find(|job| job.id == job_id))
            .map(|job| (job.game_id.clone(), None))
    }

    fn framework_group_still_arriving(&self, job_id: u64) -> bool {
        let Some(group) = self.framework_job_group(job_id) else {
            return false;
        };
        let has_explicit_resolution = |id: &u64| {
            self.pending_browse_install_meta
                .get(id)
                .is_some_and(|meta| meta.explicit_resolution.is_some())
        };
        let held_behind_me = has_explicit_resolution(&job_id);
        let queued_installs = self
            .install_queue
            .iter()
            .map(|job| job.id)
            .filter(|id| !(held_behind_me && has_explicit_resolution(id)));
        let unprepared_bundle_members = self
            .thalia_install_bundles
            .iter()
            .filter(|bundle| bundle.prepared_root.is_none())
            .flat_map(|bundle| bundle.members.iter().copied());
        self.browse_download_queue
            .iter()
            .map(|job| job.task_id)
            .chain(self.browse_download_inflight.keys().copied())
            .chain(queued_installs)
            .chain(self.install_inflight.keys().copied())
            .chain(unprepared_bundle_members)
            .any(|other| {
                other != job_id
                    && !self.frameworks.inspected.contains(&other)
                    && self.framework_job_group(other).as_ref() == Some(&group)
            })
    }

    fn framework_group_installs_models(&self, job_id: u64) -> bool {
        let Some(mine) = self.frameworks.batch_jobs.get(&job_id) else {
            return false;
        };
        self.frameworks.batch_jobs.iter().any(|(other, batch_job)| {
            *other != job_id
                && batch_job.has_models
                && batch_job.game_id == mine.game_id
                && batch_job.page == mine.page
        })
    }

    fn framework_job_file(&self, job_id: u64) -> Option<FrameworkJobFile> {
        let meta = self.pending_browse_install_meta.get(&job_id)?;
        let is_tool = self
            .frameworks
            .downloads
            .get(&job_id)
            .map_or(meta.is_tool, |download| download.is_tool);
        let task_file = self
            .state
            .tasks
            .iter()
            .find(|task| task.id == job_id)
            .and_then(Self::browse_download_retry_payload_for_task)
            .map(|payload| &payload.file);
        let file = match task_file {
            Some(task_file) => meta
                .selected_files
                .iter()
                .find(|file| file.id == task_file.id)
                .cloned()
                .unwrap_or_else(|| Self::browse_download_mod_file_from_task_file(task_file)),
            None => match meta.selected_files.as_slice() {
                [only] => only.clone(),
                _ => return None,
            },
        };
        (file.id != 0).then_some(FrameworkJobFile {
            page_id: meta.mod_id,
            is_tool,
            file,
        })
    }

    fn framework_arrival_source(&self, job_id: u64) -> FrameworkArrivalSource {
        let job_file = self.framework_job_file(job_id);
        let meta = self.pending_browse_install_meta.get(&job_id);
        let page = job_file
            .as_ref()
            .map(|job_file| (job_file.page_id, job_file.is_tool))
            .or_else(|| meta.map(|meta| (meta.mod_id, meta.is_tool)));
        let thalia_name = job_file
            .as_ref()
            .and_then(|job_file| {
                self.frameworks
                    .classify_answers
                    .get(&(job_file.page_id, job_file.file.id))
            })
            .and_then(|answer| answer.name.clone());
        let bundle_job = meta.is_some()
            && matches!(
                self.install_inflight.get(&job_id).map(|job| &job.source),
                Some(ImportSource::Folder(_))
            );
        FrameworkArrivalSource {
            page,
            file: job_file.map(|job_file| job_file.file),
            embedded: false,
            thalia_name,
            bundle_job,
        }
    }

    fn framework_candidate_decision(
        &self,
        review: &FrameworkReview,
        index: usize,
    ) -> FrameworkDecision {
        let regular = FrameworkDecision::Verdict(FrameworkVerdict::Regular);
        let framework = FrameworkDecision::Verdict(FrameworkVerdict::Framework);
        let job_id = review.pending.job_id;
        let scan = &review.scans[index];
        if !scan.framework_shaped() {
            return regular;
        }
        let Some(job) = self.install_inflight.get(&job_id) else {
            return regular;
        };
        let declared = scan.facts.declared_keys();
        if let Some(target) = self.framework_job_update_target(job_id) {
            let target_is_this_mod = target.is_none_or(|target| {
                mod_known_folder_facts(target).is_none_or(|facts| facts.declares_any(&declared))
            });
            if target_is_this_mod {
                return regular;
            }
        }
        let called_by_sibling = review
            .scans
            .iter()
            .enumerate()
            .any(|(other, other_scan)| other != index && other_scan.facts.calls_any(&declared));
        let called_this_session = self
            .frameworks
            .session_calls
            .get(&job.game_id)
            .is_some_and(|calls| declared.iter().any(|key| calls.contains(key)));
        let has_recorded_copy = scan.installed_copies.iter().any(|copy| {
            self.state
                .mods
                .iter()
                .any(|entry| entry.id == copy.mod_id && mod_framework_record(entry).is_some())
        });
        if self.frameworks.bundle_frameworks.contains(&job_id)
            || !scan.callers.is_empty()
            || called_by_sibling
            || called_this_session
            || has_recorded_copy
        {
            return framework;
        }
        if self.framework_group_still_arriving(job_id) {
            return FrameworkDecision::Wait;
        }
        let installed_alone =
            review.scans.len() == 1 && !self.framework_group_installs_models(job_id);
        if !installed_alone {
            return regular;
        }
        if let Some(verdict) = review.user_verdict {
            return FrameworkDecision::Verdict(verdict);
        }
        let thalia_file = self
            .pending_browse_install_meta
            .get(&job_id)
            .filter(|meta| !meta.is_tool && thalia::THALIA_UPDATE_MATCHING_ENABLED)
            .and_then(|_| self.framework_job_file(job_id));
        if let Some(job_file) = thalia_file {
            let key = (job_file.page_id, job_file.file.id);
            match self.frameworks.classify_answers.get(&key) {
                None => {
                    return FrameworkDecision::AskThalia {
                        page_id: key.0,
                        file_id: key.1,
                    };
                }
                Some(answer) => match answer.file_type {
                    Some(thalia::UpdateMatchFileType::Framework) => return framework,
                    Some(thalia::UpdateMatchFileType::Regular) => return regular,
                    Some(thalia::UpdateMatchFileType::Uncertain) | None => {}
                },
            }
        }
        if self.is_game_overlay_install(job_id) {
            regular
        } else {
            FrameworkDecision::AskUser
        }
    }

    fn resolve_framework_review(
        &mut self,
        review: FrameworkReview,
        verdicts: Vec<FrameworkVerdict>,
    ) {
        let FrameworkReview { pending, scans, .. } = review;
        let job_id = pending.job_id;
        let framework_count = verdicts
            .iter()
            .filter(|verdict| **verdict == FrameworkVerdict::Framework)
            .count();
        let has_movable_parts = scans.iter().zip(&verdicts).any(|(scan, verdict)| {
            *verdict == FrameworkVerdict::Regular && !scan.movable_parts.is_empty()
        });
        if framework_count == 0 && !has_movable_parts {
            self.review_pending_import(pending);
            return;
        }
        if framework_count == scans.len() && self.framework_arrives_from_own_page(&pending, &scans)
        {
            let Some(game_id) = self
                .install_inflight
                .get(&job_id)
                .map(|job| job.game_id.clone())
            else {
                return;
            };
            let candidates = pending
                .inspection
                .candidates
                .iter()
                .zip(&scans)
                .map(|(candidate, scan)| {
                    (
                        candidate.label.clone(),
                        scan.facts.declared_keys().into_iter().collect(),
                    )
                })
                .collect();
            self.frameworks.own_page_arrivals.insert(
                job_id,
                FrameworkOwnPageArrival {
                    game_id,
                    candidates,
                },
            );
            if let Some(job) = self.install_inflight.get_mut(&job_id) {
                job.install_state = ModInstallState::Enabled;
            }
            if let Some(meta) = self.pending_browse_install_meta.get_mut(&job_id) {
                meta.install_state = ModInstallState::Enabled;
            }
            self.review_pending_import(pending);
            return;
        }
        let source = self.framework_arrival_source(job_id);
        if scans.len() == 1 && framework_count == 1 {
            if let Some(scan) = scans.into_iter().next() {
                self.decide_framework_arrival(pending, scan, source);
            }
            return;
        }
        self.start_framework_split(pending, scans, &verdicts, source, false);
    }

    fn framework_arrives_from_own_page(
        &self,
        pending: &PendingImport,
        scans: &[frameworks::CandidateScan],
    ) -> bool {
        let job_id = pending.job_id;
        if self.frameworks.downloads.contains_key(&job_id) {
            return false;
        }
        let Some(meta) = self.pending_browse_install_meta.get(&job_id) else {
            return false;
        };
        if meta.update_target_mod_id.is_some() || meta.update_folder_name.is_some() {
            return false;
        }
        let bundle_job = matches!(
            self.install_inflight.get(&job_id).map(|job| &job.source),
            Some(ImportSource::Folder(_))
        );
        let title = pending
            .gb_profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .filter(|title| !title.trim().is_empty())
            .or_else(|| self.browse_mod_title_for_install(meta.mod_id, None));
        let Some(title) = title else {
            return false;
        };
        let mut names: Vec<&str> = Vec::new();
        for scan in scans {
            names.extend(
                scan.facts
                    .declared
                    .iter()
                    .map(|declared| declared.written.as_str()),
            );
            if !bundle_job && let Some(name) = scan.inner_folder_name.as_deref() {
                names.push(name);
            }
        }
        if !frameworks::title_names_framework(&title, names) {
            return false;
        }
        let has_managed_copy = scans
            .iter()
            .flat_map(|scan| &scan.installed_copies)
            .any(|copy| {
                self.state
                    .mods
                    .iter()
                    .any(|entry| entry.id == copy.mod_id && mod_is_managed_framework(entry))
            });
        if has_managed_copy {
            return false;
        }
        let page = (meta.mod_id, meta.is_tool);
        let outfit_from_page = self.state.mods.iter().any(|entry| {
            entry.game_id == meta.game_id
                && entry
                    .source
                    .as_ref()
                    .and_then(|source| source.gamebanana.as_ref())
                    .is_some_and(|link| (link.mod_id, gamebanana::is_tool_url(&link.url)) == page)
                && mod_known_folder_facts(entry)
                    .is_none_or(|facts| facts.has_models || facts.has_textures)
        });
        if outfit_from_page {
            return false;
        }
        !self.frameworks.batch_jobs.iter().any(|(other, batch_job)| {
            *other != job_id
                && batch_job.has_models
                && batch_job.game_id == meta.game_id
                && batch_job.page == Some(page)
        })
    }

    fn resolve_framework_download(
        &mut self,
        pending: PendingImport,
        scans: Vec<frameworks::CandidateScan>,
    ) {
        let job_id = pending.job_id;
        let Some(download) = self.frameworks.downloads.get(&job_id).cloned() else {
            self.review_pending_import(pending);
            return;
        };
        let verdicts: Vec<FrameworkVerdict> = scans
            .iter()
            .map(|scan| {
                if scan.framework_shaped() && scan.facts.declares_any(&download.expected_namespaces)
                {
                    FrameworkVerdict::Framework
                } else {
                    FrameworkVerdict::Regular
                }
            })
            .collect();
        if !verdicts.contains(&FrameworkVerdict::Framework) {
            self.finish_framework_download_without_copy(job_id);
            return;
        }
        let source = self.framework_arrival_source(job_id);
        if scans.len() == 1 {
            if let Some(scan) = scans.into_iter().next() {
                self.decide_framework_arrival(pending, scan, source);
            }
            return;
        }
        self.start_framework_split(pending, scans, &verdicts, source, true);
    }

    fn finish_framework_download_without_copy(&mut self, job_id: u64) {
        if let Some(download) = self.frameworks.downloads.remove(&job_id) {
            match download.mod_id.as_deref() {
                Some(mod_id) => {
                    if let Some(entry) = self.state.mods.iter_mut().find(|entry| entry.id == mod_id)
                        && let Some(record) = entry
                            .source
                            .as_mut()
                            .and_then(|source| source.framework.as_mut())
                    {
                        record
                            .page_mut(download.page_id, download.is_tool)
                            .remember_other(download.file_id);
                        let _ = xxmi::save_mod_metadata(entry);
                    }
                }
                None => {
                    for namespace in &download.expected_namespaces {
                        self.frameworks.pages_without.insert((
                            download.game_id.clone(),
                            download.page_id,
                            namespace.clone(),
                        ));
                    }
                }
            }
        }
        self.frameworks.ladder_dirty = true;
        self.drop_install_job(job_id, TaskStatus::Completed);
    }

    fn start_framework_split(
        &mut self,
        pending: PendingImport,
        scans: Vec<frameworks::CandidateScan>,
        verdicts: &[FrameworkVerdict],
        source: FrameworkArrivalSource,
        parent_is_framework_download: bool,
    ) {
        let job_id = pending.job_id;
        let mut parts = Vec::new();
        let mut requested = Vec::new();
        for (candidate_index, (scan, verdict)) in scans.iter().zip(verdicts).enumerate() {
            if *verdict == FrameworkVerdict::Framework {
                requested.push(frameworks::SplitPart {
                    new_job_id: self.next_background_job_id(),
                    candidate_index,
                    relative_dir: None,
                });
                parts.push(FrameworkSplitPart {
                    candidate_index,
                    movable: None,
                });
                continue;
            }
            if parent_is_framework_download {
                continue;
            }
            for movable in &scan.movable_parts {
                requested.push(frameworks::SplitPart {
                    new_job_id: self.next_background_job_id(),
                    candidate_index,
                    relative_dir: Some(movable.relative_dir.clone()),
                });
                parts.push(FrameworkSplitPart {
                    candidate_index,
                    movable: Some(movable.clone()),
                });
            }
        }
        if requested.is_empty() {
            self.review_pending_import(pending);
            return;
        }
        if self
            .install_request_tx
            .send(InstallRequest::SplitFrameworkParts {
                job_id,
                parts: requested,
            })
            .is_err()
        {
            self.review_pending_import(pending);
            return;
        }
        self.frameworks.splits.insert(
            job_id,
            FrameworkSplit {
                pending,
                scans,
                parts,
                source,
                parent_is_framework_download,
            },
        );
    }

    fn framework_parts_split(
        &mut self,
        job_id: u64,
        inspection: ImportInspection,
        children: Vec<SplitFrameworkChild>,
    ) {
        let split = self.frameworks.splits.remove(&job_id);
        let game_id = self
            .install_inflight
            .get(&job_id)
            .map(|job| job.game_id.clone());
        let (Some(split), Some(game_id)) = (split, game_id) else {
            for child in &children {
                let _ = self.install_request_tx.send(InstallRequest::Drop {
                    job_id: child.job_id,
                });
            }
            return;
        };
        let parent_keeps_mods =
            !inspection.candidates.is_empty() && !split.parent_is_framework_download;
        for child in children {
            let Some(part) = split.parts.get(child.part_index) else {
                let _ = self.install_request_tx.send(InstallRequest::Drop {
                    job_id: child.job_id,
                });
                continue;
            };
            let scan = match &part.movable {
                Some(movable) => frameworks::CandidateScan {
                    facts: movable.facts.clone(),
                    inner_folder_name: Some(child.label.clone()),
                    ..Default::default()
                },
                None => {
                    let mut scan = split
                        .scans
                        .get(part.candidate_index)
                        .cloned()
                        .unwrap_or_default();
                    scan.movable_parts.clear();
                    scan
                }
            };
            let embedded = part.movable.is_some() || parent_keeps_mods;
            let mut source = split.source.clone();
            source.embedded = embedded;
            source.bundle_job = false;
            if embedded {
                source.file = None;
            }
            self.install_inflight.insert(
                child.job_id,
                InstallJob {
                    id: child.job_id,
                    game_id: game_id.clone(),
                    source: ImportSource::Folder(child.path.clone()),
                    title: Some(child.label.clone()),
                    reuse_existing_task: true,
                    install_state: ModInstallState::Enabled,
                    preserve_existing_state: false,
                    preserved_states: Vec::new(),
                    category_id: None,
                },
            );
            self.frameworks.inspected.insert(child.job_id);
            let child_pending = PendingImport {
                job_id: child.job_id,
                inspection: ImportInspection {
                    game_id: game_id.clone(),
                    candidates: vec![crate::model::ImportCandidate {
                        label: child.label,
                        path: child.path,
                    }],
                    notice: None,
                },
                gb_profile: None,
            };
            self.decide_framework_arrival(child_pending, scan, source);
        }
        if parent_keeps_mods {
            self.review_pending_import(PendingImport {
                job_id,
                inspection,
                gb_profile: split.pending.gb_profile,
            });
        } else {
            self.finish_framework_job_without_install(job_id, &[]);
        }
    }

    fn framework_installed_copies_now(
        &self,
        game_id: &str,
        arriving: &frameworks::FolderFacts,
    ) -> Vec<frameworks::InstalledCopy> {
        let declared = arriving.declared_keys();
        self.state
            .mods
            .iter()
            .filter(|entry| entry.game_id == game_id && entry.status != ModStatus::Archived)
            .filter_map(|entry| {
                let facts = mod_known_folder_facts(entry)?;
                (facts.framework_shaped() && facts.declares_any(&declared)).then(|| {
                    frameworks::InstalledCopy {
                        mod_id: entry.id.clone(),
                        inside_check: frameworks::inside_check(&facts, arriving),
                    }
                })
            })
            .collect()
    }

    fn framework_existing_copy(
        &self,
        game_id: &str,
        scan: &frameworks::CandidateScan,
    ) -> Option<(String, frameworks::InsideCheck)> {
        scan.installed_copies
            .iter()
            .filter(|copy| copy.inside_check != frameworks::InsideCheck::DifferentFramework)
            .filter_map(|copy| {
                let entry = self.state.mods.iter().find(|entry| {
                    entry.id == copy.mod_id
                        && entry.game_id == game_id
                        && entry.status != ModStatus::Archived
                })?;
                let rank = match mod_framework_record(entry) {
                    Some(record) if record.follows_outfit_pages() => 0,
                    Some(_) => 1,
                    None => 2,
                };
                Some((rank, copy))
            })
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, copy)| (copy.mod_id.clone(), copy.inside_check))
    }

    fn framework_facts_are_current(&mut self, game_id: &str) -> bool {
        if self.frameworks.warm_games.contains(game_id) {
            return true;
        }
        let requested = self
            .frameworks
            .open_facts_requests
            .get(game_id)
            .is_some_and(|open| *open > 0);
        if !requested {
            self.request_framework_warm_facts(game_id);
        }
        false
    }

    fn decide_framework_arrival(
        &mut self,
        pending: PendingImport,
        mut scan: frameworks::CandidateScan,
        source: FrameworkArrivalSource,
    ) {
        let job_id = pending.job_id;
        let Some(game_id) = self
            .install_inflight
            .get(&job_id)
            .map(|job| job.game_id.clone())
        else {
            return;
        };
        let declared = scan.facts.declared_keys();
        if !self.framework_facts_are_current(&game_id)
            || self.frameworks.namespaces_arriving(&game_id, &declared)
        {
            self.frameworks.waiting_arrivals.insert(
                job_id,
                FrameworkWaitingArrival {
                    pending,
                    scan,
                    source,
                },
            );
            return;
        }
        scan.installed_copies = self.framework_installed_copies_now(&game_id, &scan.facts);
        if let (Some((page_id, is_tool)), Some(file)) = (source.page, source.file.as_ref()) {
            self.frameworks
                .page_files
                .entry((game_id.clone(), page_id, is_tool))
                .or_default()
                .insert(file.id, file.clone());
        }
        let Some((existing_id, inside_check)) = self.framework_existing_copy(&game_id, &scan)
        else {
            self.start_fresh_framework_install(pending, &scan, source);
            return;
        };
        match self.framework_arrival_outcome(&existing_id, inside_check, &source) {
            FrameworkArrivalOutcome::Swap => {
                self.start_framework_swap(pending, &scan, source, &existing_id);
            }
            FrameworkArrivalOutcome::Skip(copy_state) => {
                self.skip_framework_arrival(job_id, &existing_id, &scan, &source, copy_state);
            }
            FrameworkArrivalOutcome::AskUser => {
                let name = self
                    .state
                    .mods
                    .iter()
                    .find(|entry| entry.id == existing_id)
                    .map(|entry| {
                        mod_framework_record(entry)
                            .map_or_else(|| entry.folder_name.clone(), |record| record.name.clone())
                    })
                    .unwrap_or_default();
                self.frameworks
                    .questions
                    .push_back(FrameworkQuestion::UseArrivingCopy { job_id, name });
                self.frameworks.copy_questions.insert(
                    job_id,
                    FrameworkCopyQuestion {
                        pending,
                        scan,
                        source,
                        existing_mod_id: existing_id,
                    },
                );
            }
        }
    }

    fn advance_waiting_framework_arrivals(&mut self) {
        if self.frameworks.waiting_arrivals.is_empty() {
            return;
        }
        let mut job_ids: Vec<u64> = self.frameworks.waiting_arrivals.keys().copied().collect();
        job_ids.sort_unstable();
        for job_id in job_ids {
            let Some(game_id) = self
                .install_inflight
                .get(&job_id)
                .map(|job| job.game_id.clone())
            else {
                self.frameworks.waiting_arrivals.remove(&job_id);
                continue;
            };
            let ready = self.framework_facts_are_current(&game_id)
                && self
                    .frameworks
                    .waiting_arrivals
                    .get(&job_id)
                    .is_some_and(|waiting| {
                        !self
                            .frameworks
                            .namespaces_arriving(&game_id, &waiting.scan.facts.declared_keys())
                    });
            if !ready {
                continue;
            }
            if let Some(waiting) = self.frameworks.waiting_arrivals.remove(&job_id) {
                self.decide_framework_arrival(waiting.pending, waiting.scan, waiting.source);
            }
        }
    }

    fn framework_arrival_outcome(
        &self,
        existing_id: &str,
        inside_check: frameworks::InsideCheck,
        source: &FrameworkArrivalSource,
    ) -> FrameworkArrivalOutcome {
        let not_newer = FrameworkArrivalOutcome::Skip(FrameworkCopyState::NotNewer);
        let Some(existing) = self.state.mods.iter().find(|entry| entry.id == existing_id) else {
            return not_newer;
        };
        let Some(record) =
            mod_framework_record(existing).filter(|record| record.follows_outfit_pages())
        else {
            return not_newer;
        };
        let held = existing
            .source
            .as_ref()
            .is_some_and(|source| source.ignore_update_always);
        if held {
            return FrameworkArrivalOutcome::Skip(FrameworkCopyState::Unjudged);
        }
        let judged_newer =
            source
                .page
                .zip(source.file.as_ref())
                .is_some_and(|((page_id, is_tool), file)| {
                    record.copy_state(page_id, is_tool, file.id) == Some(FrameworkCopyState::Newer)
                });
        match inside_check {
            frameworks::InsideCheck::Newer => FrameworkArrivalOutcome::Swap,
            frameworks::InsideCheck::SameOrOlder | frameworks::InsideCheck::DifferentFramework => {
                not_newer
            }
            frameworks::InsideCheck::CannotTell if source.embedded => not_newer,
            frameworks::InsideCheck::CannotTell if judged_newer => FrameworkArrivalOutcome::Swap,
            frameworks::InsideCheck::CannotTell => match (source.page, source.file.as_ref()) {
                (Some(_), Some(file)) => framework_outcome_from_checksum_and_names(record, file),
                (Some(_), None) => not_newer,
                (None, _) => FrameworkArrivalOutcome::AskUser,
            },
        }
    }

    fn framework_namespace_is_called(
        &self,
        game_id: &str,
        key: &str,
        scan: &frameworks::CandidateScan,
    ) -> bool {
        self.frameworks
            .session_calls
            .get(game_id)
            .is_some_and(|calls| calls.contains(key))
            || scan.callers.iter().any(|caller_id| {
                self.state
                    .mods
                    .iter()
                    .find(|entry| entry.id == *caller_id)
                    .and_then(mod_known_folder_facts)
                    .is_some_and(|facts| facts.calls(key))
            })
    }

    fn framework_folder_name(
        &self,
        game_id: &str,
        candidate_label: &str,
        scan: &frameworks::CandidateScan,
        source: &FrameworkArrivalSource,
    ) -> String {
        let inner_folder = scan
            .inner_folder_name
            .as_deref()
            .filter(|_| !source.bundle_job)
            .and_then(usable_framework_folder_name);
        if let Some(name) = inner_folder {
            return name;
        }
        let called_name = scan
            .facts
            .declared
            .iter()
            .find(|declared| self.framework_namespace_is_called(game_id, &declared.key, scan))
            .or_else(|| scan.facts.shallowest_declared())
            .and_then(|declared| usable_framework_folder_name(&declared.written));
        if let Some(name) = called_name {
            return name;
        }
        let from_file = source.file.as_ref().and_then(|file| {
            usable_framework_folder_name(&frameworks::file_name_without_site_ending(
                &file.file_name,
            ))
            .or_else(|| {
                file.description
                    .as_deref()
                    .and_then(usable_framework_folder_name)
            })
        });
        from_file
            .or_else(|| {
                source
                    .thalia_name
                    .as_deref()
                    .and_then(usable_framework_folder_name)
            })
            .or_else(|| usable_framework_folder_name(candidate_label))
            .unwrap_or_else(|| sanitize_folder_name(""))
    }

    fn start_fresh_framework_install(
        &mut self,
        pending: PendingImport,
        scan: &frameworks::CandidateScan,
        source: FrameworkArrivalSource,
    ) {
        let job_id = pending.job_id;
        let Some(game_id) = self
            .install_inflight
            .get(&job_id)
            .map(|job| job.game_id.clone())
        else {
            return;
        };
        let target_root = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .and_then(|game| game.mods_path(self.state.static_prefs.use_default_mods_path));
        let Some(target_root) = target_root else {
            self.drop_install_job(job_id, TaskStatus::Failed);
            return;
        };
        let candidate_label = pending
            .inspection
            .candidates
            .first()
            .map(|candidate| candidate.label.clone())
            .unwrap_or_default();
        let name = self.framework_folder_name(&game_id, &candidate_label, scan, &source);
        if let Some(job) = self.install_inflight.get_mut(&job_id) {
            job.install_state = ModInstallState::Enabled;
            job.preserve_existing_state = false;
            job.category_id = None;
        }
        self.frameworks.arrivals.insert(
            job_id,
            FrameworkArrival {
                game_id,
                name: name.clone(),
                namespaces: scan.facts.declared_keys().into_iter().collect(),
                page: source.page,
                file: source.file.filter(|_| !source.embedded),
                swap_target: None,
            },
        );
        self.pending_browse_install_meta.remove(&job_id);
        self.pending_browse_install_safety.remove(&job_id);
        self.commit_import(
            job_id,
            vec![0],
            ConflictChoice::KeepBoth,
            target_root,
            None,
            vec![name],
        );
    }

    fn start_framework_swap(
        &mut self,
        pending: PendingImport,
        scan: &frameworks::CandidateScan,
        source: FrameworkArrivalSource,
        existing_id: &str,
    ) {
        let job_id = pending.job_id;
        let existing = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == existing_id)
            .and_then(|entry| {
                let target_root = entry.root_path.parent()?.to_path_buf();
                let name = mod_framework_record(entry)
                    .map_or_else(|| entry.folder_name.clone(), |record| record.name.clone());
                Some((
                    target_root,
                    entry.folder_name.clone(),
                    entry.game_id.clone(),
                    name,
                ))
            });
        let Some((target_root, folder_name, game_id, name)) = existing else {
            self.drop_install_job(job_id, TaskStatus::Failed);
            return;
        };
        if let Some(job) = self.install_inflight.get_mut(&job_id) {
            job.install_state = ModInstallState::Enabled;
            job.preserve_existing_state = true;
            job.category_id = None;
        }
        self.frameworks.arrivals.insert(
            job_id,
            FrameworkArrival {
                game_id,
                name,
                namespaces: scan.facts.declared_keys().into_iter().collect(),
                page: source.page,
                file: source.file.filter(|_| !source.embedded),
                swap_target: Some(existing_id.to_string()),
            },
        );
        self.pending_browse_install_meta.remove(&job_id);
        self.pending_browse_install_safety.remove(&job_id);
        self.commit_import(
            job_id,
            vec![0],
            ConflictChoice::Replace,
            target_root,
            None,
            vec![folder_name],
        );
    }

    fn skip_framework_arrival(
        &mut self,
        job_id: u64,
        existing_id: &str,
        scan: &frameworks::CandidateScan,
        source: &FrameworkArrivalSource,
        copy_state: FrameworkCopyState,
    ) {
        let namespaces: Vec<String> = scan.facts.declared_keys().into_iter().collect();
        let mut skipped = None;
        if let Some(existing) = self
            .state
            .mods
            .iter_mut()
            .find(|entry| entry.id == existing_id)
        {
            let folder_name = existing.folder_name.clone();
            let linked = existing
                .source
                .as_ref()
                .is_some_and(|source| source.gamebanana.is_some());
            let record = existing
                .source
                .get_or_insert_with(ModSourceData::default)
                .framework
                .get_or_insert_with(|| FrameworkRecord {
                    name: folder_name,
                    own_page: linked,
                    left_as_found: !linked,
                    ..Default::default()
                });
            add_framework_namespaces(record, &namespaces);
            if let Some((page_id, is_tool)) = source.page {
                let page = record.page_mut(page_id, is_tool);
                page.gone = false;
                if !source.embedded
                    && let Some(file) = source.file.as_ref()
                {
                    let judged = page.copy(file.id).is_some_and(|copy| {
                        matches!(
                            copy.state,
                            FrameworkCopyState::Installed | FrameworkCopyState::NotNewer
                        )
                    });
                    if !judged {
                        page.remember(framework_copy_from_file(file, copy_state));
                    }
                    if matches!(copy_state, FrameworkCopyState::AskThalia) {
                        self.frameworks.thalia_ready_pages.remove(&(
                            existing_id.to_string(),
                            page_id,
                            is_tool,
                        ));
                    }
                }
            }
            skipped = Some((existing.game_id.clone(), record.name.clone()));
            let _ = xxmi::save_mod_metadata(existing);
        }
        let text = self.text();
        if let Some((game_id, name)) = skipped {
            self.log_action(text.framework_copy_skipped_action(), &name);
            self.drop_framework_copies_from_outfit_file_sets(&game_id);
            self.reconcile_cached_grouped_update_states(Some(&game_id));
        }
        self.frameworks.ladder_dirty = true;
        self.finish_framework_job_without_install(job_id, &[existing_id.to_string()]);
    }

    fn finish_framework_job_without_install(&mut self, job_id: u64, mod_ids: &[String]) {
        if let Some(bundle) = self
            .thalia_install_bundles
            .iter_mut()
            .find(|bundle| bundle.members.contains(&job_id))
        {
            bundle.finished.insert(job_id);
        }
        self.frameworks.downloads.remove(&job_id);
        self.finish_game_overlay_install(job_id, mod_ids);
        self.drop_install_job(job_id, TaskStatus::Completed);
    }

    fn finalize_install_after_refresh(
        &mut self,
        job_id: u64,
        payload: PendingInstallFinalize,
    ) -> Vec<String> {
        if let Some(arrival) = self.frameworks.arrivals.remove(&job_id) {
            return self.finalize_framework_install(job_id, payload, arrival);
        }
        let own_page = self.frameworks.own_page_arrivals.remove(&job_id);
        let candidate_labels = own_page
            .as_ref()
            .map(|_| payload.installed_candidate_labels.clone());
        let mod_ids = self.finalize_regular_install_after_refresh(job_id, payload);
        if let (Some(own_page), Some(candidate_labels)) = (own_page, candidate_labels) {
            self.record_own_page_frameworks(&mod_ids, &own_page, &candidate_labels);
        }
        let game_id = mod_ids.first().and_then(|mod_id| {
            self.state
                .mods
                .iter()
                .find(|entry| entry.id == *mod_id)
                .map(|entry| entry.game_id.clone())
        });
        if let Some(game_id) = game_id {
            self.drop_framework_copies_from_outfit_file_sets(&game_id);
        }
        mod_ids
    }

    fn finalize_framework_install(
        &mut self,
        job_id: u64,
        payload: PendingInstallFinalize,
        arrival: FrameworkArrival,
    ) -> Vec<String> {
        for path in &payload.installed_paths {
            self.pending_known_installed_paths.remove(path);
        }
        let swapped = arrival.swap_target.is_some();
        let now = Utc::now();
        let mut mod_ids = Vec::new();
        let mut landed_off = Vec::new();
        let mut installed_name = arrival.name.clone();
        for path in &payload.installed_paths {
            let Some(mod_entry) = self
                .state
                .mods
                .iter_mut()
                .find(|entry| Self::install_path_matches_mod_root(path, &entry.root_path))
            else {
                continue;
            };
            let content_mtime = mod_entry.content_mtime;
            let ini_hash = mod_entry.ini_hash.clone();
            if !swapped {
                mod_entry.source = None;
            }
            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
            source.gamebanana = None;
            source.snapshot = None;
            source.raw_profile_json = None;
            source.file_set = FileSetRecipe::default();
            source.baseline_content_mtime = content_mtime;
            source.baseline_ini_hash = ini_hash;
            source.accepted_local_changes = None;
            if arrival.page.is_some() {
                source.history.downloaded_at = Some(now);
            }
            if source.history.installed_at.is_none() {
                source.history.installed_at = Some(now);
            }
            if swapped {
                source.history.updated_at = Some(now);
            }
            let record = source.framework.get_or_insert_with(|| FrameworkRecord {
                name: arrival.name.clone(),
                ..Default::default()
            });
            record.own_page = false;
            record.left_as_found = false;
            add_framework_namespaces(record, &arrival.namespaces);
            match (arrival.page, arrival.file.as_ref()) {
                (Some((page_id, is_tool)), Some(file)) => {
                    record.set_installed(
                        page_id,
                        is_tool,
                        framework_copy_from_file(file, FrameworkCopyState::Installed),
                    );
                    record.page_mut(page_id, is_tool).gone = false;
                    record.came_from = Some(FrameworkOrigin { page_id, is_tool });
                }
                (Some((page_id, is_tool)), None) => {
                    record.retire_installed_copy();
                    record.page_mut(page_id, is_tool).gone = false;
                    record.came_from = Some(FrameworkOrigin { page_id, is_tool });
                }
                (None, _) => {
                    record.retire_installed_copy();
                    record.came_from = None;
                }
            }
            record.judge_newer_copies_again();
            installed_name = record.name.clone();
            mod_entry.unsafe_content_auto = false;
            mod_entry.unsafe_content_preference = UnsafeContentPreference::Auto;
            mod_entry.unsafe_content = false;
            if !swapped {
                mod_entry.metadata.user.category_id = None;
                if mod_entry.status == ModStatus::Disabled {
                    landed_off.push(mod_entry.id.clone());
                }
            }
            let _ = xxmi::save_mod_metadata(mod_entry);
            mod_ids.push(mod_entry.id.clone());
        }
        self.reconcile_cached_grouped_update_states(Some(&arrival.game_id));
        self.run_xxmi_persist_import_restore(&mod_ids);
        self.request_xxmi_reload_for_live_mod_ids(
            &mod_ids,
            if swapped {
                ReloadHotkeyTrigger::UpdatingMods
            } else {
                ReloadHotkeyTrigger::InstallingMods
            },
        );
        let text = self.text();
        let action = if swapped {
            text.framework_updated_action()
        } else {
            text.framework_installed_action()
        };
        self.log_action(action, &installed_name);
        self.frameworks.downloads.remove(&job_id);
        for mod_id in &mod_ids {
            self.forget_framework_fetch_attempts(mod_id);
        }
        self.finish_game_overlay_install(job_id, &mod_ids);
        self.drop_framework_copies_from_outfit_file_sets(&arrival.game_id);
        self.frameworks.ladder_dirty = true;
        self.save_state();
        for mod_id in landed_off {
            self.enable_or_restore_mod_by_id(&mod_id);
        }
        mod_ids
    }

    fn record_own_page_frameworks(
        &mut self,
        mod_ids: &[String],
        own_page: &FrameworkOwnPageArrival,
        candidate_labels: &[(PathBuf, String)],
    ) {
        let all_namespaces: Vec<String> = own_page
            .candidates
            .iter()
            .flat_map(|(_, namespaces)| namespaces.iter().cloned())
            .collect();
        for mod_id in mod_ids {
            let Some(mod_entry) = self.state.mods.iter_mut().find(|entry| entry.id == *mod_id)
            else {
                continue;
            };
            let label = candidate_labels
                .iter()
                .find(|(path, _)| Self::install_path_matches_mod_root(path, &mod_entry.root_path))
                .map(|(_, label)| label.as_str());
            let namespaces = label
                .and_then(|label| {
                    own_page
                        .candidates
                        .iter()
                        .find(|(candidate_label, _)| candidate_label == label)
                })
                .map_or(&all_namespaces, |(_, namespaces)| namespaces);
            let folder_name = mod_entry.folder_name.clone();
            let record = mod_entry
                .source
                .get_or_insert_with(ModSourceData::default)
                .framework
                .get_or_insert_with(|| FrameworkRecord {
                    name: folder_name,
                    own_page: true,
                    ..Default::default()
                });
            add_framework_namespaces(record, namespaces);
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        self.frameworks.ladder_dirty = true;
    }

    fn drop_framework_copies_from_outfit_file_sets(&mut self, game_id: &str) {
        let mut covered: HashMap<(u64, bool), HashSet<u64>> = HashMap::new();
        for entry in self
            .state
            .mods
            .iter()
            .filter(|entry| entry.game_id == game_id)
        {
            let Some(record) = mod_framework_record(entry) else {
                continue;
            };
            for page in &record.pages {
                covered
                    .entry((page.page_id, page.is_tool))
                    .or_default()
                    .extend(page.copies.iter().map(|copy| copy.file_id));
            }
        }
        if covered.values().all(HashSet::is_empty) {
            return;
        }
        for entry in self
            .state
            .mods
            .iter_mut()
            .filter(|entry| entry.game_id == game_id)
        {
            let Some(source) = entry.source.as_mut() else {
                continue;
            };
            if source.framework.is_some() {
                continue;
            }
            let Some(page) = source
                .gamebanana
                .as_ref()
                .map(|link| (link.mod_id, gamebanana::is_tool_url(&link.url)))
            else {
                continue;
            };
            let Some(page_covered) = covered.get(&page) else {
                continue;
            };
            if remove_covered_files_from_file_set(&mut source.file_set, page_covered) {
                let _ = xxmi::save_mod_metadata(entry);
            }
        }
    }

    fn consume_framework_events(&mut self) {
        while let Ok(event) = self.frameworks.event_rx.try_recv() {
            match event {
                FrameworkEvent::FactsReady {
                    game_id,
                    loader_namespaces,
                } => self.framework_facts_ready(game_id, loader_namespaces),
                FrameworkEvent::Classified {
                    page_id,
                    file_id,
                    answer,
                } => {
                    let key = (page_id, file_id);
                    self.frameworks.classify_inflight.remove(&key);
                    let (file_type, name) = answer
                        .map(|answer| (answer.file_type, answer.name))
                        .unwrap_or((None, None));
                    self.frameworks
                        .classify_answers
                        .insert(key, FrameworkClassifyAnswer { file_type, name });
                }
                FrameworkEvent::QuietFetched {
                    mod_id,
                    page_id,
                    is_tool,
                    file_id,
                    compared_ini_hash,
                    inside_check,
                } => self.framework_quiet_fetched(
                    &mod_id,
                    (page_id, is_tool),
                    file_id,
                    compared_ini_hash.as_deref(),
                    inside_check,
                ),
                FrameworkEvent::NewerAnswered {
                    mod_id,
                    page_id,
                    is_tool,
                    compared_file_id,
                    answer,
                } => self.framework_newer_answered(
                    &mod_id,
                    (page_id, is_tool),
                    compared_file_id,
                    answer,
                ),
                FrameworkEvent::MissingFound {
                    game_id,
                    mod_id,
                    page_id,
                    names,
                    found,
                } => self.framework_missing_found(&game_id, &mod_id, page_id, names, found),
            }
        }
    }

    fn request_framework_warm_facts(&mut self, game_id: &str) {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
        else {
            return;
        };
        if !game.is_xxmi() {
            return;
        }
        let use_default = self.state.static_prefs.use_default_mods_path;
        let request = FrameworkRequest::WarmFacts {
            game_id: game_id.to_string(),
            folders: self
                .state
                .mods
                .iter()
                .filter(|entry| entry.game_id == game_id && entry.status != ModStatus::Archived)
                .map(framework_installed_folder)
                .collect(),
            importer_root: xxmi_persist::importer_root_for(game, use_default),
            mods_root: game.mods_path(use_default),
        };
        if self.frameworks.request_tx.send(request).is_ok() {
            self.frameworks.warm_games.remove(game_id);
            *self
                .frameworks
                .open_facts_requests
                .entry(game_id.to_string())
                .or_insert(0) += 1;
        }
    }

    fn framework_facts_ready(
        &mut self,
        game_id: String,
        loader_namespaces: std::collections::BTreeSet<String>,
    ) {
        self.frameworks
            .loader_namespaces
            .insert(game_id.clone(), loader_namespaces);
        let open = self
            .frameworks
            .open_facts_requests
            .entry(game_id.clone())
            .or_insert(0);
        *open = open.saturating_sub(1);
        if *open > 0 {
            return;
        }
        self.frameworks.warm_games.insert(game_id.clone());
        self.list_framework_caller_pages(&game_id);
        self.note_switched_on_mods(&game_id);
        self.frameworks.ladder_dirty = true;
    }

    fn list_framework_caller_pages(&mut self, game_id: &str) {
        let callers: Vec<(u64, bool, Arc<frameworks::FolderFacts>)> = self
            .state
            .mods
            .iter()
            .filter(|entry| {
                entry.game_id == game_id
                    && entry.status != ModStatus::Archived
                    && mod_framework_record(entry).is_none()
            })
            .filter_map(|entry| {
                let link = entry.source.as_ref()?.gamebanana.as_ref()?;
                let facts = mod_known_folder_facts(entry)?;
                (link.mod_id != 0 && !facts.called.is_empty())
                    .then(|| (link.mod_id, gamebanana::is_tool_url(&link.url), facts))
            })
            .collect();
        if callers.is_empty() {
            return;
        }
        for entry in self
            .state
            .mods
            .iter_mut()
            .filter(|entry| entry.game_id == game_id && entry.status != ModStatus::Archived)
        {
            let Some(record) = entry
                .source
                .as_mut()
                .and_then(|source| source.framework.as_mut())
            else {
                continue;
            };
            if record.own_page {
                continue;
            }
            let mut listed_new_page = false;
            for (page_id, is_tool, facts) in &callers {
                let calls_it = record.namespaces.iter().any(|key| facts.calls(key));
                if calls_it && record.page(*page_id, *is_tool).is_none() {
                    record.page_mut(*page_id, *is_tool);
                    listed_new_page = true;
                }
            }
            if listed_new_page {
                let _ = xxmi::save_mod_metadata(entry);
            }
        }
    }

    fn note_switched_on_mods(&mut self, game_id: &str) {
        let active: HashSet<String> = self
            .state
            .mods
            .iter()
            .filter(|entry| entry.game_id == game_id && entry.status == ModStatus::Active)
            .map(|entry| entry.id.clone())
            .collect();
        let to_check =
            active_mods_not_checked_yet(self.frameworks.active_snapshot.get(game_id), &active);
        self.frameworks.pending_switch_ons.extend(
            to_check
                .into_iter()
                .map(|mod_id| (game_id.to_string(), mod_id)),
        );
        self.frameworks
            .active_snapshot
            .insert(game_id.to_string(), active);
    }

    fn recheck_active_mods_for_missing_frameworks(&mut self, game_id: &str) {
        self.frameworks.active_snapshot.remove(game_id);
        self.frameworks
            .pages_without
            .retain(|(without_game_id, _, _)| without_game_id != game_id);
    }

    fn framework_parked_job_count(&self) -> usize {
        self.frameworks.reviews.len()
            + self.frameworks.splits.len()
            + self.frameworks.waiting_arrivals.len()
            + self.frameworks.copy_questions.len()
    }

    fn cancel_parked_framework_job(&mut self, job_id: u64) -> bool {
        let reviewing = self.frameworks.reviews.remove(&job_id).is_some();
        let splitting = self.frameworks.splits.remove(&job_id).is_some();
        let waiting = self.frameworks.waiting_arrivals.remove(&job_id).is_some();
        let asking = self.frameworks.copy_questions.remove(&job_id).is_some();
        let parked = reviewing || splitting || waiting || asking;
        if parked {
            self.withdraw_framework_questions(job_id);
        }
        parked
    }

    fn withdraw_framework_questions(&mut self, job_id: u64) {
        self.frameworks
            .questions
            .retain(|question| question.job_id() != job_id);
    }

    fn answer_framework_question(&mut self, job_id: u64, verdict: FrameworkVerdict) {
        self.withdraw_framework_questions(job_id);
        if let Some(review) = self.frameworks.reviews.get_mut(&job_id) {
            review.user_verdict = Some(verdict);
        }
        self.advance_framework_reviews();
    }

    fn answer_framework_copy_question(&mut self, job_id: u64, use_arriving_copy: bool) {
        self.withdraw_framework_questions(job_id);
        let Some(question) = self.frameworks.copy_questions.remove(&job_id) else {
            return;
        };
        if use_arriving_copy {
            self.start_framework_swap(
                question.pending,
                &question.scan,
                question.source,
                &question.existing_mod_id,
            );
        } else {
            self.skip_framework_arrival(
                job_id,
                &question.existing_mod_id,
                &question.scan,
                &question.source,
                FrameworkCopyState::NotNewer,
            );
        }
    }
}
