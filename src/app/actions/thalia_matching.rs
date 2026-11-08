const THALIA_MATCH_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const THALIA_MATCH_FAILURE_BACKOFF: Duration = Duration::from_secs(30);
const THALIA_MATCH_QUEUE_LIMIT: usize = 128;
const THALIA_MATCH_CACHE_LIMIT: usize = 512;
const THALIA_MATCH_HTTP_CONCURRENCY: usize = 2;
const THALIA_MATCH_START_SPACING: Duration = Duration::from_secs(1);
const THALIA_MATCH_PER_MINUTE: usize = 50;
const THALIA_MATCH_MINUTE: Duration = Duration::from_secs(60);
const THALIA_MATCH_RATE_LIMIT_RETRIES: usize = 3;
const THALIA_MATCH_RATE_LIMIT_WAIT: Duration = Duration::from_secs(1);
const THALIA_MATCH_PENDING_HOLD: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThaliaRecommendationAction {
    Update,
    Merge,
    Current,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThaliaRecommendation {
    pub(crate) action: ThaliaRecommendationAction,
    pub(crate) id: Option<u64>,
    pub(crate) requires: Vec<thalia::UpdateMatchRequiredFile>,
}

struct ValidatedThaliaInstall {
    action: ThaliaRecommendationAction,
    file: Option<gamebanana::ModFile>,
    requires: Vec<(thalia::UpdateMatchRequiredAction, gamebanana::ModFile)>,
}

#[derive(Clone)]
struct ThaliaInstallPlan {
    context: ThaliaMatchContext,
    source_url: String,
    recommendation: ThaliaRecommendation,
}

impl ThaliaInstallPlan {
    fn matches_target(&self, target: &ModEntry, active_profile_id: Option<ProfileId>) -> bool {
        target.id == self.context.local_mod_id
            && target.game_id == self.context.game_id
            && target.root_path == self.context.root_path
            && active_profile_id == self.context.active_profile_id
            && target.source.as_ref().is_some_and(|source| {
                thalia_hash(&serde_json::to_string(&source.file_set).unwrap_or_default())
                    == self.context.file_set_fingerprint
                    && source.gamebanana.as_ref().is_some_and(|link| {
                        link.url == self.source_url && link.mod_id == self.context.source_mod_id
                    })
            })
    }
}

fn overlay_thalia_recommendation(entry: &mut CachedUpdateEvidence, answer: &ThaliaRecommendation) {
    if entry.state == ModUpdateState::UpdateAvailable {
        return;
    }
    if answer.action == ThaliaRecommendationAction::Current && !answer.requires.is_empty() {
        entry.newer_files.clear();
        let Some(profile) = entry.profile.as_ref() else {
            return;
        };
        let canonical = matcher_adapter::canonicalize_profile(profile);
        let files: Vec<_> = answer
            .requires
            .iter()
            .filter_map(|required| {
                canonical.active.iter().find(|file| {
                    file.id == required.id
                        && profile_file_is_active_downloadable(&canonical.profile, file)
                })
            })
            .map(|file| TrackedFileMeta {
                file_id: file.id,
                file_name: file.file_name.clone(),
                file_size: Some(file.file_size),
                date_added: file.date_added,
                version: file.version.clone(),
                archived: false,
                label: file.description.clone(),
            })
            .collect();
        if files.len() == answer.requires.len() {
            entry.signature = candidate_signature(files);
            entry.state = ModUpdateState::UpdateAvailable;
        }
        return;
    }
    if answer.action == ThaliaRecommendationAction::Current {
        entry.newer_files.clear();
        return;
    }
    if !matches!(
        answer.action,
        ThaliaRecommendationAction::Update | ThaliaRecommendationAction::Merge
    ) {
        return;
    }
    let Some(profile) = entry.profile.as_ref() else {
        return;
    };
    let canonical = matcher_adapter::canonicalize_profile(profile);
    let Some(file) = canonical.active.iter().find(|file| {
        Some(file.id) == answer.id && profile_file_is_active_downloadable(&canonical.profile, file)
    }) else {
        return;
    };
    entry.signature = candidate_signature(vec![TrackedFileMeta {
        file_id: file.id,
        file_name: file.file_name.clone(),
        file_size: Some(file.file_size),
        date_added: file.date_added,
        version: file.version.clone(),
        archived: false,
        label: file.description.clone(),
    }]);
    entry.state = ModUpdateState::UpdateAvailable;
}

fn thalia_recommendation_installs_files(answer: &ThaliaRecommendation) -> bool {
    match answer.action {
        ThaliaRecommendationAction::Update | ThaliaRecommendationAction::Merge => {
            answer.id.is_some()
        }
        ThaliaRecommendationAction::Current => !answer.requires.is_empty(),
        ThaliaRecommendationAction::Uncertain => false,
    }
}

fn automatic_thalia_update_ids(
    mods: &[ModEntry],
    recommendations: &HashMap<String, ThaliaRecommendation>,
    checked_mod_ids: &HashSet<String>,
    targets: ModStatusTargets,
) -> Vec<String> {
    let mut ids: Vec<_> = mods
        .iter()
        .filter(|entry| {
            checked_mod_ids.contains(&entry.id)
                && HestiaApp::status_target_enabled(&entry.status, targets)
                && recommendations
                    .get(&entry.id)
                    .is_some_and(thalia_recommendation_installs_files)
        })
        .map(|entry| entry.id.clone())
        .collect();
    ids.sort();
    ids
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ThaliaMatchContext {
    generation: u64,
    request_id: u64,
    local_mod_id: String,
    game_id: String,
    source_mod_id: u64,
    active_profile_id: Option<ProfileId>,
    root_path: PathBuf,
    file_set_fingerprint: u64,
    catalog_fingerprint: u64,
    peer_fingerprint: u64,
    policy_fingerprint: u64,
    installed_file_id: u64,
    candidate_file_ids: Vec<u64>,
    peer_local_claim_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ThaliaMatchKey {
    local_mod_id: String,
    game_id: String,
    source_mod_id: u64,
    active_profile_id: Option<ProfileId>,
    root_path: PathBuf,
    file_set_fingerprint: u64,
    catalog_fingerprint: u64,
    peer_fingerprint: u64,
    policy_fingerprint: u64,
    installed_file_id: u64,
    candidate_file_ids: Vec<u64>,
}

impl ThaliaMatchContext {
    fn key(&self) -> ThaliaMatchKey {
        ThaliaMatchKey {
            local_mod_id: self.local_mod_id.clone(),
            game_id: self.game_id.clone(),
            source_mod_id: self.source_mod_id,
            active_profile_id: self.active_profile_id,
            root_path: self.root_path.clone(),
            file_set_fingerprint: self.file_set_fingerprint,
            catalog_fingerprint: self.catalog_fingerprint,
            peer_fingerprint: self.peer_fingerprint,
            policy_fingerprint: self.policy_fingerprint,
            installed_file_id: self.installed_file_id,
            candidate_file_ids: self.candidate_file_ids.clone(),
        }
    }

    fn same_semantic_context(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.key() == other.key()
            && self.peer_local_claim_ids == other.peer_local_claim_ids
    }
}

struct ThaliaMatchRequest {
    context: ThaliaMatchContext,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
}

enum ThaliaMatchResult {
    Completed {
        context: ThaliaMatchContext,
        response: thalia::UpdateMatchResponse,
    },
    Failed {
        context: ThaliaMatchContext,
        retry_after: Option<Duration>,
        cancelled: bool,
        permanent: bool,
    },
}

struct ThaliaCachedRecommendation {
    response: thalia::UpdateMatchResponse,
    cached_at: Instant,
}

struct ThaliaInflightRequest {
    request_id: u64,
    context: ThaliaMatchContext,
    cancellation: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
}

pub(crate) struct ThaliaMatchingState {
    request_tx: WorkerTx<ThaliaMatchRequest>,
    result_rx: WorkerRx<ThaliaMatchResult>,
    generation: u64,
    next_request_id: u64,
    force_refresh_games: HashSet<String>,
    inflight: HashMap<ThaliaMatchKey, ThaliaInflightRequest>,
    detached: HashMap<ThaliaMatchKey, ThaliaInflightRequest>,
    cache: HashMap<ThaliaMatchKey, ThaliaCachedRecommendation>,
    failure_backoff: HashMap<ThaliaMatchKey, Instant>,
    rejected: HashMap<ThaliaMatchKey, Instant>,
    failed_at: HashMap<ThaliaMatchKey, Instant>,
    awaiting_games: HashMap<String, Instant>,
    recommendations: HashMap<String, ThaliaRecommendation>,
    claims: HashMap<String, (ThaliaMatchContext, ThaliaRecommendation)>,
    blocked_claims: HashSet<(String, u64, u64, u64, u64)>,
    deferred: Vec<(ThaliaMatchContext, thalia::UpdateMatchResponse)>,
}

impl ThaliaMatchingState {
    fn new(runtime_services: &RuntimeServices) -> Self {
        let (request_tx, request_rx) = worker_channel::<ThaliaMatchRequest>();
        let (result_tx, result_rx) = worker_channel::<ThaliaMatchResult>();
        spawn_thalia_matching_worker(runtime_services, request_rx, result_tx);
        Self {
            request_tx,
            result_rx,
            generation: 0,
            next_request_id: 1,
            force_refresh_games: HashSet::new(),
            inflight: HashMap::new(),
            detached: HashMap::new(),
            cache: HashMap::new(),
            failure_backoff: HashMap::new(),
            rejected: HashMap::new(),
            failed_at: HashMap::new(),
            awaiting_games: HashMap::new(),
            recommendations: HashMap::new(),
            claims: HashMap::new(),
            blocked_claims: HashSet::new(),
            deferred: Vec::new(),
        }
    }

    fn has_pending_results(&self) -> bool {
        !self.result_rx.is_empty()
    }

    fn begin_round(&mut self, game_ids: impl IntoIterator<Item = String>) {
        let now = Instant::now();
        for game_id in game_ids {
            self.awaiting_games.insert(game_id, now);
        }
    }

    fn end_round(&mut self, game_ids: &HashSet<String>) {
        self.awaiting_games
            .retain(|game_id, _| !game_ids.contains(game_id));
    }

    fn game_awaiting(&self, game_id: &str, now: Instant) -> bool {
        self.awaiting_games
            .get(game_id)
            .is_some_and(|started| now.duration_since(*started) < THALIA_MATCH_PENDING_HOLD)
    }

    fn next_hold_expiry(&self) -> Option<Duration> {
        let now = Instant::now();
        self.awaiting_games
            .values()
            .filter_map(|started| THALIA_MATCH_PENDING_HOLD.checked_sub(now.duration_since(*started)))
            .filter(|remaining| !remaining.is_zero())
            .min()
    }

    fn pending_mod_ids(&self, contexts: &[ThaliaMatchContext]) -> HashSet<String> {
        if !thalia::THALIA_UPDATE_MATCHING_ENABLED {
            return HashSet::new();
        }
        let now = Instant::now();
        contexts
            .iter()
            .filter(|context| {
                let key = context.key();
                if let Some(cached) = self.cache.get(&key) {
                    return response_is_actionable(&cached.response)
                        && validated_recommendation(context, &cached.response).is_some()
                        && self
                            .inflight
                            .values()
                            .any(|request| request_context_group_matches(request, context));
                }
                !self.failed_at.contains_key(&key)
                    && (self.inflight.contains_key(&key)
                        || self.game_awaiting(&context.game_id, now))
            })
            .map(|context| context.local_mod_id.clone())
            .collect()
    }

    fn invalidate(&mut self) {
        for request in self.detached.values() {
            request.cancellation.store(true, Ordering::Relaxed);
        }
        self.detached.clear();
        self.invalidate_keeping_sent(false);
    }

    fn invalidate_keeping_sent(&mut self, keep_sent: bool) {
        self.generation = self.generation.wrapping_add(1);
        for (key, request) in self.inflight.drain() {
            if keep_sent && request.started.load(Ordering::Relaxed) {
                self.detached.insert(key, request);
            } else {
                request.cancellation.store(true, Ordering::Relaxed);
            }
        }
        self.recommendations.clear();
        self.claims.clear();
        self.blocked_claims.clear();
        self.deferred.clear();
    }

    fn mark_force_refresh_for_games(&mut self, game_ids: impl IntoIterator<Item = String>) {
        self.force_refresh_games.extend(game_ids);
    }

    fn take_force_refresh_for_game(&mut self, game_id: &str) -> bool {
        self.force_refresh_games.remove(game_id)
    }

    fn request(&mut self, context: ThaliaMatchContext, force_refresh: bool) {
        self.request_with_gate(
            context,
            force_refresh,
            thalia::THALIA_UPDATE_MATCHING_ENABLED,
        );
    }

    fn request_with_gate(
        &mut self,
        mut context: ThaliaMatchContext,
        force_refresh: bool,
        enabled: bool,
    ) {
        if !enabled || context.installed_file_id == 0 || context.candidate_file_ids.is_empty() {
            return;
        }
        context.generation = self.generation;
        context.request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        let key = context.key();
        let now = Instant::now();
        self.failure_backoff.retain(|_, retry_at| *retry_at > now);
        self.recommendations.remove(&context.local_mod_id);
        self.claims.remove(&context.local_mod_id);
        if !force_refresh
            && self
                .cache
                .get(&key)
                .is_some_and(|cached| now.duration_since(cached.cached_at) < THALIA_MATCH_CACHE_TTL)
        {
            if let Some(cached) = self.cache.get(&key) {
                self.accept_cached_response(context.clone(), cached.response.clone());
            }
            return;
        }
        if self
            .failure_backoff
            .get(&key)
            .is_some_and(|retry_at| *retry_at > now)
        {
            return;
        }
        if self.rejected.contains_key(&key) {
            self.failed_at.entry(key).or_insert(now);
            return;
        }
        if self.inflight.contains_key(&key) || self.inflight.len() >= THALIA_MATCH_QUEUE_LIMIT {
            return;
        }
        if let Some(mut request) = self.detached.remove(&key) {
            context.request_id = request.request_id;
            request.context = context;
            self.inflight.insert(key, request);
            return;
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));
        self.inflight.insert(
            key.clone(),
            ThaliaInflightRequest {
                request_id: context.request_id,
                context: context.clone(),
                cancellation: Arc::clone(&cancellation),
                started: Arc::clone(&started),
            },
        );
        if self
            .request_tx
            .send(ThaliaMatchRequest {
                context,
                force_refresh,
                cancellation,
                started,
            })
            .is_err()
        {
            self.inflight.remove(&key);
        }
    }

    fn take_results(&mut self) -> Vec<ThaliaMatchResult> {
        let mut results = Vec::new();
        while let Ok(mut result) = self.result_rx.try_recv() {
            let context = match &mut result {
                ThaliaMatchResult::Completed { context, .. }
                | ThaliaMatchResult::Failed { context, .. } => context,
            };
            let key = context.key();
            if let Some(request) = self.inflight.get(&key)
                && request.request_id == context.request_id
            {
                *context = request.context.clone();
            } else if self
                .detached
                .get(&key)
                .is_some_and(|request| request.request_id == context.request_id)
            {
                self.detached.remove(&key);
                self.keep_detached_result(key, result);
                continue;
            }
            results.push(result);
        }
        results
    }

    fn keep_detached_result(&mut self, key: ThaliaMatchKey, result: ThaliaMatchResult) {
        match result {
            ThaliaMatchResult::Completed { context, response } => {
                if validated_recommendation(&context, &response).is_some() {
                    self.insert_cache(key, response);
                }
            }
            ThaliaMatchResult::Failed {
                retry_after,
                cancelled,
                permanent,
                ..
            } => {
                if !cancelled {
                    self.record_failure(&key, retry_after, permanent);
                }
            }
        }
    }

    fn deferred_contexts(&self) -> impl Iterator<Item = &ThaliaMatchContext> {
        self.deferred.iter().map(|(context, _)| context)
    }

    fn prune_claims_to_contexts(&mut self, fresh_contexts: &[ThaliaMatchContext]) {
        let valid_keys: HashSet<ThaliaMatchKey> = fresh_contexts
            .iter()
            .map(ThaliaMatchContext::key)
            .filter(|key| {
                self.cache
                    .get(key)
                    .is_some_and(|cached| cached.cached_at.elapsed() < THALIA_MATCH_CACHE_TTL)
            })
            .collect();
        self.claims.retain(|_, (claim_context, _)| {
            valid_keys.contains(&claim_context.key())
                && fresh_contexts
                    .iter()
                    .any(|fresh| fresh.same_semantic_context(claim_context))
        });
        self.recommendations
            .retain(|mod_id, _| self.claims.contains_key(mod_id));

        let mut valid_claims: HashMap<(String, u64, u64), usize> = HashMap::new();
        for (claim_context, recommendation) in self.claims.values() {
            if let Some(id) = recommendation.id {
                *valid_claims
                    .entry((
                        claim_context.game_id.clone(),
                        claim_context.source_mod_id,
                        id,
                    ))
                    .or_default() += 1;
            }
        }
        let valid_block_contexts: Vec<_> = self
            .claims
            .values()
            .map(|(claim_context, recommendation)| (claim_context, recommendation.id))
            .collect();
        self.blocked_claims
            .retain(|(game_id, source_mod_id, id, catalog, peer)| {
                valid_claims
                    .get(&(game_id.clone(), *source_mod_id, *id))
                    .is_some_and(|count| *count > 1)
                    && valid_block_contexts
                        .iter()
                        .any(|(claim_context, recommendation_id)| {
                            *recommendation_id == Some(*id)
                                && claim_context.game_id == *game_id
                                && claim_context.source_mod_id == *source_mod_id
                                && claim_context.catalog_fingerprint == *catalog
                                && claim_context.peer_fingerprint == *peer
                        })
            });
    }

    fn restore_cached_responses(&mut self, fresh_contexts: &[ThaliaMatchContext]) {
        let cached: Vec<_> = fresh_contexts
            .iter()
            .filter_map(|context| {
                self.cache
                    .get(&context.key())
                    .filter(|cached| cached.cached_at.elapsed() < THALIA_MATCH_CACHE_TTL)
                    .map(|cached| (context.clone(), cached.response.clone()))
            })
            .collect();
        for (context, response) in cached {
            if !self.recommendations.contains_key(&context.local_mod_id) {
                self.accept_cached_response(context, response);
            }
        }
    }

    fn apply_results(
        &mut self,
        results: Vec<ThaliaMatchResult>,
        mut still_current: impl FnMut(&ThaliaMatchContext) -> bool,
    ) {
        let mut responses = std::mem::take(&mut self.deferred);
        for result in results {
            let context = match &result {
                ThaliaMatchResult::Completed { context, .. }
                | ThaliaMatchResult::Failed { context, .. } => context,
            };
            let key = context.key();
            let matched = self
                .inflight
                .get(&key)
                .is_some_and(|request| request.request_id == context.request_id);
            if matched {
                self.inflight.remove(&key);
            } else {
                continue;
            }
            match result {
                ThaliaMatchResult::Completed { context, response } => {
                    if context.generation != self.generation || !still_current(&context) {
                        continue;
                    }
                    if validated_recommendation(&context, &response).is_none() {
                        self.record_failure(&key, None, false);
                        continue;
                    }
                    self.insert_cache(key, response.clone());
                    responses.push((context, response));
                }
                ThaliaMatchResult::Failed {
                    context,
                    retry_after,
                    cancelled,
                    permanent,
                } => {
                    if cancelled || context.generation != self.generation {
                        continue;
                    }
                    self.record_failure(&context.key(), retry_after, permanent);
                }
            }
        }
        for (context, response) in responses {
            if context.generation != self.generation || !still_current(&context) {
                continue;
            }
            if response_is_actionable(&response)
                && self
                    .inflight
                    .values()
                    .any(|request| request_context_group_matches(request, &context))
            {
                self.deferred.push((context, response));
                continue;
            }
            self.accept_response(context, response);
        }
    }

    fn accept_cached_response(
        &mut self,
        context: ThaliaMatchContext,
        response: thalia::UpdateMatchResponse,
    ) {
        if validated_recommendation(&context, &response).is_none() {
            self.record_failure(&context.key(), None, false);
            return;
        }
        if response_is_actionable(&response)
            && self
                .inflight
                .values()
                .any(|request| request_context_group_matches(request, &context))
        {
            self.deferred.push((context, response));
        } else {
            self.accept_response(context, response);
        }
    }

    fn accept_response(
        &mut self,
        context: ThaliaMatchContext,
        response: thalia::UpdateMatchResponse,
    ) {
        let Some(recommendation) = validated_recommendation(&context, &response) else {
            self.record_failure(&context.key(), None, false);
            return;
        };
        if recommendation
            .id
            .is_some_and(|id| context.peer_local_claim_ids.contains(&id))
        {
            return;
        }
        if let Some(id) = recommendation.id {
            let group_key = claim_block_key(&context, id);
            if self.blocked_claims.contains(&group_key) {
                return;
            }
            let conflict_mod_ids: Vec<String> = self
                .claims
                .iter()
                .filter(|(mod_id, (existing_context, existing))| {
                    mod_id.as_str() != context.local_mod_id
                        && existing.id == Some(id)
                        && existing_context.game_id == context.game_id
                        && existing_context.source_mod_id == context.source_mod_id
                })
                .map(|(mod_id, _)| mod_id.clone())
                .collect();
            if !conflict_mod_ids.is_empty() {
                self.blocked_claims.insert(group_key);
                let mod_id = context.local_mod_id.clone();
                self.claims
                    .insert(mod_id, (context.clone(), recommendation.clone()));
                for mod_id in conflict_mod_ids {
                    self.recommendations.remove(&mod_id);
                }
                return;
            }
        }
        let mod_id = context.local_mod_id.clone();
        self.claims
            .insert(mod_id.clone(), (context, recommendation.clone()));
        self.recommendations.insert(mod_id.clone(), recommendation);
    }

    fn record_failure(
        &mut self,
        key: &ThaliaMatchKey,
        retry_after: Option<Duration>,
        permanent: bool,
    ) {
        let delay = retry_after
            .unwrap_or(THALIA_MATCH_FAILURE_BACKOFF)
            .max(THALIA_MATCH_FAILURE_BACKOFF);
        let now = Instant::now();
        if permanent {
            self.rejected.insert(key.clone(), now);
            if self.rejected.len() > THALIA_MATCH_CACHE_LIMIT {
                let oldest = self
                    .rejected
                    .iter()
                    .min_by_key(|(_, rejected_at)| **rejected_at)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    self.rejected.remove(&oldest);
                }
            }
        }
        self.failure_backoff.insert(key.clone(), now + delay);
        self.failed_at.insert(key.clone(), now);
        if self.failed_at.len() > THALIA_MATCH_CACHE_LIMIT {
            let oldest = self
                .failed_at
                .iter()
                .min_by_key(|(_, failed_at)| **failed_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.failed_at.remove(&oldest);
            }
        }
        if self.failure_backoff.len() > THALIA_MATCH_CACHE_LIMIT {
            let oldest = self
                .failure_backoff
                .iter()
                .min_by_key(|(_, retry_at)| **retry_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.failure_backoff.remove(&oldest);
            }
        }
    }

    fn insert_cache(&mut self, key: ThaliaMatchKey, response: thalia::UpdateMatchResponse) {
        if self.cache.len() >= THALIA_MATCH_CACHE_LIMIT && !self.cache.contains_key(&key) {
            let oldest = self
                .cache
                .iter()
                .min_by_key(|(_, cached)| cached.cached_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.cache.remove(&oldest);
            }
        }
        self.failed_at.remove(&key);
        self.cache.insert(
            key,
            ThaliaCachedRecommendation {
                response,
                cached_at: Instant::now(),
            },
        );
    }

    #[allow(dead_code)]
    fn recommendation_for(&self, mod_entry_id: &str) -> Option<ThaliaRecommendation> {
        self.recommendations
            .get(mod_entry_id)
            .and_then(|recommendation| {
                let (claim_context, _) = self.claims.get(mod_entry_id)?;
                if recommendation.id.is_some()
                    && self
                        .inflight
                        .values()
                        .any(|request| request_context_group_matches(request, claim_context))
                {
                    return None;
                }
                self.cache
                    .get(&claim_context.key())
                    .is_some_and(|cached| cached.cached_at.elapsed() < THALIA_MATCH_CACHE_TTL)
                    .then_some(recommendation.clone())
            })
    }

    #[allow(dead_code)]
    fn recommendation_for_context(
        &self,
        mod_entry_id: &str,
        context: &ThaliaMatchContext,
    ) -> Option<ThaliaRecommendation> {
        let recommendation = self.recommendations.get(mod_entry_id)?;
        let (claim_context, _) = self.claims.get(mod_entry_id)?;
        if recommendation.id.is_some()
            && self
                .inflight
                .values()
                .any(|request| request_context_group_matches(request, claim_context))
        {
            return None;
        }
        (self
            .cache
            .get(&claim_context.key())
            .is_some_and(|cached| cached.cached_at.elapsed() < THALIA_MATCH_CACHE_TTL)
            && claim_context.same_semantic_context(context))
        .then_some(recommendation.clone())
    }

    fn current_recommendations(
        &self,
        contexts: &[ThaliaMatchContext],
    ) -> HashMap<String, ThaliaRecommendation> {
        self.recommendations_within(contexts, None)
    }

    fn fresh_recommendations(
        &self,
        contexts: &[ThaliaMatchContext],
    ) -> HashMap<String, ThaliaRecommendation> {
        self.recommendations_within(contexts, Some(THALIA_MATCH_CACHE_TTL))
    }

    fn recommendations_within(
        &self,
        contexts: &[ThaliaMatchContext],
        max_age: Option<Duration>,
    ) -> HashMap<String, ThaliaRecommendation> {
        let mut answers = Vec::new();
        let mut claims: HashMap<(String, u64, u64), usize> = HashMap::new();
        for context in contexts {
            let Some(cached) = self.cache.get(&context.key()) else {
                continue;
            };
            if max_age.is_some_and(|max_age| cached.cached_at.elapsed() >= max_age) {
                continue;
            }
            let Some(answer) = validated_recommendation(context, &cached.response) else {
                continue;
            };
            if let Some(id) = answer.id {
                if self
                    .inflight
                    .values()
                    .any(|request| request_context_group_matches(request, context))
                {
                    continue;
                }
                *claims
                    .entry((context.game_id.clone(), context.source_mod_id, id))
                    .or_default() += 1;
            }
            answers.push((context, answer));
        }
        answers
            .into_iter()
            .filter_map(|(context, answer)| {
                if answer.id.is_some_and(|id| {
                    claims.get(&(context.game_id.clone(), context.source_mod_id, id)) != Some(&1)
                }) {
                    return None;
                }
                Some((context.local_mod_id.clone(), answer))
            })
            .collect()
    }
}

fn spawn_thalia_matching_worker(
    runtime_services: &RuntimeServices,
    mut request_rx: WorkerRx<ThaliaMatchRequest>,
    result_tx: WorkerTx<ThaliaMatchResult>,
) {
    let runtime_services = runtime_services.clone();
    let worker_services = runtime_services.clone();
    runtime_services.spawn(async move {
        let active_jobs = Arc::new(Semaphore::new(THALIA_MATCH_HTTP_CONCURRENCY));
        let pacer = Arc::new(Mutex::new(ThaliaPacer::default()));
        while let Some(request) = request_rx.recv().await {
            let permit = match Arc::clone(&active_jobs).acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let worker_services = worker_services.clone();
            let result_tx = result_tx.clone();
            let pacer = Arc::clone(&pacer);
            tokio::spawn(async move {
                let _permit = permit;
                let result = match worker_services
                    .async_client_builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                {
                    Ok(client) => {
                        fetch_paced_update_match(&pacer, &request, || {
                            thalia::fetch_update_match(
                                &client,
                                thalia::UpdateMatchRequest {
                                    mod_id: request.context.source_mod_id,
                                    installed_file_id: request.context.installed_file_id,
                                },
                                request.force_refresh,
                                Arc::clone(&request.cancellation),
                            )
                        })
                        .await
                    }
                    Err(error) => Err(thalia::UpdateMatchError::Transport(error.to_string())),
                };
                let event = match result {
                    Ok(response) => ThaliaMatchResult::Completed {
                        context: request.context,
                        response,
                    },
                    Err(error) => ThaliaMatchResult::Failed {
                        context: request.context,
                        retry_after: error.retry_after(),
                        cancelled: error.is_cancelled(),
                        permanent: error.is_permanent(),
                    },
                };
                let _ = result_tx.send(event);
            });
        }
    });
}

struct ThaliaPacer {
    starts: VecDeque<Instant>,
    paused_until: Option<Instant>,
    spacing: Duration,
    per_minute: usize,
    rate_limit_wait: Duration,
}

impl Default for ThaliaPacer {
    fn default() -> Self {
        Self {
            starts: VecDeque::new(),
            paused_until: None,
            spacing: THALIA_MATCH_START_SPACING,
            per_minute: THALIA_MATCH_PER_MINUTE,
            rate_limit_wait: THALIA_MATCH_RATE_LIMIT_WAIT,
        }
    }
}

impl ThaliaPacer {
    fn next_start(&mut self, now: Instant) -> Instant {
        while self
            .starts
            .front()
            .is_some_and(|start| now.duration_since(*start) >= THALIA_MATCH_MINUTE)
        {
            self.starts.pop_front();
        }
        let mut at = now;
        if let Some(last) = self.starts.back() {
            at = at.max(*last + self.spacing);
        }
        if self.starts.len() >= self.per_minute {
            at = at.max(self.starts[self.starts.len() - self.per_minute] + THALIA_MATCH_MINUTE);
        }
        if let Some(paused_until) = self.paused_until {
            at = at.max(paused_until);
        }
        at
    }

    fn try_start(&mut self, now: Instant) -> Option<Duration> {
        let at = self.next_start(now);
        if at > now {
            return Some(at - now);
        }
        self.starts.push_back(now);
        None
    }

    fn pause(&mut self, now: Instant, retry_after: Option<Duration>) {
        let until = now
            + retry_after
                .unwrap_or(self.rate_limit_wait)
                .max(self.rate_limit_wait);
        self.paused_until = Some(self.paused_until.map_or(until, |paused| paused.max(until)));
    }
}

async fn fetch_paced_update_match<F, Fut>(
    pacer: &Mutex<ThaliaPacer>,
    request: &ThaliaMatchRequest,
    mut send: F,
) -> Result<thalia::UpdateMatchResponse, thalia::UpdateMatchError>
where
    F: FnMut() -> Fut,
    Fut:
        std::future::Future<Output = Result<thalia::UpdateMatchResponse, thalia::UpdateMatchError>>,
{
    let lock = || pacer.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut retries = 0;
    loop {
        loop {
            if request.cancellation.load(Ordering::Relaxed) {
                return Err(thalia::UpdateMatchError::Cancelled);
            }
            let Some(wait) = lock().try_start(Instant::now()) else {
                break;
            };
            tokio::time::sleep(wait.min(Duration::from_millis(250))).await;
        }
        request.started.store(true, Ordering::Relaxed);
        let result = send().await;
        match &result {
            Err(error) if error.is_rate_limited() => {
                lock().pause(Instant::now(), error.retry_after());
                if retries == THALIA_MATCH_RATE_LIMIT_RETRIES {
                    return result;
                }
                retries += 1;
            }
            _ => return result,
        }
    }
}

fn response_is_actionable(response: &thalia::UpdateMatchResponse) -> bool {
    match response.action {
        thalia::UpdateMatchAction::Update | thalia::UpdateMatchAction::Merge => true,
        thalia::UpdateMatchAction::Current => !response.requires.is_empty(),
        thalia::UpdateMatchAction::Uncertain => false,
    }
}

fn claim_block_key(context: &ThaliaMatchContext, id: u64) -> (String, u64, u64, u64, u64) {
    (
        context.game_id.clone(),
        context.source_mod_id,
        id,
        context.catalog_fingerprint,
        context.peer_fingerprint,
    )
}

fn request_context_group_matches(
    request: &ThaliaInflightRequest,
    context: &ThaliaMatchContext,
) -> bool {
    request.context.game_id == context.game_id
        && request.context.source_mod_id == context.source_mod_id
}

fn recommendation_from_response(response: &thalia::UpdateMatchResponse) -> ThaliaRecommendation {
    ThaliaRecommendation {
        action: match response.action {
            thalia::UpdateMatchAction::Update => ThaliaRecommendationAction::Update,
            thalia::UpdateMatchAction::Merge => ThaliaRecommendationAction::Merge,
            thalia::UpdateMatchAction::Current => ThaliaRecommendationAction::Current,
            thalia::UpdateMatchAction::Uncertain => ThaliaRecommendationAction::Uncertain,
        },
        id: response.id,
        requires: response.requires.clone(),
    }
}

fn validated_recommendation(
    context: &ThaliaMatchContext,
    response: &thalia::UpdateMatchResponse,
) -> Option<ThaliaRecommendation> {
    let recommendation = recommendation_from_response(response);
    match recommendation.action {
        ThaliaRecommendationAction::Update | ThaliaRecommendationAction::Merge => {
            let id = recommendation.id?;
            (id != context.installed_file_id
                && context.candidate_file_ids.contains(&id)
                && !context.peer_local_claim_ids.contains(&id)
                && required_files_are_valid(context, &recommendation))
            .then_some(recommendation)
        }
        ThaliaRecommendationAction::Current => (recommendation.id.is_none()
            && required_files_are_valid(context, &recommendation))
        .then_some(recommendation),
        ThaliaRecommendationAction::Uncertain => (recommendation.id.is_none()
            && recommendation.requires.is_empty())
        .then_some(recommendation),
    }
}

fn refused_thalia_answer(answer: &ThaliaRecommendation) -> RefusedThaliaAnswer {
    let action = match answer.action {
        ThaliaRecommendationAction::Update => "update",
        ThaliaRecommendationAction::Merge => "merge",
        ThaliaRecommendationAction::Current => "current",
        ThaliaRecommendationAction::Uncertain => "uncertain",
    };
    let mut requires: Vec<_> = answer
        .requires
        .iter()
        .map(|required| RefusedThaliaRequiredFile {
            action: match required.action {
                thalia::UpdateMatchRequiredAction::Merge => "merge",
                thalia::UpdateMatchRequiredAction::Separate => "separate",
            }
            .to_owned(),
            file: required.id,
        })
        .collect();
    requires.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.action.cmp(&right.action))
    });
    RefusedThaliaAnswer {
        action: action.to_owned(),
        file: answer.id,
        requires,
    }
}

fn required_files_are_valid(
    context: &ThaliaMatchContext,
    recommendation: &ThaliaRecommendation,
) -> bool {
    recommendation.requires.iter().all(|required| {
        required.id != context.installed_file_id
            && Some(required.id) != recommendation.id
            && (context.candidate_file_ids.contains(&required.id)
                || context.peer_local_claim_ids.contains(&required.id))
    })
}

fn persisted_single_installed_file_id(file_set: &FileSetRecipe) -> Option<u64> {
    let selected_count = file_set
        .selected_file_ids
        .len()
        .max(file_set.selected_file_names.len())
        .max(file_set.selected_files_meta.len());
    if selected_count != 1 || file_set.selected_files_meta.len() > 1 {
        return None;
    }
    let id_from_meta = file_set
        .selected_files_meta
        .first()
        .map(|file| file.file_id)
        .filter(|id| *id != 0);
    let id_from_recipe = file_set
        .selected_file_ids
        .first()
        .copied()
        .filter(|id| *id != 0);
    match (id_from_meta, id_from_recipe) {
        (Some(meta), Some(recipe)) if meta != recipe => None,
        (Some(id), _) | (_, Some(id)) => Some(id),
        (None, None) => None,
    }
}

fn historical_newer_candidate_ids(
    profile: &gamebanana::ProfileResponse,
    installed_file_id: u64,
    baseline: i64,
) -> Vec<u64> {
    let canonical = matcher_adapter::canonicalize_profile(profile);
    if gamebanana::is_unavailable(&canonical.profile) || !canonical.ambiguous_ids.is_empty() {
        return Vec::new();
    }
    let mut ids: Vec<u64> = canonical
        .active
        .iter()
        .filter(|file| {
            file.id != 0
                && file.id != installed_file_id
                && file.date_added > baseline
                && profile_file_is_active_downloadable(&canonical.profile, file)
        })
        .map(|file| file.id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn thalia_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn thalia_profile_fingerprint(profile: &gamebanana::ProfileResponse) -> u64 {
    thalia_hash(
        &serde_json::to_string(&(
            profile_catalog_fingerprint(profile, false),
            &profile.name,
            &profile.short_description,
            &profile.html_text,
        ))
        .unwrap_or_default(),
    )
}

impl HestiaApp {
    fn schedule_thalia_matching_for_games(&mut self, game_ids: &HashSet<String>) {
        let force_games: HashSet<String> = game_ids
            .iter()
            .filter(|game_id| self.thalia_matching.take_force_refresh_for_game(game_id))
            .cloned()
            .collect();
        self.schedule_thalia_matching_for_games_with_force(game_ids, &force_games);
    }

    fn schedule_thalia_matching_for_games_with_force(
        &mut self,
        game_ids: &HashSet<String>,
        force_games: &HashSet<String>,
    ) {
        if !thalia::THALIA_UPDATE_MATCHING_ENABLED {
            return;
        }
        self.thalia_matching.end_round(game_ids);
        let evidence = build_cached_update_evidence(&self.state.mods, None);
        let contexts: Vec<_> = self
            .state
            .mods
            .iter()
            .filter(|mod_entry| game_ids.contains(&mod_entry.game_id))
            .filter_map(|mod_entry| self.thalia_context_for_mod(mod_entry, &evidence))
            .collect();
        self.thalia_matching.prune_claims_to_contexts(&contexts);
        self.thalia_matching.restore_cached_responses(&contexts);
        let checked_mod_ids = contexts
            .iter()
            .map(|context| context.local_mod_id.clone())
            .collect();
        for context in contexts {
            self.thalia_matching
                .request(context.clone(), force_games.contains(&context.game_id));
        }
        self.queue_automatic_thalia_updates(&checked_mod_ids);
    }

    fn thalia_context_for_mod(
        &self,
        mod_entry: &ModEntry,
        evidence: &HashMap<String, CachedUpdateEvidence>,
    ) -> Option<ThaliaMatchContext> {
        self.thalia_context_for_mod_internal(mod_entry, evidence, false)
    }

    fn thalia_context_for_mod_internal(
        &self,
        mod_entry: &ModEntry,
        evidence: &HashMap<String, CachedUpdateEvidence>,
        allow_pending_install: bool,
    ) -> Option<ThaliaMatchContext> {
        let source = mod_entry.source.as_ref()?;
        let link = source.gamebanana.as_ref()?;
        if gamebanana::is_tool_url(&link.url)
            || !Self::status_target_enabled(
                &mod_entry.status,
                self.state.static_prefs.update_check_statuses,
            )
            || source.ignore_update_always
            || source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|signature| signature.prearmed_next_update)
            || mod_has_local_changes_for_update_check(mod_entry)
            || matches!(
                mod_entry.update_state,
                ModUpdateState::UpdateAvailable
                    | ModUpdateState::ModifiedLocally
                    | ModUpdateState::IgnoringUpdateAlways
                    | ModUpdateState::CheckSkipped
            )
            || self.folder_batch_game_busy(&mod_entry.game_id)
            || (!allow_pending_install && self.has_active_update_for_mod(mod_entry))
        {
            return None;
        }
        let current_evidence = evidence.get(&mod_entry.id)?;
        if current_evidence.state == ModUpdateState::UpdateAvailable {
            return None;
        }
        let profile = current_evidence.profile.as_ref()?;
        if gamebanana::is_unavailable(profile) || profile.id != link.mod_id {
            return None;
        }
        if !selector_shape_is_valid(&source.file_set) {
            return None;
        }
        let installed_file_id = persisted_single_installed_file_id(&source.file_set)?;
        let duplicate_source_count = self
            .state
            .mods
            .iter()
            .filter(|peer| {
                peer.game_id == mod_entry.game_id
                    && peer
                        .source
                        .as_ref()
                        .and_then(|source| source.gamebanana.as_ref())
                        .is_some_and(|peer_link| {
                            peer_link.mod_id == link.mod_id
                                && peer.source.as_ref().and_then(|source| {
                                    persisted_single_installed_file_id(&source.file_set)
                                }) == Some(installed_file_id)
                        })
            })
            .count();
        if duplicate_source_count > 1 {
            return None;
        }
        if (!current_evidence.installed_files.is_empty()
            && current_evidence.installed_files.len() != 1)
            || current_evidence
                .installed_files
                .first()
                .is_some_and(|file| file.file_id != installed_file_id)
        {
            return None;
        }
        let mut candidate_file_ids: Vec<u64> = current_evidence
            .newer_files
            .iter()
            .filter(|file| {
                file.id != 0
                    && !file.is_archived
                    && file
                        .download_url
                        .as_deref()
                        .is_some_and(|url| !url.trim().is_empty())
            })
            .map(|file| file.id)
            .collect();
        if candidate_file_ids.is_empty() && current_evidence.state == ModUpdateState::MissingSource
        {
            let baseline = source
                .file_set
                .selected_files_meta
                .iter()
                .find(|file| file.file_id == installed_file_id)
                .map(|file| file.date_added)
                .filter(|date| *date > 0)
                .or_else(|| {
                    source.snapshot.as_ref().and_then(|snapshot| {
                        snapshot
                            .files
                            .iter()
                            .find(|file| file.file_id == installed_file_id)
                            .map(|file| file.date_added)
                            .filter(|date| *date > 0)
                    })
                });
            if let Some(baseline) = baseline {
                candidate_file_ids =
                    historical_newer_candidate_ids(profile, installed_file_id, baseline);
            }
        }
        let mut dismissed: HashSet<u64> = source
            .dismissed_newer_file_ids
            .iter()
            .copied()
            .filter(|id| *id != 0)
            .collect();
        dismissed.extend(
            source
                .ignored_update_signature
                .iter()
                .flat_map(|signature| signature.files.iter().map(|file| file.file_id)),
        );
        candidate_file_ids.retain(|id| !dismissed.contains(id));
        if candidate_file_ids.is_empty() {
            return None;
        }
        candidate_file_ids.sort_unstable();
        candidate_file_ids.dedup();

        let mut peers = Vec::new();
        let mut peer_hasher = DefaultHasher::new();
        for peer in self
            .state
            .mods
            .iter()
            .filter(|peer| peer.game_id == mod_entry.game_id)
        {
            let Some(peer_source) = peer.source.as_ref() else {
                continue;
            };
            let Some(peer_link) = peer_source.gamebanana.as_ref() else {
                continue;
            };
            if peer_link.mod_id != link.mod_id {
                continue;
            }
            peer.id.hash(&mut peer_hasher);
            thalia_hash(&serde_json::to_string(&peer_source.file_set).unwrap_or_default())
                .hash(&mut peer_hasher);
            if let Some(peer_installed_id) =
                persisted_single_installed_file_id(&peer_source.file_set)
            {
                peers.push(peer_installed_id);
            }
            if evidence
                .get(&peer.id)
                .is_some_and(|peer_evidence| peer_evidence.state == ModUpdateState::UpdateAvailable)
            {
                peers.extend(
                    evidence
                        .get(&peer.id)
                        .into_iter()
                        .flat_map(|peer_evidence| {
                            peer_evidence
                                .signature
                                .as_ref()
                                .or(peer_evidence.prearmed_signature.as_ref())
                                .into_iter()
                                .flat_map(|signature| {
                                    signature.files.iter().filter_map(|file| {
                                        (file.file_id != 0).then_some(file.file_id)
                                    })
                                })
                        }),
                );
            }
        }
        peers.sort_unstable();
        peers.dedup();
        candidate_file_ids.retain(|id| peers.binary_search(id).is_err());
        if candidate_file_ids.is_empty() {
            return None;
        }
        Some(ThaliaMatchContext {
            generation: self.thalia_matching.generation,
            request_id: 0,
            local_mod_id: mod_entry.id.clone(),
            game_id: mod_entry.game_id.clone(),
            source_mod_id: link.mod_id,
            active_profile_id: self.active_profile_id(&mod_entry.game_id),
            root_path: mod_entry.root_path.clone(),
            file_set_fingerprint: thalia_hash(
                &serde_json::to_string(&source.file_set).unwrap_or_default(),
            ),
            catalog_fingerprint: thalia_profile_fingerprint(profile),
            peer_fingerprint: peer_hasher.finish(),
            policy_fingerprint: thalia_hash(&(
                format!("{:?}", mod_entry.status),
                format!("{:?}", mod_entry.update_state),
                source.ignore_update_always,
                source.ignored_update_signature.is_some(),
                mod_has_local_changes_for_update_check(mod_entry),
            )),
            installed_file_id,
            candidate_file_ids,
            peer_local_claim_ids: peers,
        })
    }

    fn consume_thalia_matching_results(&mut self) {
        let results = self.thalia_matching.take_results();
        if results.is_empty() {
            return;
        }
        let evidence = build_cached_update_evidence(&self.state.mods, None);
        let fresh_contexts: Vec<_> = self
            .state
            .mods
            .iter()
            .filter_map(|mod_entry| self.thalia_context_for_mod(mod_entry, &evidence))
            .collect();
        self.thalia_matching
            .prune_claims_to_contexts(&fresh_contexts);
        self.thalia_matching
            .restore_cached_responses(&fresh_contexts);
        let mut current_by_mod = HashMap::new();
        let mut contexts = Vec::new();
        for result in &results {
            let context = match result {
                ThaliaMatchResult::Completed { context, .. }
                | ThaliaMatchResult::Failed { context, .. } => context,
            };
            contexts.push(context.clone());
        }
        contexts.extend(self.thalia_matching.deferred_contexts().cloned());
        let checked_mod_ids = contexts
            .iter()
            .map(|context| context.local_mod_id.clone())
            .collect();
        for context in contexts {
            let current = self
                .state
                .mods
                .iter()
                .find(|mod_entry| mod_entry.id == context.local_mod_id)
                .and_then(|mod_entry| self.thalia_context_for_mod(mod_entry, &evidence))
                .is_some_and(|current| current.same_semantic_context(&context));
            current_by_mod.insert(context.local_mod_id.clone(), current);
        }
        self.thalia_matching.apply_results(results, |context| {
            current_by_mod
                .get(&context.local_mod_id)
                .copied()
                .unwrap_or(false)
        });
        self.queue_automatic_thalia_updates(&checked_mod_ids);
    }

    fn queue_automatic_thalia_updates(&mut self, checked_mod_ids: &HashSet<String>) {
        let evidence = build_cached_update_evidence(&self.state.mods, None);
        let contexts = self.thalia_display_contexts(&evidence);
        let recommendations = self.without_present_thalia_requires(
            &contexts,
            self.thalia_matching.fresh_recommendations(&contexts),
        );
        let ids = automatic_thalia_update_ids(
            &self.state.mods,
            &recommendations,
            checked_mod_ids,
            self.state.static_prefs.auto_update_statuses,
        );
        for mod_id in ids {
            self.queue_update_apply(&mod_id);
        }
    }

    fn thalia_display_contexts(
        &self,
        evidence: &HashMap<String, CachedUpdateEvidence>,
    ) -> Vec<ThaliaMatchContext> {
        self.state
            .mods
            .iter()
            .filter_map(|entry| self.thalia_context_for_mod_internal(entry, evidence, true))
            .collect()
    }

    fn thalia_recommendations_from_evidence(
        &self,
        evidence: &HashMap<String, CachedUpdateEvidence>,
    ) -> HashMap<String, ThaliaRecommendation> {
        let contexts = self.thalia_display_contexts(evidence);
        self.without_present_thalia_requires(
            &contexts,
            self.thalia_matching.current_recommendations(&contexts),
        )
    }

    fn without_present_thalia_requires(
        &self,
        contexts: &[ThaliaMatchContext],
        mut answers: HashMap<String, ThaliaRecommendation>,
    ) -> HashMap<String, ThaliaRecommendation> {
        let refused: HashMap<&str, &RefusedThaliaAnswer> = if answers.is_empty() {
            HashMap::new()
        } else {
            self.state
                .mods
                .iter()
                .filter_map(|entry| {
                    let refused = entry.source.as_ref()?.refused_thalia_answer.as_ref()?;
                    Some((entry.id.as_str(), refused))
                })
                .collect()
        };
        for (mod_id, answer) in &mut answers {
            let Some(context) = contexts
                .iter()
                .find(|context| context.local_mod_id == *mod_id)
            else {
                continue;
            };
            answer.requires.retain(|required| {
                !self.thalia_required_file_is_present(
                    &context.game_id,
                    context.source_mod_id,
                    required.id,
                )
            });
            if refused
                .get(mod_id.as_str())
                .is_some_and(|refused| **refused == refused_thalia_answer(answer))
            {
                *answer = ThaliaRecommendation {
                    action: ThaliaRecommendationAction::Uncertain,
                    id: None,
                    requires: Vec::new(),
                };
            }
        }
        answers
    }

    fn remember_refused_thalia_answer(&mut self, mod_id: &str, answer: &ThaliaRecommendation) {
        let Some(mod_index) = self.state.mods.iter().position(|entry| entry.id == mod_id) else {
            return;
        };
        let backend = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == self.state.mods[mod_index].game_id)
            .map(|game| game.definition.backend)
            .unwrap_or_default();
        let refused = Some(refused_thalia_answer(answer));
        let Some(source) = self.state.mods[mod_index].source.as_mut() else {
            return;
        };
        if source.refused_thalia_answer == refused {
            return;
        }
        source.refused_thalia_answer = refused;
        let save_result = match backend {
            GameBackend::Xxmi => xxmi::save_mod_metadata(&mut self.state.mods[mod_index]),
            GameBackend::UnrealEngine => {
                unrealengine::write_portable_metadata(&self.state.mods[mod_index])
            }
        };
        if let Err(err) = save_result {
            tracing::warn!("could not save the refused Thalia answer for {mod_id}: {err:#}");
        }
        self.library_card_cache.key = None;
        self.library_card_cache.rows = Arc::new(Vec::new());
        self.library_card_cache.evidence.clear();
        self.library_card_cache.evidence_key = None;
    }

    fn overlay_thalia_update_evidence(&self, evidence: &mut HashMap<String, CachedUpdateEvidence>) {
        let contexts = self.thalia_display_contexts(evidence);
        let answers = self.without_present_thalia_requires(
            &contexts,
            self.thalia_matching.current_recommendations(&contexts),
        );
        let pending = self.thalia_matching.pending_mod_ids(&contexts);
        for (mod_id, answer) in answers {
            let Some(entry) = evidence.get_mut(&mod_id) else {
                continue;
            };
            overlay_thalia_recommendation(entry, &answer);
        }
        for mod_id in pending {
            let Some(entry) = evidence.get_mut(&mod_id) else {
                continue;
            };
            if entry.state != ModUpdateState::UpdateAvailable {
                entry.newer_files.clear();
                entry.thalia_pending = true;
            }
        }
    }

    fn thalia_evidence_fingerprint(&self) -> u64 {
        let mut cache: Vec<_> = self
            .thalia_matching
            .cache
            .iter()
            .map(|(key, value)| (thalia_hash(key), thalia_hash(&value.cached_at)))
            .collect();
        cache.sort_unstable();
        let mut failed: Vec<_> = self
            .thalia_matching
            .failed_at
            .keys()
            .map(thalia_hash)
            .collect();
        failed.sort_unstable();
        let now = Instant::now();
        let mut awaiting: Vec<_> = self
            .thalia_matching
            .awaiting_games
            .keys()
            .filter(|game_id| self.thalia_matching.game_awaiting(game_id, now))
            .cloned()
            .collect();
        awaiting.sort_unstable();
        let mut pending: Vec<_> = self
            .thalia_matching
            .inflight
            .keys()
            .map(thalia_hash)
            .collect();
        pending.sort_unstable();
        thalia_hash(&(self.thalia_matching.generation, cache, pending, failed, awaiting))
    }

    fn thalia_update_install_plan(&self, mod_entry_id: &str) -> Option<ThaliaInstallPlan> {
        let evidence = build_cached_update_evidence(&self.state.mods, None);
        let recommendation = self
            .thalia_recommendations_from_evidence(&evidence)
            .remove(mod_entry_id)?;
        if !thalia_recommendation_installs_files(&recommendation) {
            return None;
        }
        let target = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_entry_id)?;
        Some(ThaliaInstallPlan {
            context: self.thalia_context_for_mod(target, &evidence)?,
            source_url: target.source.as_ref()?.gamebanana.as_ref()?.url.clone(),
            recommendation,
        })
    }

    fn thalia_evidence_for_profile(
        &self,
        mod_entry_id: &str,
        profile: &gamebanana::ProfileResponse,
    ) -> Option<HashMap<String, CachedUpdateEvidence>> {
        let target = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_entry_id)?;
        let link = target.source.as_ref()?.gamebanana.as_ref()?;
        if link.mod_id != profile.id || gamebanana::is_unavailable(profile) {
            return None;
        }
        let mut mods = self.state.mods.clone();
        let raw = serde_json::to_string(profile).ok()?;
        for entry in mods
            .iter_mut()
            .filter(|entry| entry.game_id == target.game_id)
        {
            if let Some(source) = entry.source.as_mut().filter(|source| {
                source.gamebanana.as_ref().is_some_and(|peer| {
                    peer.mod_id == link.mod_id && !gamebanana::is_tool_url(&peer.url)
                })
            }) {
                source.raw_profile_json = Some(raw.clone());
                source.snapshot = Some(profile_to_snapshot(profile));
                source.profile_cached_at = Some(Utc::now());
            }
        }
        Some(build_cached_update_evidence(&mods, None))
    }

    fn thalia_recommendation_for_profile(
        &self,
        mod_entry_id: &str,
        profile: &gamebanana::ProfileResponse,
    ) -> Option<ThaliaRecommendation> {
        let evidence = self.thalia_evidence_for_profile(mod_entry_id, profile)?;
        self.thalia_recommendations_from_evidence(&evidence)
            .remove(mod_entry_id)
    }

    fn validate_thalia_install_plan(
        &self,
        plan: &ThaliaInstallPlan,
        target: &ModEntry,
        fresh_profile: &gamebanana::ProfileResponse,
    ) -> Option<ValidatedThaliaInstall> {
        let action = match plan.recommendation.action {
            ThaliaRecommendationAction::Update => thalia::UpdateMatchAction::Update,
            ThaliaRecommendationAction::Merge => thalia::UpdateMatchAction::Merge,
            ThaliaRecommendationAction::Current => thalia::UpdateMatchAction::Current,
            ThaliaRecommendationAction::Uncertain => return None,
        };
        if !plan.matches_target(target, self.active_profile_id(&target.game_id)) {
            return None;
        }
        let evidence = self.thalia_evidence_for_profile(&target.id, fresh_profile)?;
        let context = self.thalia_context_for_mod_internal(target, &evidence, true)?;
        if !context.same_semantic_context(&plan.context) {
            return None;
        }
        let response = thalia::UpdateMatchResponse {
            action,
            id: plan.recommendation.id,
            requires: plan.recommendation.requires.clone(),
        };
        validated_recommendation(&context, &response)?;
        let canonical = matcher_adapter::canonicalize_profile(fresh_profile);
        let downloadable = |id: u64| {
            canonical.active.iter().find(|file| {
                file.id == id && profile_file_is_active_downloadable(&canonical.profile, file)
            })
        };
        let file = match plan.recommendation.id {
            Some(id) => Some(downloadable(id)?.clone()),
            None => None,
        };
        let mut requires = Vec::new();
        for required in &plan.recommendation.requires {
            if self.thalia_required_file_is_present(&target.game_id, context.source_mod_id, required.id)
            {
                continue;
            }
            requires.push((required.action, downloadable(required.id)?.clone()));
        }
        Some(ValidatedThaliaInstall {
            action: plan.recommendation.action,
            file,
            requires,
        })
    }

    fn thalia_required_file_is_present(&self, game_id: &str, source_mod_id: u64, file_id: u64) -> bool {
        let installed = self.state.mods.iter().any(|entry| {
            entry.game_id == game_id
                && entry.source.as_ref().is_some_and(|source| {
                    source
                        .gamebanana
                        .as_ref()
                        .is_some_and(|link| link.mod_id == source_mod_id)
                        && (source.file_set.selected_file_ids.contains(&file_id)
                            || source
                                .file_set
                                .selected_files_meta
                                .iter()
                                .any(|file| file.file_id == file_id))
                })
        });
        installed
            || self.pending_browse_install_meta.values().any(|meta| {
                meta.game_id == game_id
                    && meta.mod_id == source_mod_id
                    && meta.selected_files.iter().any(|file| file.id == file_id)
            })
    }

    fn invalidate_thalia_matching(&mut self) {
        self.thalia_matching.invalidate();
    }

    fn invalidate_thalia_matching_keeping_sent(&mut self) {
        self.thalia_matching.invalidate_keeping_sent(true);
    }

    fn invalidate_thalia_matching_for_game(&mut self, game_id: &str) {
        if self
            .selected_game()
            .is_some_and(|game| game.definition.id == game_id)
        {
            self.invalidate_thalia_matching_keeping_sent();
        }
    }
}

#[cfg(test)]
mod thalia_matching_tests {
    use super::*;

    fn overlay_evidence() -> CachedUpdateEvidence {
        let profile = gamebanana::ProfileResponse {
            id: 42,
            files: vec![gamebanana::ModFile {
                id: 88,
                file_name: "new.zip".into(),
                file_size: 123,
                date_added: 20,
                download_count: 0,
                description: None,
                version: None,
                download_url: Some("https://gamebanana.com/dl/88".into()),
                is_archived: false,
                md5: None,
            }],
            ..Default::default()
        };
        CachedUpdateEvidence {
            state: ModUpdateState::UpToDate,
            signature: None,
            prearmed_signature: None,
            newer_files: profile.files.clone(),
            installed_files: vec![TrackedFileMeta {
                file_id: 7,
                date_added: 10,
                ..Default::default()
            }],
            other_mods_files: Vec::new(),
            profile: Some(Arc::new(profile)),
            profile_cached_at: None,
            thalia_pending: false,
        }
    }

    #[test]
    fn accepted_update_exposes_exact_signature_without_changing_factual_files() {
        let mut evidence = overlay_evidence();
        overlay_thalia_recommendation(
            &mut evidence,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Update,
                id: Some(88),
                requires: Vec::new(),
            },
        );
        assert_eq!(evidence.state, ModUpdateState::UpdateAvailable);
        assert_eq!(evidence.signature.unwrap().files[0].file_id, 88);
        assert_eq!(evidence.newer_files[0].id, 88);
        assert_eq!(evidence.installed_files[0].file_id, 7);
    }

    #[test]
    fn nonreplacement_answers_only_suppress_notice_when_current() {
        let mut uncertain = overlay_evidence();
        overlay_thalia_recommendation(
            &mut uncertain,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Uncertain,
                id: None,
                requires: Vec::new(),
            },
        );
        assert_eq!(uncertain.state, ModUpdateState::UpToDate);
        assert!(uncertain.signature.is_none());
        assert_eq!(uncertain.newer_files[0].id, 88);

        let mut merge = overlay_evidence();
        overlay_thalia_recommendation(
            &mut merge,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Merge,
                id: Some(88),
                requires: Vec::new(),
            },
        );
        assert_eq!(merge.state, ModUpdateState::UpdateAvailable);
        assert_eq!(merge.signature.unwrap().files[0].file_id, 88);
        assert_eq!(merge.newer_files[0].id, 88);

        let mut current = overlay_evidence();
        overlay_thalia_recommendation(
            &mut current,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Current,
                id: None,
                requires: Vec::new(),
            },
        );
        assert!(current.newer_files.is_empty());
        assert_eq!(current.state, ModUpdateState::UpToDate);

        let mut local_update = overlay_evidence();
        local_update.state = ModUpdateState::UpdateAvailable;
        local_update.newer_files = vec![gamebanana::ModFile {
            id: 99,
            file_name: "local.zip".into(),
            file_size: 1,
            date_added: 30,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some("https://gamebanana.com/dl/99".into()),
            is_archived: false,
            md5: None,
        }];
        overlay_thalia_recommendation(
            &mut local_update,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Current,
                id: None,
                requires: Vec::new(),
            },
        );
        assert_eq!(local_update.newer_files[0].id, 99);
    }

    #[test]
    fn local_update_wins_and_archived_ai_target_is_rejected() {
        let answer = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        let mut evidence = overlay_evidence();
        evidence.state = ModUpdateState::UpdateAvailable;
        evidence.signature = candidate_signature(vec![TrackedFileMeta {
            file_id: 99,
            ..Default::default()
        }]);
        overlay_thalia_recommendation(&mut evidence, &answer);
        assert_eq!(evidence.signature.unwrap().files[0].file_id, 99);
        let mut evidence = overlay_evidence();
        let profile = Arc::make_mut(evidence.profile.as_mut().unwrap());
        profile.archived_files = profile.files.clone();
        overlay_thalia_recommendation(&mut evidence, &answer);
        assert_eq!(evidence.state, ModUpdateState::UpToDate);
    }

    #[test]
    fn live_recommendations_settle_peers_and_reject_collisions() {
        let mut state = test_state();
        let first = context_for("a", 1, vec![88]);
        let second = context_for("b", 2, vec![88]);
        for context in [&first, &second] {
            state.insert_cache(
                context.key(),
                thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Update,
                    id: Some(88),
                    requires: Vec::new(),
                },
            );
        }
        assert!(
            state
                .current_recommendations(&[first.clone(), second])
                .is_empty()
        );
        assert_eq!(
            state.current_recommendations(&[first.clone()])["a"].id,
            Some(88)
        );
        state.cache.get_mut(&first.key()).unwrap().cached_at =
            Instant::now() - THALIA_MATCH_CACHE_TTL;
        assert_eq!(state.current_recommendations(&[first])["a"].id, Some(88));
    }

    #[test]
    fn notice_is_held_until_an_outcome_arrives() {
        let (mut state, _requests) = test_coordinator();
        let context = context_for("a", 1, vec![88]);
        let contexts = [context.clone()];
        assert!(state.pending_mod_ids(&contexts).is_empty());

        state.begin_round(["game".to_string()]);
        assert!(state.pending_mod_ids(&contexts).contains("a"));

        state.end_round(&HashSet::from(["game".to_string()]));
        assert!(state.pending_mod_ids(&contexts).is_empty());

        state.request_with_gate(context.clone(), false, true);
        assert!(state.pending_mod_ids(&contexts).contains("a"));

        state.inflight.clear();
        state.insert_cache(
            context.key(),
            thalia::UpdateMatchResponse {
                action: thalia::UpdateMatchAction::Current,
                id: None,
                requires: Vec::new(),
            },
        );
        assert!(state.pending_mod_ids(&contexts).is_empty());
    }

    #[test]
    fn failure_or_expired_hold_releases_the_blue_fallback() {
        let mut state = test_state();
        let context = context_for("a", 1, vec![88]);
        let contexts = [context.clone()];

        state.begin_round(["game".to_string()]);
        state.record_failure(&context.key(), None, false);
        assert!(state.pending_mod_ids(&contexts).is_empty());

        let mut state = test_state();
        state
            .awaiting_games
            .insert("game".to_string(), Instant::now() - THALIA_MATCH_PENDING_HOLD);
        assert!(state.pending_mod_ids(&contexts).is_empty());
        assert!(state.next_hold_expiry().is_none());
    }

    #[test]
    fn forced_refresh_keeps_the_previous_answer_visible() {
        let (mut state, _requests) = test_coordinator();
        let context = context_for("a", 1, vec![88]);
        state.insert_cache(
            context.key(),
            thalia::UpdateMatchResponse {
                action: thalia::UpdateMatchAction::Current,
                id: None,
                requires: Vec::new(),
            },
        );
        state.request_with_gate(context.clone(), true, true);
        assert!(state.inflight.contains_key(&context.key()));
        assert!(state.pending_mod_ids(std::slice::from_ref(&context)).is_empty());
        assert_eq!(
            state.current_recommendations(&[context])["a"].action,
            ThaliaRecommendationAction::Current
        );
    }

    #[test]
    fn live_recommendation_waits_for_pending_peer() {
        let (mut state, _requests) = test_coordinator();
        let first = context_for("a", 1, vec![88]);
        state.insert_cache(
            first.key(),
            thalia::UpdateMatchResponse {
                action: thalia::UpdateMatchAction::Update,
                id: Some(88),
                requires: Vec::new(),
            },
        );
        state.request_with_gate(context_for("b", 2, vec![88]), false, true);
        assert!(state.current_recommendations(&[first]).is_empty());
    }

    fn tracked_mod_for_test(id: &str, status: ModStatus) -> ModEntry {
        let recipe = FileSetRecipe {
            selected_file_ids: vec![7],
            ..Default::default()
        };
        ModEntry {
            id: id.into(),
            game_id: "game".into(),
            folder_name: id.into(),
            root_path: PathBuf::from("root"),
            status,
            metadata: crate::model::ModMetadata::default(),
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
                gamebanana: Some(GameBananaLink {
                    mod_id: 42,
                    url: gamebanana::browser_url(42),
                }),
                file_set: recipe.clone(),
                ..Default::default()
            }),
            update_state: ModUpdateState::UpToDate,
        }
    }

    #[test]
    fn queued_update_plan_rejects_changed_target_and_profile() {
        let mut target = tracked_mod_for_test("a", ModStatus::Active);
        let recipe = target.source.as_ref().unwrap().file_set.clone();
        let mut context = context_for("a", 1, vec![88]);
        context.file_set_fingerprint = thalia_hash(&serde_json::to_string(&recipe).unwrap());
        let plan = ThaliaInstallPlan {
            context,
            source_url: gamebanana::browser_url(42),
            recommendation: ThaliaRecommendation {
                action: ThaliaRecommendationAction::Update,
                id: Some(88),
                requires: Vec::new(),
            },
        };
        assert!(plan.matches_target(&target, None));
        assert!(!plan.matches_target(&target, Some("12345678".parse().unwrap())));
        target.root_path = PathBuf::from("moved");
        assert!(!plan.matches_target(&target, None));
        target.root_path = PathBuf::from("root");
        target.source.as_mut().unwrap().file_set.selected_file_ids = vec![8];
        assert!(!plan.matches_target(&target, None));
        target.source.as_mut().unwrap().file_set = recipe;
        target
            .source
            .as_mut()
            .unwrap()
            .gamebanana
            .as_mut()
            .unwrap()
            .url = gamebanana::browser_url(99);
        assert!(!plan.matches_target(&target, None));
    }

    #[test]
    fn completed_and_cached_thalia_updates_follow_automatic_status_preferences() {
        let (mut state, mut request_rx) = test_coordinator();
        let cases = [
            (
                "update",
                thalia::UpdateMatchAction::Update,
                Some(88),
                ModStatus::Active,
            ),
            (
                "disabled",
                thalia::UpdateMatchAction::Update,
                Some(88),
                ModStatus::Disabled,
            ),
            (
                "merge",
                thalia::UpdateMatchAction::Merge,
                Some(88),
                ModStatus::Active,
            ),
            (
                "current",
                thalia::UpdateMatchAction::Current,
                None,
                ModStatus::Active,
            ),
            (
                "uncertain",
                thalia::UpdateMatchAction::Uncertain,
                None,
                ModStatus::Active,
            ),
        ];
        let mods: Vec<_> = cases
            .iter()
            .map(|(id, _, _, status)| tracked_mod_for_test(id, status.clone()))
            .collect();
        let checked: HashSet<_> = mods.iter().map(|entry| entry.id.clone()).collect();
        let mut contexts = Vec::new();
        let mut results = Vec::new();
        for (index, (id, action, file_id, _)) in cases.into_iter().enumerate() {
            let mut context = context_for(id, 0, vec![88]);
            context.source_mod_id += index as u64;
            state.request_with_gate(context, false, true);
            let request = request_rx.try_recv().unwrap();
            contexts.push(request.context.clone());
            results.push(ThaliaMatchResult::Completed {
                context: request.context,
                response: thalia::UpdateMatchResponse {
                    action,
                    id: file_id,
                    requires: Vec::new(),
                },
            });
        }
        state.apply_results(results, |_| true);
        let answers = state.current_recommendations(&contexts);
        let active_only = ModStatusTargets {
            active: true,
            disabled: false,
            archived: false,
        };
        assert_eq!(
            automatic_thalia_update_ids(&mods, &answers, &checked, active_only),
            vec!["merge", "update"]
        );
        assert!(
            automatic_thalia_update_ids(&mods, &answers, &HashSet::new(), active_only).is_empty()
        );
        assert!(
            automatic_thalia_update_ids(
                &mods,
                &answers,
                &checked,
                ModStatusTargets {
                    active: false,
                    disabled: false,
                    archived: false
                }
            )
            .is_empty()
        );
        assert_eq!(
            automatic_thalia_update_ids(
                &mods,
                &answers,
                &checked,
                ModStatusTargets {
                    active: false,
                    disabled: true,
                    archived: false
                }
            ),
            vec!["disabled"]
        );

        state.request_with_gate(contexts[0].clone(), false, true);
        assert!(
            request_rx.try_recv().is_err(),
            "fresh result should be reused without another HTTP job"
        );
        let cached_answers = state.current_recommendations(&contexts);
        assert_eq!(
            automatic_thalia_update_ids(&mods, &cached_answers, &checked, active_only),
            vec!["merge", "update"]
        );
        state.cache.get_mut(&contexts[0].key()).unwrap().cached_at =
            Instant::now() - THALIA_MATCH_CACHE_TTL;
        assert_eq!(
            automatic_thalia_update_ids(
                &mods,
                &state.fresh_recommendations(&contexts),
                &checked,
                active_only
            ),
            vec!["merge"]
        );
    }

    #[test]
    fn refused_answer_is_saved_and_matches_only_the_same_answer() {
        use thalia::{UpdateMatchRequiredAction as Mode, UpdateMatchRequiredFile as Required};
        let answer = |action, id, requires: Vec<(u64, Mode)>| ThaliaRecommendation {
            action,
            id,
            requires: requires
                .into_iter()
                .map(|(id, action)| Required { id, action })
                .collect(),
        };
        let update = ThaliaRecommendationAction::Update;
        let refused = refused_thalia_answer(&answer(
            update,
            Some(88),
            vec![(90, Mode::Merge), (89, Mode::Merge)],
        ));

        assert_eq!(
            refused,
            refused_thalia_answer(&answer(
                update,
                Some(88),
                vec![(89, Mode::Merge), (90, Mode::Merge)]
            ))
        );
        for other in [
            answer(update, Some(87), vec![(89, Mode::Merge), (90, Mode::Merge)]),
            answer(
                update,
                Some(88),
                vec![(89, Mode::Merge), (90, Mode::Separate)],
            ),
            answer(update, Some(88), vec![(89, Mode::Merge)]),
            answer(
                ThaliaRecommendationAction::Merge,
                Some(88),
                vec![(89, Mode::Merge), (90, Mode::Merge)],
            ),
        ] {
            assert_ne!(refused, refused_thalia_answer(&other));
        }

        let source = ModSourceData {
            refused_thalia_answer: Some(refused.clone()),
            ..Default::default()
        };
        let saved = serde_json::to_value(&source).unwrap();
        assert_eq!(
            saved["refused_thalia_answer"],
            serde_json::json!({
                "action": "update",
                "file": 88,
                "requires": [
                    { "action": "merge", "file": 89 },
                    { "action": "merge", "file": 90 },
                ],
            })
        );
        let saved: ModSourceData = serde_json::from_value(saved).unwrap();
        assert_eq!(saved.refused_thalia_answer, Some(refused.clone()));
        let plain = serde_json::to_value(ModSourceData::default()).unwrap();
        assert!(plain.get("refused_thalia_answer").is_none());
        let old: ModSourceData = serde_json::from_value(plain).unwrap();
        assert_eq!(old.refused_thalia_answer, None);
    }

    #[test]
    fn bundle_member_is_never_retried_alone() {
        let mut payload: crate::model::BrowseDownloadTaskPayload =
            serde_json::from_value(serde_json::json!({
                "game_id": "game", "mod_id": 42,
                "file": { "id": 88, "file_name": "new.zip", "file_size": 1, "date_added": 20 }
            }))
            .unwrap();
        let mut task = TaskEntry {
            id: 1,
            kind: TaskKind::Download,
            status: TaskStatus::Canceled,
            title: "new.zip".into(),
            game_id: Some("game".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            total_size: None,
            unsafe_content: false,
            retry_payload: Some(TaskRetryPayload::BrowseDownload(payload.clone())),
        };
        assert!(HestiaApp::task_is_retryable_browse_download(&task));

        payload.thalia_bundle = true;
        let saved: crate::model::BrowseDownloadTaskPayload =
            serde_json::from_str(&serde_json::to_string(&payload).unwrap()).unwrap();
        assert!(saved.thalia_bundle);
        task.retry_payload = Some(TaskRetryPayload::BrowseDownload(saved));
        assert!(!HestiaApp::task_is_retryable_browse_download(&task));
    }

    #[test]
    fn queued_task_without_download_payload_already_blocks_a_duplicate_update() {
        let target = tracked_mod_for_test("a", ModStatus::Active);
        let mut task = TaskEntry {
            id: 1,
            kind: TaskKind::Download,
            status: TaskStatus::Queued,
            title: "Updating a".into(),
            game_id: Some(target.game_id.clone()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            total_size: None,
            unsafe_content: false,
            retry_payload: None,
        };
        assert!(update_task_targets_mod(&task, &target, "Updating a"));
        task.status = TaskStatus::Canceling;
        assert!(update_task_targets_mod(&task, &target, "Updating a"));
        task.status = TaskStatus::Failed;
        assert!(!update_task_targets_mod(&task, &target, "Updating a"));
        task.status = TaskStatus::Queued;
        task.game_id = Some("other-game".into());
        assert!(!update_task_targets_mod(&task, &target, "Updating a"));

        task.game_id = Some(target.game_id.clone());
        let mut legacy_payload: crate::model::BrowseDownloadTaskPayload =
            serde_json::from_value(serde_json::json!({
                "game_id": "game", "mod_id": 42,
                "file": { "id": 88, "file_name": "new.zip", "file_size": 1, "date_added": 20 }
            }))
            .unwrap();
        task.retry_payload = Some(TaskRetryPayload::BrowseDownload(legacy_payload.clone()));
        assert!(update_task_targets_mod(&task, &target, "Updating a"));
        legacy_payload.update_target_mod_id = Some("other-mod".into());
        task.retry_payload = Some(TaskRetryPayload::BrowseDownload(legacy_payload));
        assert!(!update_task_targets_mod(&task, &target, "Updating a"));
    }

    #[test]
    fn bundle_of_separate_files_blocks_a_duplicate_update() {
        let target = tracked_mod_for_test("a", ModStatus::Active);
        let mut bundle = ThaliaInstallBundle {
            key: 9,
            title: "a".into(),
            game_id: target.game_id.clone(),
            members: vec![2],
            separate: HashSet::from([2]),
            main: None,
            target: Some((target.id.clone(), Some(1))),
            answer: None,
            downloaded: HashMap::new(),
            preparing: false,
            prepared_root: None,
            target_changed: false,
            finished: HashSet::new(),
            installed_any: false,
        };
        assert!(thalia_bundle_updates_mod(&bundle, &target));
        bundle.game_id = "other-game".into();
        assert!(!thalia_bundle_updates_mod(&bundle, &target));
        bundle.game_id = target.game_id.clone();
        bundle.target = Some(("other-mod".into(), Some(1)));
        assert!(!thalia_bundle_updates_mod(&bundle, &target));
        bundle.target = None;
        assert!(!thalia_bundle_updates_mod(&bundle, &target));
    }

    fn context_for(
        local_mod_id: &str,
        request_id: u64,
        candidates: Vec<u64>,
    ) -> ThaliaMatchContext {
        ThaliaMatchContext {
            generation: 0,
            request_id,
            local_mod_id: local_mod_id.to_string(),
            game_id: "game".to_string(),
            source_mod_id: 42,
            active_profile_id: None,
            root_path: PathBuf::from("root"),
            file_set_fingerprint: 1,
            catalog_fingerprint: 2,
            peer_fingerprint: 3,
            policy_fingerprint: 4,
            installed_file_id: 7,
            candidate_file_ids: candidates,
            peer_local_claim_ids: Vec::new(),
        }
    }

    #[test]
    fn required_files_must_be_known() {
        use thalia::{UpdateMatchRequiredAction as Mode, UpdateMatchRequiredFile as Required};
        let mut context = context_for("a", 0, vec![88, 89, 90]);
        context.peer_local_claim_ids = vec![55];
        let answer = |action, requires: Vec<(u64, Mode)>| ThaliaRecommendation {
            action,
            id: Some(88),
            requires: requires
                .into_iter()
                .map(|(id, action)| Required { id, action })
                .collect(),
        };
        let update = ThaliaRecommendationAction::Update;
        let merge = ThaliaRecommendationAction::Merge;

        assert!(required_files_are_valid(
            &context,
            &answer(update, vec![(89, Mode::Separate), (90, Mode::Merge)])
        ));
        assert!(required_files_are_valid(&context, &answer(update, vec![(55, Mode::Separate)])));
        assert!(required_files_are_valid(&context, &answer(merge, vec![(89, Mode::Separate)])));

        assert!(!required_files_are_valid(&context, &answer(update, vec![(12, Mode::Separate)])));
        assert!(!required_files_are_valid(&context, &answer(update, vec![(7, Mode::Merge)])));
        assert!(!required_files_are_valid(&context, &answer(update, vec![(88, Mode::Merge)])));
        assert!(required_files_are_valid(
            &context,
            &answer(update, vec![(89, Mode::Merge), (90, Mode::Merge)])
        ));
        assert!(required_files_are_valid(&context, &answer(merge, vec![(89, Mode::Merge)])));

        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
            requires: vec![Required {
                id: 12,
                action: Mode::Separate,
            }],
        };
        assert!(validated_recommendation(&context, &response).is_none());
    }

    #[test]
    fn mock_answers_with_requires_parse_check_and_show() {
        use thalia::{UpdateMatchRequiredAction as Mode, UpdateMatchRequiredFile as Required};
        let file = |id: u64, date_added: i64| gamebanana::ModFile {
            id,
            file_name: format!("{id}.zip"),
            file_size: 100 + id,
            date_added,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some(format!("https://gamebanana.com/dl/{id}")),
            is_archived: false,
            md5: None,
        };
        let evidence = || {
            let mut evidence = overlay_evidence();
            let profile = gamebanana::ProfileResponse {
                id: 42,
                files: vec![file(88, 20), file(89, 21), file(90, 22)],
                ..Default::default()
            };
            evidence.newer_files = profile.files.clone();
            evidence.profile = Some(Arc::new(profile));
            evidence
        };
        let context = context_for("a", 0, vec![88, 89, 90]);
        let answer = |body: &str| {
            let response = thalia::parse_update_match_response(body.as_bytes()).unwrap();
            validated_recommendation(&context, &response)
        };
        let shown = |answer: &ThaliaRecommendation| {
            let mut evidence = evidence();
            overlay_thalia_recommendation(&mut evidence, answer);
            let ids = evidence
                .signature
                .map(|signature| signature.files.iter().map(|file| file.file_id).collect())
                .unwrap_or_else(Vec::new);
            (evidence.state, ids)
        };

        let update =
            answer(r#"{"action":"update","file":88,"requires":[{"file":89,"action":"separate"}]}"#)
                .unwrap();
        assert_eq!(update.action, ThaliaRecommendationAction::Update);
        assert_eq!(
            update.requires,
            vec![Required {
                id: 89,
                action: Mode::Separate
            }]
        );
        assert!(thalia_recommendation_installs_files(&update));
        assert_eq!(shown(&update), (ModUpdateState::UpdateAvailable, vec![88]));

        let merged =
            answer(r#"{"action":"update","file":88,"requires":[{"file":90,"action":"merge"}]}"#)
                .unwrap();
        assert_eq!(merged.requires[0].action, Mode::Merge);
        assert!(thalia_recommendation_installs_files(&merged));

        let merge = answer(r#"{"action":"merge","file":88,"requires":[]}"#).unwrap();
        assert_eq!(merge.action, ThaliaRecommendationAction::Merge);
        assert!(merge.requires.is_empty());
        assert_eq!(shown(&merge), (ModUpdateState::UpdateAvailable, vec![88]));

        let current =
            answer(r#"{"action":"current","file":null,"requires":[{"file":89,"action":"separate"}]}"#)
                .unwrap();
        assert_eq!(current.action, ThaliaRecommendationAction::Current);
        assert!(current.id.is_none());
        assert!(thalia_recommendation_installs_files(&current));
        assert_eq!(shown(&current), (ModUpdateState::UpdateAvailable, vec![89]));
        let current_merge =
            answer(r#"{"action":"current","file":null,"requires":[{"file":90,"action":"merge"}]}"#)
                .unwrap();
        assert_eq!(shown(&current_merge), (ModUpdateState::UpdateAvailable, vec![90]));

        let mut installed = current.clone();
        installed.requires.clear();
        assert!(!thalia_recommendation_installs_files(&installed));
        let (state, ids) = shown(&installed);
        assert_eq!(state, ModUpdateState::UpToDate);
        assert!(ids.is_empty());

        let uncertain = answer(r#"{"action":"uncertain","file":null,"requires":[]}"#).unwrap();
        assert!(!thalia_recommendation_installs_files(&uncertain));

        assert!(
            answer(r#"{"action":"current","file":null,"requires":[{"file":12,"action":"separate"}]}"#)
                .is_none()
        );
        assert!(
            answer(r#"{"action":"current","file":null,"requires":[{"file":7,"action":"separate"}]}"#)
                .is_none()
        );
        assert!(
            answer(
                r#"{"action":"current","file":null,"requires":[{"file":89,"action":"merge"},{"file":90,"action":"merge"}]}"#
            )
            .is_some()
        );
        assert!(answer(r#"{"action":"update","file":88}"#).unwrap().requires.is_empty());
    }

    fn test_state() -> ThaliaMatchingState {
        let (request_tx, _request_rx) = worker_channel::<ThaliaMatchRequest>();
        let (_result_tx, result_rx) = worker_channel::<ThaliaMatchResult>();
        ThaliaMatchingState {
            request_tx,
            result_rx,
            generation: 0,
            next_request_id: 1,
            force_refresh_games: HashSet::new(),
            inflight: HashMap::new(),
            detached: HashMap::new(),
            cache: HashMap::new(),
            failure_backoff: HashMap::new(),
            rejected: HashMap::new(),
            failed_at: HashMap::new(),
            awaiting_games: HashMap::new(),
            recommendations: HashMap::new(),
            claims: HashMap::new(),
            blocked_claims: HashSet::new(),
            deferred: Vec::new(),
        }
    }

    fn test_coordinator() -> (ThaliaMatchingState, WorkerRx<ThaliaMatchRequest>) {
        let (request_tx, request_rx) = worker_channel::<ThaliaMatchRequest>();
        let (_result_tx, result_rx) = worker_channel::<ThaliaMatchResult>();
        (
            ThaliaMatchingState {
                request_tx,
                result_rx,
                generation: 0,
                next_request_id: 1,
                force_refresh_games: HashSet::new(),                inflight: HashMap::new(),
                detached: HashMap::new(),
                cache: HashMap::new(),
                failure_backoff: HashMap::new(),
                rejected: HashMap::new(),
                failed_at: HashMap::new(),
                awaiting_games: HashMap::new(),
                recommendations: HashMap::new(),
                claims: HashMap::new(),
                blocked_claims: HashSet::new(),
                deferred: Vec::new(),
            },
            request_rx,
        )
    }

    #[test]
    fn missing_persisted_installed_id_skips_matching() {
        let recipe = FileSetRecipe {
            selected_file_names: vec!["old.zip".to_string()],
            ..Default::default()
        };
        assert_eq!(persisted_single_installed_file_id(&recipe), None);
    }

    #[test]
    fn multiple_installed_files_skip_single_source_matching() {
        let recipe = FileSetRecipe {
            selected_file_ids: vec![11, 12],
            ..Default::default()
        };
        assert_eq!(persisted_single_installed_file_id(&recipe), None);
    }

    #[test]
    fn persisted_id_must_agree_with_file_metadata() {
        let recipe = FileSetRecipe {
            selected_file_ids: vec![11],
            selected_files_meta: vec![TrackedFileMeta {
                file_id: 12,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(persisted_single_installed_file_id(&recipe), None);
    }

    #[test]
    fn metadata_only_recipe_with_known_id_is_a_single_source() {
        let recipe = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: 17,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(persisted_single_installed_file_id(&recipe), Some(17));
    }

    #[test]
    fn metadata_only_recipe_without_id_still_skips_matching() {
        let recipe = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta::default()],
            ..Default::default()
        };
        assert_eq!(persisted_single_installed_file_id(&recipe), None);
    }

    #[test]
    fn historical_catalog_can_supply_newer_file_when_installed_id_is_absent() {
        let profile = gamebanana::ProfileResponse {
            id: 42,
            files: vec![gamebanana::ModFile {
                id: 88,
                file_name: "new.zip".to_string(),
                file_size: 1,
                date_added: 20,
                download_count: 0,
                description: None,
                version: None,
                download_url: Some("https://gamebanana.com/dl/88".to_string()),
                is_archived: false,
                md5: None,
            }],
            ..Default::default()
        };
        assert_eq!(historical_newer_candidate_ids(&profile, 7, 10), vec![88]);
    }

    #[test]
    fn historical_catalog_rejects_active_file_shadowed_by_archived_same_id() {
        let file = gamebanana::ModFile {
            id: 88,
            file_name: "new.zip".to_string(),
            file_size: 1,
            date_added: 20,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some("https://gamebanana.com/dl/88".to_string()),
            is_archived: false,
            md5: None,
        };
        let profile = gamebanana::ProfileResponse {
            id: 42,
            files: vec![file.clone()],
            archived_files: vec![file],
            ..Default::default()
        };
        assert!(historical_newer_candidate_ids(&profile, 7, 10).is_empty());
    }

    #[test]
    fn update_response_must_name_a_known_newer_candidate() {
        let context = context_for("mod", 1, vec![88]);
        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(77),
            requires: Vec::new(),
        };
        assert!(validated_recommendation(&context, &response).is_none());
    }

    #[test]
    fn merge_response_is_kept_separate_from_update() {
        let context = context_for("mod", 1, vec![88]);
        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Merge,
            id: Some(88),
            requires: Vec::new(),
        };
        let recommendation = validated_recommendation(&context, &response).unwrap();
        assert_eq!(recommendation.action, ThaliaRecommendationAction::Merge);
        assert_eq!(recommendation.id, Some(88));
    }

    #[test]
    fn current_and_uncertain_require_no_file_id() {
        let context = context_for("mod", 1, vec![88]);
        for action in [
            thalia::UpdateMatchAction::Current,
            thalia::UpdateMatchAction::Uncertain,
        ] {
            let response = thalia::UpdateMatchResponse { action, id: None, requires: Vec::new() };
            assert!(validated_recommendation(&context, &response).is_some());
        }
    }

    #[test]
    fn current_notice_suppression_requires_the_same_context() {
        let mut state = test_state();
        let context = context_for("mod", 1, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
                cached_at: Instant::now(),
            },
        );
        assert_eq!(
            state.current_recommendations(&[context.clone()])["mod"].action,
            ThaliaRecommendationAction::Current
        );

        let changed = context_for("mod", 2, vec![99]);
        assert!(state.current_recommendations(&[changed]).is_empty());

        state.cache.get_mut(&context.key()).unwrap().cached_at =
            Instant::now() - THALIA_MATCH_CACHE_TTL - Duration::from_secs(1);
        assert_eq!(
            state.current_recommendations(std::slice::from_ref(&context))["mod"].action,
            ThaliaRecommendationAction::Current
        );
        assert!(state.fresh_recommendations(&[context]).is_empty());
    }

    #[test]
    fn late_old_result_cannot_remove_a_reused_semantic_request() {
        let mut state = test_state();
        let old = context_for("mod", 1, vec![88]);
        let fresh = context_for("mod", 2, vec![88]);
        state.inflight.insert(
            fresh.key(),
            ThaliaInflightRequest {
                request_id: fresh.request_id,
                context: fresh.clone(),
                cancellation: Arc::new(AtomicBool::new(false)),
                started: Arc::new(AtomicBool::new(false)),
            },
        );
        state.apply_results(
            vec![ThaliaMatchResult::Completed {
                context: old,
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
            }],
            |_| true,
        );
        assert!(state.inflight.contains_key(&fresh.key()));
    }

    #[test]
    fn conflicting_same_group_claims_block_later_cached_claimants() {
        let mut state = test_state();
        let contexts = [
            context_for("a", 1, vec![88]),
            context_for("b", 2, vec![88]),
            context_for("c", 3, vec![88]),
        ];
        for context in &contexts[..2] {
            state.inflight.insert(
                context.key(),
                ThaliaInflightRequest {
                    request_id: context.request_id,
                    context: context.clone(),
                    cancellation: Arc::new(AtomicBool::new(false)),
                    started: Arc::new(AtomicBool::new(false)),
                },
            );
        }
        let response = || thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        state.apply_results(
            contexts[..2]
                .iter()
                .cloned()
                .map(|context| ThaliaMatchResult::Completed {
                    context,
                    response: response(),
                })
                .collect(),
            |_| true,
        );
        assert!(state.recommendations.is_empty());
        assert!(
            state
                .blocked_claims
                .contains(&("game".to_string(), 42, 88, 2, 3))
        );
        state.inflight.insert(
            contexts[2].key(),
            ThaliaInflightRequest {
                request_id: contexts[2].request_id,
                context: contexts[2].clone(),
                cancellation: Arc::new(AtomicBool::new(false)),
                started: Arc::new(AtomicBool::new(false)),
            },
        );
        state.apply_results(
            vec![ThaliaMatchResult::Completed {
                context: contexts[2].clone(),
                response: response(),
            }],
            |_| true,
        );
        assert!(state.recommendations.is_empty());
    }

    #[test]
    fn deferred_first_claim_is_revalidated_when_peer_result_arrives_later() {
        let mut state = test_state();
        let first = context_for("a", 1, vec![88]);
        let second = context_for("b", 2, vec![88]);
        state.inflight.insert(
            first.key(),
            ThaliaInflightRequest {
                request_id: first.request_id,
                context: first.clone(),
                cancellation: Arc::new(AtomicBool::new(false)),
                started: Arc::new(AtomicBool::new(false)),
            },
        );
        state.inflight.insert(
            second.key(),
            ThaliaInflightRequest {
                request_id: second.request_id,
                context: second.clone(),
                cancellation: Arc::new(AtomicBool::new(false)),
                started: Arc::new(AtomicBool::new(false)),
            },
        );
        let response = || thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        state.apply_results(
            vec![ThaliaMatchResult::Completed {
                context: first.clone(),
                response: response(),
            }],
            |_| true,
        );
        assert_eq!(state.deferred.len(), 1);
        state.apply_results(
            vec![ThaliaMatchResult::Completed {
                context: second,
                response: response(),
            }],
            |_| true,
        );
        assert!(state.recommendations.is_empty());
        assert!(
            state
                .blocked_claims
                .contains(&("game".to_string(), 42, 88, 2, 3))
        );
    }

    #[test]
    fn removing_a_peer_claim_unblocks_the_remaining_current_claim() {
        let mut state = test_state();
        let first = context_for("a", 1, vec![88]);
        let second = context_for("b", 2, vec![88]);
        let recommendation = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        for (context, mod_id) in [(&first, "a"), (&second, "b")] {
            state.cache.insert(
                context.key(),
                ThaliaCachedRecommendation {
                    response: thalia::UpdateMatchResponse {
                        action: thalia::UpdateMatchAction::Update,
                        id: Some(88),
                        requires: Vec::new(),
                    },
                    cached_at: Instant::now(),
                },
            );
            state.claims.insert(
                mod_id.to_string(),
                (context.clone(), recommendation.clone()),
            );
        }
        state.recommendations.clear();
        state.blocked_claims.insert(claim_block_key(&first, 88));

        state.prune_claims_to_contexts(&[first.clone()]);
        state.restore_cached_responses(&[first]);

        assert!(state.recommendations.contains_key("a"));
        assert!(!state.claims.contains_key("b"));
        assert!(state.blocked_claims.is_empty());
    }

    #[test]
    fn expired_peer_claim_does_not_block_a_fresh_claim() {
        let mut state = test_state();
        let first = context_for("a", 1, vec![88]);
        let second = context_for("b", 2, vec![88]);
        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        let recommendation = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(88),
            requires: Vec::new(),
        };
        state.cache.insert(
            first.key(),
            ThaliaCachedRecommendation {
                response: response.clone(),
                cached_at: Instant::now(),
            },
        );
        state.cache.insert(
            second.key(),
            ThaliaCachedRecommendation {
                response: response.clone(),
                cached_at: Instant::now() - THALIA_MATCH_CACHE_TTL - Duration::from_secs(1),
            },
        );
        state
            .claims
            .insert("a".to_string(), (first.clone(), recommendation.clone()));
        state
            .claims
            .insert("b".to_string(), (second.clone(), recommendation));
        state.blocked_claims.insert(claim_block_key(&first, 88));

        state.prune_claims_to_contexts(&[first.clone(), second]);
        state.accept_response(first, response);

        assert!(state.recommendations.contains_key("a"));
        assert!(!state.claims.contains_key("b"));
        assert!(state.blocked_claims.is_empty());
    }

    #[test]
    fn disabled_gate_queues_no_request() {
        let (mut state, mut request_rx) = test_coordinator();
        state.request_with_gate(context_for("mod", 0, vec![88]), false, false);
        assert!(request_rx.try_recv().is_err());
        assert!(state.inflight.is_empty());
    }

    #[test]
    fn normal_fresh_cache_hit_queues_no_request() {
        let (mut state, mut request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
                cached_at: Instant::now(),
            },
        );
        state.request_with_gate(context, false, true);
        assert!(request_rx.try_recv().is_err());
        assert!(state.inflight.is_empty());
    }

    #[test]
    fn force_refresh_bypasses_cache_and_sets_request_flag() {
        let (mut state, mut request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
                cached_at: Instant::now(),
            },
        );
        state.request_with_gate(context, true, true);
        let request = request_rx.try_recv().expect("force refresh should queue");
        assert!(request.force_refresh);
    }

    #[test]
    fn force_refresh_still_honors_failure_backoff() {
        let (mut state, mut request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state
            .failure_backoff
            .insert(context.key(), Instant::now() + Duration::from_secs(30));
        state.request_with_gate(context, true, true);
        assert!(request_rx.try_recv().is_err());
        assert!(state.inflight.is_empty());
    }

    #[test]
    fn duplicate_context_queues_only_once() {
        let (mut state, mut request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.request_with_gate(context.clone(), false, true);
        state.request_with_gate(context, false, true);
        assert!(request_rx.try_recv().is_ok());
        assert!(request_rx.try_recv().is_err());
        assert_eq!(state.inflight.len(), 1);
    }

    #[test]
    fn changed_catalog_context_cannot_read_an_old_recommendation() {
        let (mut state, _request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Update,
                    id: Some(88),
                    requires: Vec::new(),
                },
                cached_at: Instant::now(),
            },
        );
        state.recommendations.insert(
            "mod".to_string(),
            ThaliaRecommendation {
                action: ThaliaRecommendationAction::Update,
                id: Some(88),
                requires: Vec::new(),
            },
        );
        let recommendation = state.recommendations["mod"].clone();
        state
            .claims
            .insert("mod".to_string(), (context.clone(), recommendation));
        let mut changed = context;
        changed.catalog_fingerprint = 99;
        assert!(state.recommendation_for_context("mod", &changed).is_none());
        let mut ignored = context_for("mod", 0, vec![88]);
        ignored.policy_fingerprint = 99;
        assert!(state.recommendation_for_context("mod", &ignored).is_none());
    }

    #[test]
    fn cache_hit_does_not_extend_original_expiry() {
        let (mut state, _request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
                cached_at: Instant::now() - THALIA_MATCH_CACHE_TTL + Duration::from_secs(1),
            },
        );
        state.request_with_gate(context.clone(), false, true);
        assert!(state.recommendation_for_context("mod", &context).is_some());
        state.cache.get_mut(&context.key()).unwrap().cached_at =
            Instant::now() - THALIA_MATCH_CACHE_TTL - Duration::from_secs(1);
        assert!(state.recommendation_for_context("mod", &context).is_none());
    }

    #[test]
    fn expired_recommendations_are_not_exposed_by_accessor() {
        let mut state = test_state();
        state.recommendations.insert(
            "mod".to_string(),
            ThaliaRecommendation {
                action: ThaliaRecommendationAction::Current,
                id: None,
                requires: Vec::new(),
            },
        );
        let context = context_for("mod", 1, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
                    requires: Vec::new(),
                },
                cached_at: Instant::now() - THALIA_MATCH_CACHE_TTL - Duration::from_secs(1),
            },
        );
        let recommendation = state.recommendations.get("mod").cloned().unwrap();
        state
            .claims
            .insert("mod".to_string(), (context, recommendation));
        assert!(state.recommendation_for("mod").is_none());
    }

    #[test]
    fn pacer_spaces_starts_and_stays_under_the_minute_limit() {
        let mut pacer = ThaliaPacer::default();
        let start = Instant::now();
        assert_eq!(pacer.try_start(start), None);
        assert_eq!(pacer.try_start(start), Some(THALIA_MATCH_START_SPACING));
        let mut now = start;
        for _ in 1..THALIA_MATCH_PER_MINUTE {
            now += THALIA_MATCH_START_SPACING;
            assert_eq!(pacer.try_start(now), None);
        }
        now += THALIA_MATCH_START_SPACING;
        assert_eq!(
            pacer.try_start(now),
            Some(THALIA_MATCH_MINUTE - (now - start))
        );
        assert_eq!(pacer.try_start(start + THALIA_MATCH_MINUTE), None);

        let later = start + Duration::from_secs(200);
        pacer.pause(later, Some(Duration::ZERO));
        assert_eq!(pacer.try_start(later), Some(THALIA_MATCH_RATE_LIMIT_WAIT));
        pacer.pause(later, Some(Duration::from_secs(5)));
        pacer.pause(later, Some(Duration::from_secs(2)));
        assert_eq!(pacer.try_start(later), Some(Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn busy_answers_are_retried_in_turn_then_given_up() {
        let pacer = Mutex::new(ThaliaPacer {
            spacing: Duration::ZERO,
            rate_limit_wait: Duration::ZERO,
            ..Default::default()
        });
        let request = || ThaliaMatchRequest {
            context: context_for("mod", 0, vec![88]),
            force_refresh: false,
            cancellation: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
        };
        let http = |status| {
            Err(thalia::UpdateMatchError::Http {
                status,
                retry_after: Some(Duration::ZERO),
            })
        };
        let current = || {
            Ok(thalia::UpdateMatchResponse {
                action: thalia::UpdateMatchAction::Current,
                id: None,
                requires: Vec::new(),
            })
        };
        let run = |answers: Vec<Result<_, _>>| {
            let pacer = &pacer;
            async move {
                let request = request();
                let mut answers = answers.into_iter();
                let mut sent = 0;
                let result = fetch_paced_update_match(pacer, &request, || {
                    sent += 1;
                    std::future::ready(answers.next().expect("no more answers"))
                })
                .await;
                (result, sent, request.started.load(Ordering::Relaxed))
            }
        };

        let (result, sent, started) = run(vec![http(429), http(429), current()]).await;
        assert!(result.is_ok());
        assert_eq!(sent, 3);
        assert!(started);
        let (result, sent, _) = run((0..4).map(|_| http(429)).collect()).await;
        assert!(result.unwrap_err().is_rate_limited());
        assert_eq!(sent, 4);
        for status in [422, 502] {
            let (result, sent, _) = run(vec![http(status)]).await;
            assert_eq!(result.unwrap_err().is_permanent(), status == 422);
            assert_eq!(sent, 1);
        }

        let request = request();
        request.cancellation.store(true, Ordering::Relaxed);
        let mut sent = 0;
        let result = fetch_paced_update_match(&pacer, &request, || {
            sent += 1;
            std::future::ready(current())
        })
        .await;
        assert!(result.unwrap_err().is_cancelled());
        assert_eq!(sent, 0);
        assert!(!request.started.load(Ordering::Relaxed));
    }

    #[test]
    fn refused_contexts_are_not_asked_again() {
        let (mut state, mut request_rx) = test_coordinator();
        let context = context_for("mod", 0, vec![88]);
        state.record_failure(&context.key(), None, true);
        state.failure_backoff.clear();
        state.request_with_gate(context.clone(), true, true);
        assert!(request_rx.try_recv().is_err());
        assert!(state.inflight.is_empty());
        assert!(state.failed_at.contains_key(&context.key()));

        let mut changed = context.clone();
        changed.catalog_fingerprint = 9;
        state.request_with_gate(changed, false, true);
        assert!(request_rx.try_recv().is_ok());

        let other = context_for("other", 0, vec![88]);
        state.record_failure(&other.key(), None, false);
        state.failure_backoff.clear();
        state.request_with_gate(other, false, true);
        assert!(request_rx.try_recv().is_ok());
    }

    #[test]
    fn a_switch_keeps_sent_requests_and_drops_queued_ones() {
        let (mut state, mut request_rx) = test_coordinator();
        let (result_tx, result_rx) = worker_channel::<ThaliaMatchResult>();
        state.result_rx = result_rx;
        let sent = context_for("sent", 0, vec![88]);
        state.request_with_gate(sent.clone(), false, true);
        state.request_with_gate(context_for("queued", 0, vec![88]), false, true);
        let sent_request = request_rx.try_recv().unwrap();
        let queued_request = request_rx.try_recv().unwrap();
        sent_request.started.store(true, Ordering::Relaxed);

        state.invalidate_keeping_sent(true);
        assert!(queued_request.cancellation.load(Ordering::Relaxed));
        assert!(!sent_request.cancellation.load(Ordering::Relaxed));

        state.request_with_gate(sent.clone(), false, true);
        assert!(request_rx.try_recv().is_err());
        let answer = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Current,
            id: None,
            requires: Vec::new(),
        };
        result_tx
            .send(ThaliaMatchResult::Completed {
                context: sent_request.context.clone(),
                response: answer.clone(),
            })
            .unwrap();
        let results = state.take_results();
        let [ThaliaMatchResult::Completed { context, .. }] = results.as_slice() else {
            panic!("expected the answer");
        };
        assert_eq!(context.generation, state.generation);
        assert!(context.same_semantic_context(&ThaliaMatchContext {
            generation: state.generation,
            request_id: context.request_id,
            ..sent.clone()
        }));
        state.apply_results(results, |_| true);
        assert!(state.inflight.is_empty());

        state.cache.clear();
        state.request_with_gate(sent.clone(), false, true);
        let again = request_rx.try_recv().unwrap();
        again.started.store(true, Ordering::Relaxed);
        state.invalidate_keeping_sent(true);
        result_tx
            .send(ThaliaMatchResult::Completed {
                context: again.context.clone(),
                response: answer.clone(),
            })
            .unwrap();
        assert!(state.take_results().is_empty());
        assert_eq!(
            state.cache.get(&sent.key()).map(|cached| &cached.response),
            Some(&answer)
        );

        state.cache.clear();
        state.request_with_gate(sent.clone(), false, true);
        let last = request_rx.try_recv().unwrap();
        last.started.store(true, Ordering::Relaxed);
        state.invalidate_keeping_sent(true);
        state.invalidate();
        assert!(last.cancellation.load(Ordering::Relaxed));
        assert!(state.detached.is_empty());
    }
}
