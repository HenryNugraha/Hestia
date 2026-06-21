#[derive(Clone)]

struct ThumbnailByteCache {
    inner: Arc<Mutex<lru::LruCache<String, Arc<Vec<u8>>>>>,
}

impl ThumbnailByteCache {
    fn new(capacity: usize) -> Self {
        use std::num::NonZeroUsize;
        Self {
            inner: Arc::new(Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap())
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
}

#[derive(Clone)]
pub struct RuntimeServices {
    runtime: Arc<tokio::runtime::Runtime>,
    http_client: Arc<RwLock<ClientWithMiddleware>>,
    custom_proxy: Arc<RwLock<Option<CustomProxyConfig>>>,
    full_image_limiter: Arc<Semaphore>,
    thumb_image_limiter: Arc<Semaphore>,
    json_limiter: Arc<Semaphore>,
    full_decode_limiter: Arc<Semaphore>,
    thumbnail_byte_cache: ThumbnailByteCache,
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

    pub fn new(custom_proxy: Option<CustomProxyConfig>) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to create tokio runtime: {err}"))?;
        let http_client = Self::http_client_for(&custom_proxy)?;
        Ok(Self {
            runtime: Arc::new(runtime),
            http_client: Arc::new(RwLock::new(http_client)),
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
        self.runtime.spawn(fut);
    }

    fn handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
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
        self.custom_proxy.read().ok().and_then(|proxy| proxy.clone())
    }

    pub(crate) fn http_client(&self) -> ClientWithMiddleware {
        self.http_client
            .read()
            .expect("HTTP client lock must not be poisoned")
            .clone()
    }

    pub(crate) fn replace_custom_proxy(
        &self,
        custom_proxy: Option<CustomProxyConfig>,
    ) -> Result<()> {
        let http_client = Self::http_client_for(&custom_proxy)?;
        *self
            .http_client
            .write()
            .expect("HTTP client lock must not be poisoned") = http_client;
        *self
            .custom_proxy
            .write()
            .expect("custom proxy lock must not be poisoned") = custom_proxy;
        Ok(())
    }
}
