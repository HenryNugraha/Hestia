#[derive(Clone)]

struct ThumbnailByteCache {
    inner: Arc<Mutex<ThumbnailByteCacheInner>>,
}

struct ThumbnailByteCacheInner {
    entries: HashMap<String, Arc<Vec<u8>>>,
    order: VecDeque<String>,
}

impl ThumbnailByteCache {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ThumbnailByteCacheInner {
                entries: HashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
            })),
        }
    }

    fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let mut inner = self.inner.lock().ok()?;
        let bytes = inner.entries.get(key)?.clone();
        if let Some(position) = inner.order.iter().position(|existing| existing == key) {
            inner.order.remove(position);
        }
        inner.order.push_back(key.to_string());
        Some(bytes)
    }

    fn insert(&self, key: impl Into<String>, bytes: Vec<u8>) {
        let key = key.into();
        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => return,
        };
        if inner.entries.contains_key(&key) {
            inner.entries.insert(key.clone(), Arc::new(bytes));
            if let Some(position) = inner.order.iter().position(|existing| existing == &key) {
                inner.order.remove(position);
            }
            inner.order.push_back(key);
            return;
        }
        while inner.entries.len() >= THUMBNAIL_BYTE_CACHE_CAPACITY {
            let Some(evicted) = inner.order.pop_front() else { break; };
            inner.entries.remove(&evicted);
        }
        inner.entries.insert(key.clone(), Arc::new(bytes));
        inner.order.push_back(key);
    }
}

#[derive(Clone)]
pub struct RuntimeServices {
    runtime: Arc<tokio::runtime::Runtime>,
    http_client: ClientWithMiddleware,
    full_image_limiter: Arc<Semaphore>,
    thumb_image_limiter: Arc<Semaphore>,
    json_limiter: Arc<Semaphore>,
    full_decode_limiter: Arc<Semaphore>,
    thumbnail_byte_cache: ThumbnailByteCache,
}

impl RuntimeServices {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| anyhow!("failed to create tokio runtime: {err}"))?;
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let http_client = MiddlewareClientBuilder::new(
            reqwest::Client::builder()
                .user_agent(gamebanana::USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|err| anyhow!("failed to create reqwest client: {err}"))?,
        )
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();
        Ok(Self {
            runtime: Arc::new(runtime),
            http_client,
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
}
