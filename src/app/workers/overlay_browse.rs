// GameBanana for the in-game overlay: Hestia gets the pages and characters
// the overlay asks for through Browse's caches, then downloads their pictures
// and sends those as they arrive.

/// What the overlay's GameBanana requests need, taken from the app when one
/// arrives.
#[derive(Clone)]
struct OverlayBrowse {
    runtime: RuntimeServices,
    portable: PortablePaths,
    game_id: String,
    outbox: Arc<OverlayOutbox>,
    browse_sort: BrowseSort,
    unsafe_content_mode: UnsafeContentMode,
    cache_limit_bytes: u64,
}

impl OverlayBrowse {
    /// Answers with `ToOverlay::BrowsePage`, then the pictures Hestia hasn't
    /// downloaded yet.
    fn fetch_page(self, request: overlay_protocol::Browse) {
        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            let mut page = overlay_protocol::BrowsePage {
                character: request.character,
                query: request.query.clone(),
                page: request.page,
                ..Default::default()
            };
            let mut missing = Vec::new();
            match self.load_page(&request).await {
                Ok(envelope) => {
                    page.more = !envelope.metadata.is_complete && !envelope.records.is_empty();
                    let hide = matches!(
                        self.unsafe_content_mode,
                        UnsafeContentMode::HideNoCounter | UnsafeContentMode::HideShowCounter
                    );
                    let censor = self.unsafe_content_mode == UnsafeContentMode::Censor;
                    for record in envelope.records {
                        if hide && record.has_content_ratings {
                            continue;
                        }
                        let url = gamebanana::record_thumbnail_image(&record).map(|image| {
                            gamebanana::thumbnail_url(image)
                                .unwrap_or_else(|| gamebanana::full_image_url(image))
                        });
                        let image = downloaded_picture(url.as_deref());
                        if image.is_none()
                            && let Some(url) = url
                        {
                            missing.push((record.id, url));
                        }
                        page.mods.push(overlay_protocol::BrowseMod {
                            id: record.id,
                            name: record.name,
                            image,
                            censored: censor && record.has_content_ratings,
                        });
                    }
                }
                Err(error) => {
                    let error = format!("{error:#}");
                    tracing::warn!(%error, character = request.character, "The overlay couldn't get GameBanana mods");
                    page.error = Some(error);
                }
            }
            self.outbox
                .send(overlay_protocol::ToOverlay::BrowsePage(page));
            self.download_pictures(missing, false).await;
        });
    }

    async fn load_page(
        &self,
        request: &overlay_protocol::Browse,
    ) -> Result<gamebanana::ApiEnvelope<gamebanana::BrowseRecord>> {
        let gamebanana_id = gamebanana::game_id_for_hestia(&self.game_id)
            .ok_or_else(|| anyhow!("unsupported game id: {}", self.game_id))?;
        let query = Some(request.query.trim()).filter(|query| !query.is_empty());
        let page = request.page.max(1) as usize;
        let cache_key = gamebanana::character_browse_page_cache_key(
            &self.game_id,
            request.character,
            query,
            page,
            self.browse_sort,
        );
        // Like Browse: GameBanana first, the saved page when it can't answer.
        let (envelope, warning) = load_browse_page_with_cache(
            &self.portable,
            self.runtime.custom_proxy(),
            gamebanana_id,
            &self.game_id,
            query,
            Some(request.character),
            page,
            self.browse_sort,
            SearchSort::default(),
            true,
            &cache_key,
        )
        .await?;
        if let Some(warning) = warning {
            tracing::debug!(%warning, "The overlay got a saved GameBanana page");
        }
        Ok(envelope)
    }

    /// Answers with `ToOverlay::Characters`, then the icons Hestia hasn't
    /// downloaded yet.
    fn list_characters(self) {
        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            let mut missing = Vec::new();
            let message = match self.load_characters().await {
                Ok(characters) => overlay_protocol::ToOverlay::Characters {
                    characters: characters
                        .into_iter()
                        .filter(|character| !character.is_obsolete)
                        .map(|character| {
                            let url = character.icon_url.filter(|url| !url.trim().is_empty());
                            let image = downloaded_picture(url.as_deref());
                            if image.is_none()
                                && let Some(url) = url
                            {
                                missing.push((character.id, url));
                            }
                            overlay_protocol::Character {
                                id: character.id,
                                name: character.name,
                                image,
                            }
                        })
                        .collect(),
                    error: None,
                },
                Err(error) => {
                    let error = format!("{error:#}");
                    tracing::warn!(%error, "The overlay couldn't get the GameBanana characters");
                    overlay_protocol::ToOverlay::Characters {
                        characters: Vec::new(),
                        error: Some(error),
                    }
                }
            };
            self.outbox.send(message);
            self.download_pictures(missing, true).await;
        });
    }

    async fn load_characters(&self) -> Result<Vec<gamebanana::CharacterCategory>> {
        let super_category = gamebanana::character_super_category_id_for_hestia(&self.game_id)
            .ok_or_else(|| anyhow!("no GameBanana characters for {}", self.game_id))?;
        let cache_key = gamebanana::character_categories_cache_key(&self.game_id, super_category);
        let (characters, _) = load_character_categories_with_cache(
            &self.portable,
            self.runtime.custom_proxy(),
            super_category,
            false,
            &cache_key,
        )
        .await?;
        Ok(characters)
    }

    /// Downloads pictures into Browse's image cache, and sends each one as it
    /// lands.
    async fn download_pictures(&self, missing: Vec<(u64, String)>, characters: bool) {
        let mut downloads = tokio::task::JoinSet::new();
        for (id, url) in missing {
            let client = self.runtime.http_client();
            let limiter = Arc::clone(&self.runtime.thumb_image_limiter);
            let portable = self.portable.clone();
            let limit = self.cache_limit_bytes;
            downloads.spawn(async move {
                let _permit = limiter.acquire().await.ok();
                let bytes = client
                    .get(&url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await?
                    .to_vec();
                image::guess_format(&bytes)?;
                let key = HestiaApp::browse_image_cache_key(&url);
                let path = persistence::cache_file_path(&key);
                tokio::task::spawn_blocking(move || {
                    persistence::cache_put(&portable, &key, "browse-img", &bytes, limit)
                })
                .await??;
                Ok::<_, anyhow::Error>((id, path))
            });
        }
        while let Some(done) = downloads.join_next().await {
            match done {
                Ok(Ok((id, path))) => {
                    let mut pictures = overlay_protocol::Pictures::default();
                    if characters {
                        pictures.characters.push((id, path));
                    } else {
                        pictures.mods.push((id, path));
                    }
                    self.outbox.add_pictures(pictures);
                }
                Ok(Err(error)) => {
                    tracing::debug!(error = %format!("{error:#}"), "An overlay picture failed");
                }
                Err(_) => {}
            }
        }
    }
}

/// Where Browse saved the picture at `url`, if it has.
fn downloaded_picture(url: Option<&str>) -> Option<PathBuf> {
    url.map(|url| persistence::cache_file_path(&HestiaApp::browse_image_cache_key(url)))
        .filter(|path| path.is_file())
}
