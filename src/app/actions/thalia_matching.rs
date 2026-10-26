const THALIA_MATCH_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const THALIA_MATCH_FAILURE_BACKOFF: Duration = Duration::from_secs(30);
const THALIA_MATCH_QUEUE_LIMIT: usize = 128;
const THALIA_MATCH_CACHE_LIMIT: usize = 512;
const THALIA_MATCH_HTTP_CONCURRENCY: usize = 2;

/// Recommendations are transient; persisted local matching stays independent.
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
    if answer.action == ThaliaRecommendationAction::Current {
        // `current` is a transient acknowledgement for this exact catalog
        // and installed-file context. Keep the local state untouched, but
        // suppress the broader factual notice while this cached answer is
        // fresh. A changed context or an expired cache simply rebuilds the
        // original `newer_files` list.
        entry.newer_files.clear();
        return;
    }
    if answer.action != ThaliaRecommendationAction::Update {
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
                && recommendations.get(&entry.id).is_some_and(|answer| {
                    answer.action == ThaliaRecommendationAction::Update && answer.id.is_some()
                })
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
}

/// Transient app-owned coordination state for Thalia matching requests.
/// Nothing here is serialized, and the rollout gate is checked before a
/// request can leave the process.
pub(crate) struct ThaliaMatchingState {
    request_tx: WorkerTx<ThaliaMatchRequest>,
    result_rx: WorkerRx<ThaliaMatchResult>,
    generation: u64,
    next_request_id: u64,
    force_refresh_games: HashSet<String>,
    inflight: HashMap<ThaliaMatchKey, ThaliaInflightRequest>,
    cache: HashMap<ThaliaMatchKey, ThaliaCachedRecommendation>,
    failure_backoff: HashMap<ThaliaMatchKey, Instant>,
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
            cache: HashMap::new(),
            failure_backoff: HashMap::new(),
            recommendations: HashMap::new(),
            claims: HashMap::new(),
            blocked_claims: HashSet::new(),
            deferred: Vec::new(),
        }
    }

    fn has_pending_results(&self) -> bool {
        !self.result_rx.is_empty()
    }

    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        for request in self.inflight.values() {
            request.cancellation.store(true, Ordering::Relaxed);
        }
        self.inflight.clear();
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
        if force_refresh {
            self.cache.remove(&key);
        }
        if self
            .failure_backoff
            .get(&key)
            .is_some_and(|retry_at| *retry_at > now)
        {
            return;
        }
        if self.inflight.contains_key(&key) || self.inflight.len() >= THALIA_MATCH_QUEUE_LIMIT {
            return;
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        self.inflight.insert(
            key.clone(),
            ThaliaInflightRequest {
                request_id: context.request_id,
                context: context.clone(),
                cancellation: Arc::clone(&cancellation),
            },
        );
        if self
            .request_tx
            .send(ThaliaMatchRequest {
                context,
                force_refresh,
                cancellation,
            })
            .is_err()
        {
            self.inflight.remove(&key);
        }
    }

    fn take_results(&mut self) -> Vec<ThaliaMatchResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }
        results
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

    /// Rebuild recommendations that were previously suppressed by a peer
    /// collision, but only from cache entries belonging to the fresh context
    /// set. This matters when a peer is ignored or removed without a new
    /// network result for the remaining mod.
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
        // Remove only the request identified by the result's generation-local
        // nonce. A late result from before invalidation must not remove a fresh
        // request that happens to have the same semantic key.
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
                        self.record_failure(&key, None);
                        continue;
                    }
                    self.insert_cache(key, response.clone());
                    responses.push((context, response));
                }
                ThaliaMatchResult::Failed {
                    context,
                    retry_after,
                    cancelled,
                } => {
                    if cancelled || context.generation != self.generation {
                        continue;
                    }
                    self.record_failure(&context.key(), retry_after);
                }
            }
        }
        // A completed answer is actionable only after all same-GameBanana-group
        // jobs have settled. Current/Uncertain claim no download target and
        // may be retained immediately; their overlays still require a fresh,
        // matching source/catalog context.
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
            self.record_failure(&context.key(), None);
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
            self.record_failure(&context.key(), None);
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

    fn record_failure(&mut self, key: &ThaliaMatchKey, retry_after: Option<Duration>) {
        let delay = retry_after
            .unwrap_or(THALIA_MATCH_FAILURE_BACKOFF)
            .max(THALIA_MATCH_FAILURE_BACKOFF);
        self.failure_backoff
            .insert(key.clone(), Instant::now() + delay);
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
        let mut answers = Vec::new();
        let mut claims: HashMap<(String, u64, u64), usize> = HashMap::new();
        for context in contexts {
            let Some(cached) = self.cache.get(&context.key()) else {
                continue;
            };
            if cached.cached_at.elapsed() >= THALIA_MATCH_CACHE_TTL {
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
        while let Some(request) = request_rx.recv().await {
            let permit = match Arc::clone(&active_jobs).acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let worker_services = worker_services.clone();
            let result_tx = result_tx.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = match worker_services
                    .async_client_builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                {
                    Ok(client) => {
                        thalia::fetch_update_match(
                            &client,
                            thalia::UpdateMatchRequest {
                                mod_id: request.context.source_mod_id,
                                installed_file_id: request.context.installed_file_id,
                            },
                            request.force_refresh,
                            Arc::clone(&request.cancellation),
                        )
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
                    },
                };
                let _ = result_tx.send(event);
            });
        }
    });
}

fn response_is_actionable(response: &thalia::UpdateMatchResponse) -> bool {
    matches!(
        response.action,
        thalia::UpdateMatchAction::Update | thalia::UpdateMatchAction::Merge
    )
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
                && !context.peer_local_claim_ids.contains(&id))
            .then_some(recommendation)
        }
        ThaliaRecommendationAction::Current | ThaliaRecommendationAction::Uncertain => {
            recommendation.id.is_none().then_some(recommendation)
        }
    }
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
            cached_profile_catalog_fingerprint(profile),
            &profile.name,
            &profile.short_description,
            &profile.html_text,
        ))
        .unwrap_or_default(),
    )
}

impl HestiaApp {
    /// Called only after an explicit local update check has completed (or a
    /// cached local check was explicitly reconciled). Rendering never calls it.
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
        // If the installed file disappeared from the public catalog, the local
        // matcher quite correctly reports MissingSource.  The persisted file
        // metadata still gives us a factual date baseline, so Thalia may
        // recover the update lineage from its historical store.
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
        let recommendations = self.thalia_recommendations_from_evidence(&evidence);
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

    fn thalia_recommendations_from_evidence(
        &self,
        evidence: &HashMap<String, CachedUpdateEvidence>,
    ) -> HashMap<String, ThaliaRecommendation> {
        let contexts: Vec<_> = self
            .state
            .mods
            .iter()
            .filter_map(|entry| self.thalia_context_for_mod_internal(entry, evidence, true))
            .collect();
        self.thalia_matching.current_recommendations(&contexts)
    }

    fn overlay_thalia_update_evidence(&self, evidence: &mut HashMap<String, CachedUpdateEvidence>) {
        let answers = self.thalia_recommendations_from_evidence(evidence);
        for (mod_id, answer) in answers {
            let Some(entry) = evidence.get_mut(&mod_id) else {
                continue;
            };
            overlay_thalia_recommendation(entry, &answer);
        }
    }

    fn thalia_evidence_fingerprint(&self) -> u64 {
        let mut cache: Vec<_> = self
            .thalia_matching
            .cache
            .iter()
            .map(|(key, value)| {
                (
                    thalia_hash(key),
                    thalia_hash(&value.cached_at),
                    value.cached_at.elapsed() < THALIA_MATCH_CACHE_TTL,
                )
            })
            .collect();
        cache.sort_unstable();
        let mut pending: Vec<_> = self
            .thalia_matching
            .inflight
            .keys()
            .map(thalia_hash)
            .collect();
        pending.sort_unstable();
        thalia_hash(&(self.thalia_matching.generation, cache, pending))
    }

    fn thalia_update_install_plan(&self, mod_entry_id: &str) -> Option<ThaliaInstallPlan> {
        let evidence = build_cached_update_evidence(&self.state.mods, None);
        let recommendation = self
            .thalia_recommendations_from_evidence(&evidence)
            .remove(mod_entry_id)?;
        if recommendation.action != ThaliaRecommendationAction::Update {
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
    ) -> Option<gamebanana::ModFile> {
        if plan.recommendation.action != ThaliaRecommendationAction::Update
            || !plan.matches_target(target, self.active_profile_id(&target.game_id))
        {
            return None;
        }
        let evidence = self.thalia_evidence_for_profile(&target.id, fresh_profile)?;
        let context = self.thalia_context_for_mod_internal(target, &evidence, true)?;
        if !context.same_semantic_context(&plan.context) {
            return None;
        }
        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: plan.recommendation.id,
        };
        validated_recommendation(&context, &response)?;
        let canonical = matcher_adapter::canonicalize_profile(fresh_profile);
        canonical.active.into_iter().find(|file| {
            Some(file.id) == plan.recommendation.id
                && profile_file_is_active_downloadable(&canonical.profile, file)
        })
    }

    fn invalidate_thalia_matching(&mut self) {
        self.thalia_matching.invalidate();
    }

    fn invalidate_thalia_matching_for_game(&mut self, game_id: &str) {
        if self
            .selected_game()
            .is_some_and(|game| game.definition.id == game_id)
        {
            self.invalidate_thalia_matching();
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
            profile: Some(Arc::new(profile)),
            profile_cached_at: None,
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
            },
        );
        assert_eq!(evidence.state, ModUpdateState::UpdateAvailable);
        assert_eq!(evidence.signature.unwrap().files[0].file_id, 88);
        assert_eq!(evidence.newer_files[0].id, 88);
        assert_eq!(evidence.installed_files[0].file_id, 7);
    }

    #[test]
    fn nonreplacement_answers_only_suppress_notice_when_current() {
        for action in [
            ThaliaRecommendationAction::Merge,
            ThaliaRecommendationAction::Uncertain,
        ] {
            let mut evidence = overlay_evidence();
            overlay_thalia_recommendation(
                &mut evidence,
                &ThaliaRecommendation {
                    action,
                    id: (action == ThaliaRecommendationAction::Merge).then_some(88),
                },
            );
            assert_eq!(evidence.state, ModUpdateState::UpToDate);
            assert!(evidence.signature.is_none());
            assert_eq!(evidence.newer_files[0].id, 88);
        }

        let mut current = overlay_evidence();
        overlay_thalia_recommendation(
            &mut current,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Current,
                id: None,
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
        }];
        overlay_thalia_recommendation(
            &mut local_update,
            &ThaliaRecommendation {
                action: ThaliaRecommendationAction::Current,
                id: None,
            },
        );
        assert_eq!(local_update.newer_files[0].id, 99);
    }

    #[test]
    fn local_update_wins_and_archived_ai_target_is_rejected() {
        let answer = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(88),
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
        assert!(state.current_recommendations(&[first]).is_empty());
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
            vec!["update"]
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
            vec!["update"]
        );
        state.cache.get_mut(&contexts[0].key()).unwrap().cached_at =
            Instant::now() - THALIA_MATCH_CACHE_TTL;
        assert!(
            automatic_thalia_update_ids(
                &mods,
                &state.current_recommendations(&contexts),
                &checked,
                active_only
            )
            .is_empty()
        );
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
            cache: HashMap::new(),
            failure_backoff: HashMap::new(),
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
                force_refresh_games: HashSet::new(),
                inflight: HashMap::new(),
                cache: HashMap::new(),
                failure_backoff: HashMap::new(),
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
        };
        assert!(validated_recommendation(&context, &response).is_none());
    }

    #[test]
    fn merge_response_is_kept_separate_from_update() {
        let context = context_for("mod", 1, vec![88]);
        let response = thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Merge,
            id: Some(88),
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
            let response = thalia::UpdateMatchResponse { action, id: None };
            assert!(validated_recommendation(&context, &response).is_some());
        }
    }

    #[test]
    fn current_notice_suppression_requires_the_same_fresh_context() {
        let mut state = test_state();
        let context = context_for("mod", 1, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
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
        assert!(state.current_recommendations(&[context]).is_empty());
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
            },
        );
        state.apply_results(
            vec![ThaliaMatchResult::Completed {
                context: old,
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
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
                },
            );
        }
        let response = || thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
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
            },
        );
        state.inflight.insert(
            second.key(),
            ThaliaInflightRequest {
                request_id: second.request_id,
                context: second.clone(),
                cancellation: Arc::new(AtomicBool::new(false)),
            },
        );
        let response = || thalia::UpdateMatchResponse {
            action: thalia::UpdateMatchAction::Update,
            id: Some(88),
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
        };
        for (context, mod_id) in [(&first, "a"), (&second, "b")] {
            state.cache.insert(
                context.key(),
                ThaliaCachedRecommendation {
                    response: thalia::UpdateMatchResponse {
                        action: thalia::UpdateMatchAction::Update,
                        id: Some(88),
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

        // The second mod is now ignored and therefore absent from the fresh
        // context set. Its old claim must not continue blocking the first one.
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
        };
        let recommendation = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(88),
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
                },
                cached_at: Instant::now(),
            },
        );
        state.recommendations.insert(
            "mod".to_string(),
            ThaliaRecommendation {
                action: ThaliaRecommendationAction::Update,
                id: Some(88),
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
            },
        );
        let context = context_for("mod", 1, vec![88]);
        state.cache.insert(
            context.key(),
            ThaliaCachedRecommendation {
                response: thalia::UpdateMatchResponse {
                    action: thalia::UpdateMatchAction::Current,
                    id: None,
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
}
