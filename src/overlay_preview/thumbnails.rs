//! Bounded, asynchronous thumbnail loading for the native overlay.
//!
//! Image decoding is deliberately kept off the egui frame thread.  The cache
//! only queues paths requested by the current view (and the small set of paths
//! explicitly prefetched by its caller), so opening the overlay does not walk
//! or decode the whole mod library.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_THUMBNAIL_DIMENSION: u32 = 640;
const MAX_QUEUED_WORK: usize = 48;
const MAX_RESIDENT_TEXTURES: usize = 64;
const MAX_FAILED_PATHS: usize = 256;
const RESULT_CHANNEL_CAPACITY: usize = MAX_QUEUED_WORK;
const MAX_UPLOADS_PER_POLL: usize = 4;

/// A picture that failed loads again after this long, twice as long after
/// each failure in a row, up to `MAX_RETRY_DELAY`.  A failure is often over a
/// moment later: a mod's folder moves while it turns on or off, and Hestia
/// may still be saving a picture.
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Priority {
    Demand,
    Speculative,
}

#[derive(Debug, Eq, PartialEq)]
enum QueuePush {
    Rejected,
    Accepted { evicted: Option<PathBuf> },
}

#[derive(Debug)]
struct DecodeQueueState {
    demand: VecDeque<PathBuf>,
    speculative: VecDeque<PathBuf>,
    stopped: bool,
}

struct DecodeQueue {
    state: Mutex<DecodeQueueState>,
    wake: Condvar,
}

impl DecodeQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(DecodeQueueState {
                demand: VecDeque::new(),
                speculative: VecDeque::new(),
                stopped: false,
            }),
            wake: Condvar::new(),
        }
    }

    /// Add work without waiting on the UI thread. Demand evicts the oldest
    /// speculative item when the bounded queue is full; speculative work is
    /// dropped when it cannot fit.
    fn push(&self, path: PathBuf, priority: Priority) -> QueuePush {
        let mut state = self.state.lock().expect("thumbnail queue lock poisoned");
        if state.stopped
            || state.demand.iter().any(|queued| queued == &path)
            || state.speculative.iter().any(|queued| queued == &path)
        {
            return QueuePush::Rejected;
        }

        let queued = state.demand.len() + state.speculative.len();
        let mut evicted = None;
        if queued >= MAX_QUEUED_WORK {
            match priority {
                Priority::Demand => {
                    if let Some(path) = state.speculative.pop_front() {
                        evicted = Some(path);
                    } else {
                        return QueuePush::Rejected;
                    }
                }
                Priority::Speculative => return QueuePush::Rejected,
            }
        }

        match priority {
            Priority::Demand => state.demand.push_back(path),
            Priority::Speculative => state.speculative.push_back(path),
        }
        self.wake.notify_one();
        QueuePush::Accepted { evicted }
    }

    /// Promote queued speculative work when the card becomes visible. Work
    /// already being decoded cannot be reprioritized, but it is already the
    /// next and only active decode.
    fn promote(&self, path: &Path) -> bool {
        let mut state = self.state.lock().expect("thumbnail queue lock poisoned");
        let Some(index) = state.speculative.iter().position(|queued| queued == path) else {
            return false;
        };
        let promoted = state
            .speculative
            .remove(index)
            .expect("thumbnail queue index was found");
        state.demand.push_back(promoted);
        self.wake.notify_one();
        true
    }

    fn pop_blocking(&self) -> Option<PathBuf> {
        let mut state = self.state.lock().expect("thumbnail queue lock poisoned");
        loop {
            if state.stopped {
                return None;
            }
            if let Some(path) = state.demand.pop_front() {
                return Some(path);
            }
            if let Some(path) = state.speculative.pop_front() {
                return Some(path);
            }
            state = self
                .wake
                .wait(state)
                .expect("thumbnail queue lock poisoned while waiting");
        }
    }

    fn stop(&self) {
        let mut state = self.state.lock().expect("thumbnail queue lock poisoned");
        state.stopped = true;
        state.demand.clear();
        state.speculative.clear();
        self.wake.notify_all();
    }
}

#[derive(Debug)]
struct DecodedThumbnail {
    size: [usize; 2],
    rgba: Vec<u8>,
}

struct DecodeResult {
    path: PathBuf,
    censored: bool,
    image: Result<DecodedThumbnail, String>,
}

fn decode_thumbnail(path: &Path, censored: bool) -> Result<DecodedThumbnail, String> {
    // Hestia's downloaded pictures are `.bin` files, so read the format from
    // the contents rather than the extension.
    let image = image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?
        .thumbnail(MAX_THUMBNAIL_DIMENSION, MAX_THUMBNAIL_DIMENSION)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    if censored {
        // The same tiny blurred copy the library draws for a censored card.
        let copy = crate::app::censor_copy(&egui::ColorImage::from_rgba_unmultiplied(
            size,
            image.as_raw(),
        ));
        return Ok(DecodedThumbnail {
            size: copy.size,
            rgba: copy
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                .collect(),
        });
    }
    Ok(DecodedThumbnail {
        size,
        rgba: image.into_raw(),
    })
}

fn decode_worker(
    queue: Arc<DecodeQueue>,
    results: SyncSender<DecodeResult>,
    repaint_context: Arc<Mutex<Option<egui::Context>>>,
    censored: Arc<Mutex<HashSet<PathBuf>>>,
    decode: impl Fn(&Path, bool) -> Result<DecodedThumbnail, String>,
) {
    while let Some(path) = queue.pop_blocking() {
        let censored = censored
            .lock()
            .expect("thumbnail censor lock poisoned")
            .contains(&path);
        // A picture that crashes its decoder fails alone, instead of ending
        // the worker and every picture after it.
        let image =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode(&path, censored)))
                .unwrap_or_else(|_| Err(format!("decoding {} crashed", path.display())));
        if results
            .send(DecodeResult {
                path,
                censored,
                image,
            })
            .is_err()
        {
            break;
        }
        if let Some(ctx) = repaint_context
            .lock()
            .expect("thumbnail repaint context lock poisoned")
            .as_ref()
            .cloned()
        {
            ctx.request_repaint();
        }
    }
}

enum CacheEntry {
    Pending(Priority),
    Texture(egui::TextureHandle),
    /// Loads again when it's drawn after `retry_at`.
    Failed {
        retry_at: Instant,
    },
}

/// How long a picture waits to load again after `failures` failures in a row.
fn retry_delay(failures: u32) -> Duration {
    FIRST_RETRY_DELAY
        .saturating_mul(1 << failures.saturating_sub(1).min(16))
        .min(MAX_RETRY_DELAY)
}

/// A small UI-thread cache backed by one bounded decode worker.
///
/// `get` never waits for disk or image decoding. Call `poll` once per frame
/// before drawing cards, and use `prefetch` with only the nearby cards that
/// are likely to become visible next.
pub(super) struct ThumbnailCache {
    queue: Arc<DecodeQueue>,
    repaint_context: Arc<Mutex<Option<egui::Context>>>,
    results: Option<Receiver<DecodeResult>>,
    censored: Arc<Mutex<HashSet<PathBuf>>>,
    worker: Option<JoinHandle<()>>,
    entries: HashMap<PathBuf, CacheEntry>,
    resident_lru: VecDeque<PathBuf>,
    failed_lru: VecDeque<PathBuf>,
    /// Failures in a row, by picture.
    failures: HashMap<PathBuf, u32>,
}

impl ThumbnailCache {
    pub(super) fn new() -> Self {
        let queue = Arc::new(DecodeQueue::new());
        let repaint_context = Arc::new(Mutex::new(None));
        let (results_tx, results_rx) = mpsc::sync_channel(RESULT_CHANNEL_CAPACITY);
        let worker_queue = Arc::clone(&queue);
        let worker_repaint_context = Arc::clone(&repaint_context);
        let censored = Arc::new(Mutex::new(HashSet::new()));
        let worker_censored = Arc::clone(&censored);
        let worker = thread::Builder::new()
            .name("hestia-overlay-thumbnails".to_owned())
            .spawn(move || {
                decode_worker(
                    worker_queue,
                    results_tx,
                    worker_repaint_context,
                    worker_censored,
                    decode_thumbnail,
                )
            })
            .expect("failed to start overlay thumbnail worker");

        Self {
            queue,
            repaint_context,
            results: Some(results_rx),
            censored,
            worker: Some(worker),
            entries: HashMap::new(),
            resident_lru: VecDeque::new(),
            failed_lru: VecDeque::new(),
            failures: HashMap::new(),
        }
    }

    /// Return a resident texture immediately, or queue a demand decode.
    pub(super) fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        self.set_repaint_context(ctx);
        match self.entries.get(path) {
            Some(CacheEntry::Texture(texture)) => {
                let texture = texture.clone();
                self.touch_resident(path);
                return Some(texture);
            }
            Some(CacheEntry::Pending(Priority::Speculative)) => {
                if self.queue.promote(path)
                    && let Some(CacheEntry::Pending(priority)) = self.entries.get_mut(path)
                {
                    *priority = Priority::Demand;
                }
                return None;
            }
            Some(CacheEntry::Pending(Priority::Demand)) => return None,
            Some(&CacheEntry::Failed { retry_at }) => {
                let now = Instant::now();
                if now < retry_at {
                    // Draw again when it's due, even if nothing else moves.
                    ctx.request_repaint_after(retry_at - now);
                    return None;
                }
            }
            None => {}
        }

        let path = path.to_path_buf();
        if let QueuePush::Accepted { evicted } = self.queue.push(path.clone(), Priority::Demand) {
            self.remove_evicted_pending(evicted);
            self.entries
                .insert(path, CacheEntry::Pending(Priority::Demand));
        }
        None
    }

    /// Queue nearby paths at a lower priority than visible card requests.
    pub(super) fn prefetch<I>(&mut self, ctx: &egui::Context, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        self.set_repaint_context(ctx);
        for path in paths {
            if self.entries.contains_key(&path) {
                continue;
            }
            if let QueuePush::Accepted { evicted } =
                self.queue.push(path.clone(), Priority::Speculative)
            {
                self.remove_evicted_pending(evicted);
                self.entries
                    .insert(path, CacheEntry::Pending(Priority::Speculative));
            }
        }
    }

    /// Drain completed decodes and upload them to egui. This is intentionally
    /// separate from `get` so callers can control when the UI thread performs
    /// the small GPU texture upload.
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.set_repaint_context(ctx);
        let mut completed = false;
        let mut processed = 0;
        loop {
            if processed >= MAX_UPLOADS_PER_POLL {
                ctx.request_repaint();
                break;
            }
            let result = match self.results.as_ref() {
                Some(results) => results.try_recv(),
                None => return,
            };
            let result = match result {
                Ok(result) => result,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            };
            completed = true;
            processed += 1;
            if result.censored != self.is_censored(&result.path) {
                // The library changed while it loaded.  The next frame asks again.
                self.entries.remove(&result.path);
                continue;
            }
            match result.image {
                Ok(decoded) => {
                    self.failures.remove(&result.path);
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        decoded.size,
                        decoded.rgba.as_slice(),
                    );
                    let texture = ctx.load_texture(
                        texture_name(&result.path),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.entries
                        .insert(result.path.clone(), CacheEntry::Texture(texture));
                    self.touch_resident(&result.path);
                    self.evict_resident_if_needed();
                }
                Err(error) => {
                    let failures = self.failures.entry(result.path.clone()).or_insert(0);
                    *failures += 1;
                    if *failures == 1 {
                        tracing::debug!(%error, "An overlay picture didn't load");
                    }
                    let retry_at = Instant::now() + retry_delay(*failures);
                    self.entries
                        .insert(result.path.clone(), CacheEntry::Failed { retry_at });
                    remove_path(&mut self.failed_lru, &result.path);
                    self.failed_lru.push_back(result.path);
                    self.evict_failed_if_needed();
                }
            }
        }
        if completed {
            ctx.request_repaint();
        }
    }

    /// Loads the pictures that failed again the next time they're drawn.
    /// Hestia sends a new library or pictures after files changed.
    pub(super) fn retry_failed(&mut self) {
        self.entries
            .retain(|_, entry| !matches!(entry, CacheEntry::Failed { .. }));
        self.failed_lru.clear();
        self.failures.clear();
    }

    /// The pictures to blur and darken, like the library's censored cards.
    /// Pictures that were censored and no longer are, or the other way, load
    /// again.
    pub(super) fn set_censored(&mut self, paths: HashSet<PathBuf>) {
        let changed: Vec<PathBuf> = {
            let mut censored = self
                .censored
                .lock()
                .expect("thumbnail censor lock poisoned");
            if *censored == paths {
                return;
            }
            let changed = censored.symmetric_difference(&paths).cloned().collect();
            *censored = paths;
            changed
        };
        for path in changed {
            if matches!(
                self.entries.get(&path),
                Some(CacheEntry::Texture(_) | CacheEntry::Failed { .. })
            ) {
                self.entries.remove(&path);
            }
        }
    }

    fn is_censored(&self, path: &Path) -> bool {
        self.censored
            .lock()
            .expect("thumbnail censor lock poisoned")
            .contains(path)
    }

    fn remove_evicted_pending(&mut self, evicted: Option<PathBuf>) {
        let Some(evicted) = evicted else {
            return;
        };
        if matches!(self.entries.get(&evicted), Some(CacheEntry::Pending(_))) {
            self.entries.remove(&evicted);
        }
    }

    fn set_repaint_context(&self, ctx: &egui::Context) {
        let mut repaint_context = self
            .repaint_context
            .lock()
            .expect("thumbnail repaint context lock poisoned");
        *repaint_context = Some(ctx.clone());
    }

    fn touch_resident(&mut self, path: &Path) {
        remove_path(&mut self.resident_lru, path);
        self.resident_lru.push_back(path.to_path_buf());
    }

    fn evict_resident_if_needed(&mut self) {
        while self.resident_lru.len() > MAX_RESIDENT_TEXTURES {
            let Some(path) = self.resident_lru.pop_front() else {
                break;
            };
            if matches!(self.entries.get(&path), Some(CacheEntry::Texture(_))) {
                self.entries.remove(&path);
            }
        }
    }

    fn evict_failed_if_needed(&mut self) {
        while self.failed_lru.len() > MAX_FAILED_PATHS {
            let Some(path) = self.failed_lru.pop_front() else {
                break;
            };
            self.failures.remove(&path);
            if matches!(self.entries.get(&path), Some(CacheEntry::Failed { .. })) {
                self.entries.remove(&path);
            }
        }
    }
}

fn remove_path(paths: &mut VecDeque<PathBuf>, path: &Path) {
    if let Some(index) = paths.iter().position(|queued| queued == path) {
        paths.remove(index);
    }
}

fn texture_name(path: &Path) -> String {
    format!("hestia-overlay-thumbnail:{}", path.display())
}

impl Drop for ThumbnailCache {
    fn drop(&mut self) {
        self.queue.stop();
        // Dropping the receiver first lets a worker blocked on a full result
        // channel exit promptly instead of making cache destruction wait for
        // the UI to poll another frame.
        self.results.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// A cache whose decodes the test does itself, with `results`.
    fn cache_without_worker(results: Option<Receiver<DecodeResult>>) -> ThumbnailCache {
        ThumbnailCache {
            queue: Arc::new(DecodeQueue::new()),
            repaint_context: Arc::new(Mutex::new(None)),
            results,
            censored: Arc::new(Mutex::new(HashSet::new())),
            worker: None,
            entries: HashMap::new(),
            resident_lru: VecDeque::new(),
            failed_lru: VecDeque::new(),
            failures: HashMap::new(),
        }
    }

    fn queued(cache: &ThumbnailCache) -> usize {
        let state = cache
            .queue
            .state
            .lock()
            .expect("thumbnail queue lock poisoned");
        state.demand.len() + state.speculative.len()
    }

    fn make_due(cache: &mut ThumbnailCache, path: &Path) {
        let Some(CacheEntry::Failed { retry_at }) = cache.entries.get_mut(path) else {
            panic!("{} didn't fail", path.display());
        };
        *retry_at = Instant::now();
    }

    #[test]
    fn demand_replaces_oldest_speculative_work_and_wins_next_pop() {
        let queue = DecodeQueue::new();
        for index in 0..MAX_QUEUED_WORK {
            assert_eq!(
                queue.push(
                    PathBuf::from(format!("speculative-{index}")),
                    Priority::Speculative
                ),
                QueuePush::Accepted { evicted: None }
            );
        }
        assert_eq!(
            queue.push(PathBuf::from("speculative-overflow"), Priority::Speculative),
            QueuePush::Rejected
        );
        assert_eq!(
            queue.push(PathBuf::from("visible-now"), Priority::Demand),
            QueuePush::Accepted {
                evicted: Some(PathBuf::from("speculative-0"))
            }
        );

        assert_eq!(queue.pop_blocking(), Some(PathBuf::from("visible-now")));
        let state = queue.state.lock().expect("thumbnail queue lock poisoned");
        assert_eq!(
            state.demand.len() + state.speculative.len(),
            MAX_QUEUED_WORK - 1
        );
        assert!(
            !state
                .speculative
                .iter()
                .any(|path| path == Path::new("speculative-0"))
        );
    }

    #[test]
    fn queued_speculative_work_can_be_promoted_without_duplication() {
        let queue = DecodeQueue::new();
        let path = PathBuf::from("nearby-card");
        assert_eq!(
            queue.push(path.clone(), Priority::Speculative),
            QueuePush::Accepted { evicted: None }
        );
        assert!(queue.promote(&path));
        assert!(!queue.promote(&path));
        assert_eq!(queue.pop_blocking(), Some(path));
    }

    #[test]
    fn evicted_speculative_entry_can_be_requeued_on_revisit() {
        // Pause decoding by constructing the cache without a worker. This
        // exercises saturation deterministically through the public API.
        let mut cache = cache_without_worker(None);
        let ctx = egui::Context::default();
        cache.prefetch(
            &ctx,
            (0..MAX_QUEUED_WORK).map(|index| PathBuf::from(format!("nearby-{index}"))),
        );
        assert!(cache.get(&ctx, Path::new("visible-now")).is_none());
        assert!(!cache.entries.contains_key(Path::new("nearby-0")));
        // Scrolling back to the evicted image must submit a fresh demand job,
        // not leave the card permanently waiting on a discarded request.
        assert!(cache.get(&ctx, Path::new("nearby-0")).is_none());
        assert_eq!(
            cache.queue.pop_blocking(),
            Some(PathBuf::from("visible-now"))
        );
        assert_eq!(cache.queue.pop_blocking(), Some(PathBuf::from("nearby-0")));
    }

    #[test]
    fn decode_worker_resizes_large_source_to_bounded_thumbnail() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hestia-overlay-thumbnail-test-{}-{suffix}.png",
            std::process::id()
        ));
        let source = image::RgbaImage::from_pixel(1200, 600, image::Rgba([12, 34, 56, 255]));
        source.save(&path).expect("write thumbnail test image");

        let decoded = decode_thumbnail(&path, false).expect("decode thumbnail test image");
        assert_eq!(decoded.size, [MAX_THUMBNAIL_DIMENSION as usize, 320]);
        assert_eq!(decoded.rgba.len(), 640 * 320 * 4);

        fs::remove_file(path).expect("remove thumbnail test image");
    }

    #[test]
    fn downloaded_pictures_decode_without_an_image_extension() {
        let directory = tempfile::tempdir().expect("create test directory");
        let png = directory.path().join("picture.png");
        image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]))
            .save(&png)
            .expect("write test image");
        let cached = directory.path().join("0123456789abcdef.bin");
        fs::rename(&png, &cached).expect("rename test image");

        let decoded = decode_thumbnail(&cached, false).expect("decode cached picture");
        // Scaled to the thumbnail size, like every picture.
        let width = MAX_THUMBNAIL_DIMENSION as usize;
        assert_eq!(decoded.size, [width, width / 2]);
        assert!(decode_thumbnail(&directory.path().join("missing.bin"), false).is_err());
    }

    #[test]
    fn censored_pictures_load_as_the_librarys_blurred_copy() {
        let directory = tempfile::tempdir().expect("create test directory");
        let path = directory.path().join("picture.png");
        image::RgbaImage::from_pixel(1200, 600, image::Rgba([255, 0, 128, 255]))
            .save(&path)
            .expect("write test image");

        let decoded = decode_thumbnail(&path, true).expect("decode censored picture");
        let expected = crate::app::censor_copy(&egui::ColorImage::new(
            [640, 320],
            vec![egui::Color32::from_rgb(255, 0, 128); 640 * 320],
        ));
        assert_eq!(decoded.size, expected.size);
        // Darker, but not black.
        assert_eq!(&decoded.rgba[..4], &[112, 7, 60, 255]);
    }

    #[test]
    fn changing_what_is_censored_loads_those_pictures_again() {
        let mut cache = cache_without_worker(None);
        let ctx = egui::Context::default();
        let kept = PathBuf::from("kept.png");
        let flipped = PathBuf::from("flipped.png");
        for path in [&kept, &flipped] {
            let texture = ctx.load_texture(
                texture_name(path),
                egui::ColorImage::from_rgba_unmultiplied([1, 1], &[255; 4]),
                egui::TextureOptions::LINEAR,
            );
            cache
                .entries
                .insert(path.clone(), CacheEntry::Texture(texture));
        }
        cache.set_censored(HashSet::from([flipped.clone()]));
        assert!(cache.is_censored(&flipped));
        assert!(cache.entries.contains_key(&kept));
        assert!(!cache.entries.contains_key(&flipped));
    }

    #[test]
    fn failed_pictures_wait_longer_after_each_failure() {
        assert_eq!(retry_delay(1), Duration::from_secs(2));
        assert_eq!(retry_delay(2), Duration::from_secs(4));
        assert_eq!(retry_delay(3), Duration::from_secs(8));
        assert_eq!(retry_delay(6), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(u32::MAX), MAX_RETRY_DELAY);
    }

    #[test]
    fn a_failed_picture_loads_again_once_its_wait_is_over() {
        let (results, receiver) = mpsc::sync_channel(RESULT_CHANNEL_CAPACITY);
        let mut cache = cache_without_worker(Some(receiver));
        let ctx = egui::Context::default();
        let path = PathBuf::from("mod-turning-on/preview.png");
        let decoded = |image| DecodeResult {
            path: path.clone(),
            censored: false,
            image,
        };

        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(cache.queue.pop_blocking(), Some(path.clone()));
        results
            .send(decoded(Err("the folder is moving".to_owned())))
            .unwrap();
        cache.poll(&ctx);
        // Drawn again while it waits, it doesn't load.
        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(queued(&cache), 0);

        make_due(&mut cache, &path);
        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(cache.queue.pop_blocking(), Some(path.clone()));
        results
            .send(decoded(Err("the folder is moving".to_owned())))
            .unwrap();
        cache.poll(&ctx);
        assert_eq!(cache.failures.get(&path), Some(&2));

        make_due(&mut cache, &path);
        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(cache.queue.pop_blocking(), Some(path.clone()));
        results
            .send(decoded(Ok(DecodedThumbnail {
                size: [1, 1],
                rgba: vec![255; 4],
            })))
            .unwrap();
        cache.poll(&ctx);
        assert!(cache.get(&ctx, &path).is_some());
        assert!(cache.failures.is_empty());
    }

    #[test]
    fn new_files_from_hestia_load_failed_pictures_right_away() {
        let mut cache = cache_without_worker(None);
        let ctx = egui::Context::default();
        let path = PathBuf::from("gamebanana.bin");
        cache.entries.insert(
            path.clone(),
            CacheEntry::Failed {
                retry_at: Instant::now() + MAX_RETRY_DELAY,
            },
        );
        cache.failures.insert(path.clone(), 5);
        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(queued(&cache), 0);

        cache.retry_failed();
        assert!(cache.failures.is_empty());
        assert!(cache.get(&ctx, &path).is_none());
        assert_eq!(cache.queue.pop_blocking(), Some(path));
    }

    #[test]
    fn a_picture_that_crashes_its_decoder_fails_alone() {
        let queue = Arc::new(DecodeQueue::new());
        let (results, receiver) = mpsc::sync_channel(RESULT_CHANNEL_CAPACITY);
        let worker_queue = Arc::clone(&queue);
        let worker = thread::spawn(move || {
            decode_worker(
                worker_queue,
                results,
                Arc::new(Mutex::new(None)),
                Arc::new(Mutex::new(HashSet::new())),
                |path, _| {
                    assert!(path != Path::new("broken.png"), "a decoder bug");
                    Ok(DecodedThumbnail {
                        size: [1, 1],
                        rgba: vec![255; 4],
                    })
                },
            );
        });
        for path in ["broken.png", "fine.png"] {
            queue.push(PathBuf::from(path), Priority::Demand);
        }
        let wait = Duration::from_secs(10);
        let broken = receiver
            .recv_timeout(wait)
            .expect("the broken picture's result");
        assert_eq!(broken.path, Path::new("broken.png"));
        assert!(broken.image.unwrap_err().contains("crashed"));
        let fine = receiver
            .recv_timeout(wait)
            .expect("the next picture's result");
        assert_eq!(fine.path, Path::new("fine.png"));
        assert!(fine.image.is_ok());
        queue.stop();
        worker.join().expect("the worker ends normally");
    }
}
