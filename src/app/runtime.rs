#[derive(Clone)]

struct ThumbnailByteCache {
    inner: Arc<Mutex<lru::LruCache<String, Arc<Vec<u8>>>>>,
}

impl ThumbnailByteCache {
    fn new(capacity: usize) -> Self {
        use std::num::NonZeroUsize;
        Self {
            inner: Arc::new(Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap()),
            ))),
        }
    }

    fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let mut inner = self.inner.lock().ok()?;
        inner.get(key).cloned()
    }

    fn insert(&self, key: impl Into<String>, bytes: Vec<u8>) {
        let key = key.into();
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => return,
        };
        inner.put(key, Arc::new(bytes));
    }

    fn remove(&self, key: &str) {
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => return,
        };
        inner.pop(key);
    }
}

pub struct RuntimeServices {
    // Only the app instance owns this Arc. Clones captured by Tokio tasks keep a Handle, not
    // the runtime owner, so shutdown cannot occur from one of its own worker threads.
    _runtime_owner: Option<Arc<tokio::runtime::Runtime>>,
    runtime_handle: tokio::runtime::Handle,
    http_client: Arc<RwLock<ClientWithMiddleware>>,
    download_http_client: Arc<RwLock<ClientWithMiddleware>>,
    custom_proxy: Arc<RwLock<Option<CustomProxyConfig>>>,
    full_image_limiter: Arc<Semaphore>,
    thumb_image_limiter: Arc<Semaphore>,
    json_limiter: Arc<Semaphore>,
    full_decode_limiter: Arc<Semaphore>,
    thumbnail_byte_cache: ThumbnailByteCache,
}

impl Clone for RuntimeServices {
    fn clone(&self) -> Self {
        Self {
            _runtime_owner: None,
            runtime_handle: self.runtime_handle.clone(),
            http_client: Arc::clone(&self.http_client),
            download_http_client: Arc::clone(&self.download_http_client),
            custom_proxy: Arc::clone(&self.custom_proxy),
            full_image_limiter: Arc::clone(&self.full_image_limiter),
            thumb_image_limiter: Arc::clone(&self.thumb_image_limiter),
            json_limiter: Arc::clone(&self.json_limiter),
            full_decode_limiter: Arc::clone(&self.full_decode_limiter),
            thumbnail_byte_cache: self.thumbnail_byte_cache.clone(),
        }
    }
}

impl RuntimeServices {
    pub(crate) fn http_client_for(
        custom_proxy: &Option<CustomProxyConfig>,
    ) -> Result<ClientWithMiddleware> {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = MiddlewareClientBuilder::new(
            Self::async_client_builder_for(custom_proxy)
                .user_agent(gamebanana::USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|err| anyhow!("failed to create reqwest client: {err}"))?,
        )
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();
        Ok(client)
    }

    pub(crate) fn download_http_client_for(
        custom_proxy: &Option<CustomProxyConfig>,
    ) -> Result<ClientWithMiddleware> {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = MiddlewareClientBuilder::new(
            Self::async_client_builder_for(custom_proxy)
                .user_agent(gamebanana::USER_AGENT)
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(120))
                .build()
                .map_err(|err| anyhow!("failed to create download client: {err}"))?,
        )
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();
        Ok(client)
    }

    pub fn new(custom_proxy: Option<CustomProxyConfig>) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to create tokio runtime: {err}"))?;
        let runtime = Arc::new(runtime);
        let http_client = Self::http_client_for(&custom_proxy)?;
        let download_http_client = Self::download_http_client_for(&custom_proxy)?;
        Ok(Self {
            _runtime_owner: Some(Arc::clone(&runtime)),
            runtime_handle: runtime.handle().clone(),
            http_client: Arc::new(RwLock::new(http_client)),
            download_http_client: Arc::new(RwLock::new(download_http_client)),
            custom_proxy: Arc::new(RwLock::new(custom_proxy)),
            full_image_limiter: Arc::new(Semaphore::new(FULL_IMAGE_LIMIT)),
            thumb_image_limiter: Arc::new(Semaphore::new(THUMB_IMAGE_LIMIT)),
            json_limiter: Arc::new(Semaphore::new(JSON_LIMIT)),
            full_decode_limiter: Arc::new(Semaphore::new(FULL_IMAGE_DECODE_LIMIT)),
            thumbnail_byte_cache: ThumbnailByteCache::new(THUMBNAIL_BYTE_CACHE_CAPACITY),
        })
    }

    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime_handle.spawn(fut);
    }

    fn handle(&self) -> tokio::runtime::Handle {
        self.runtime_handle.clone()
    }

    pub(crate) fn async_client_builder_for(
        custom_proxy: &Option<CustomProxyConfig>,
    ) -> reqwest::ClientBuilder {
        let builder = reqwest::Client::builder();
        match custom_proxy {
            Some(proxy) => builder.proxy(
                reqwest::Proxy::all(proxy.endpoint())
                    .expect("custom proxy configuration was validated before startup"),
            ),
            None => builder,
        }
    }

    pub(crate) fn async_client_builder(&self) -> reqwest::ClientBuilder {
        let custom_proxy = self.custom_proxy();
        Self::async_client_builder_for(&custom_proxy)
    }

    pub(crate) fn custom_proxy(&self) -> Option<CustomProxyConfig> {
        self.custom_proxy
            .read()
            .ok()
            .and_then(|proxy| proxy.clone())
    }

    pub(crate) fn http_client(&self) -> ClientWithMiddleware {
        self.http_client
            .read()
            .expect("HTTP client lock must not be poisoned")
            .clone()
    }

    pub(crate) fn download_http_client(&self) -> ClientWithMiddleware {
        self.download_http_client
            .read()
            .expect("download HTTP client lock must not be poisoned")
            .clone()
    }

    pub(crate) fn replace_custom_proxy(
        &self,
        custom_proxy: Option<CustomProxyConfig>,
    ) -> Result<()> {
        let http_client = Self::http_client_for(&custom_proxy)?;
        let download_http_client = Self::download_http_client_for(&custom_proxy)?;
        *self
            .http_client
            .write()
            .expect("HTTP client lock must not be poisoned") = http_client;
        *self
            .download_http_client
            .write()
            .expect("download HTTP client lock must not be poisoned") = download_http_client;
        *self
            .custom_proxy
            .write()
            .expect("custom proxy lock must not be poisoned") = custom_proxy;
        Ok(())
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn worker_clones_do_not_own_the_tokio_runtime() {
        let services = RuntimeServices::new(None).unwrap();
        assert!(services._runtime_owner.is_some());
        assert!(services.clone()._runtime_owner.is_none());
    }

    #[test]
    fn download_client_uses_a_separate_configuration() {
        assert!(RuntimeServices::download_http_client_for(&None).is_ok());
    }
}
