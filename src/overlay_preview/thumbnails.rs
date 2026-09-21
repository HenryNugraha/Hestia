//! Bounded, asynchronous thumbnail loading for the native overlay.
//!
//! Image decoding is deliberately kept off the egui frame thread.  The cache
//! only queues paths requested by the current view (and the small set of paths
//! explicitly prefetched by its caller), so opening the overlay does not walk
//! or decode the whole mod library.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
};

const MAX_THUMBNAIL_DIMENSION: u32 = 640;
const MAX_QUEUED_WORK: usize = 48;
const MAX_RESIDENT_TEXTURES: usize = 64;
const MAX_FAILED_PATHS: usize = 256;
const RESULT_CHANNEL_CAPACITY: usize = MAX_QUEUED_WORK;
const MAX_UPLOADS_PER_POLL: usize = 4;

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
    image: Result<DecodedThumbnail, String>,
}

fn decode_thumbnail(path: &Path) -> Result<DecodedThumbnail, String> {
    let image = image::open(path)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?
        .thumbnail(MAX_THUMBNAIL_DIMENSION, MAX_THUMBNAIL_DIMENSION)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(DecodedThumbnail {
        size,
        rgba: image.into_raw(),
    })
}

fn decode_worker(
    queue: Arc<DecodeQueue>,
    results: SyncSender<DecodeResult>,
    repaint_context: Arc<Mutex<Option<egui::Context>>>,
) {
    while let Some(path) = queue.pop_blocking() {
        let image = decode_thumbnail(&path);
        if results.send(DecodeResult { path, image }).is_err() {
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
    Failed,
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
    worker: Option<JoinHandle<()>>,
    entries: HashMap<PathBuf, CacheEntry>,
    resident_lru: VecDeque<PathBuf>,
    failed_lru: VecDeque<PathBuf>,
}

impl ThumbnailCache {
    pub(super) fn new() -> Self {
        let queue = Arc::new(DecodeQueue::new());
        let repaint_context = Arc::new(Mutex::new(None));
        let (results_tx, results_rx) = mpsc::sync_channel(RESULT_CHANNEL_CAPACITY);
        let worker_queue = Arc::clone(&queue);
        let worker_repaint_context = Arc::clone(&repaint_context);
        let worker = thread::Builder::new()
            .name("hestia-overlay-thumbnails".to_owned())
            .spawn(move || decode_worker(worker_queue, results_tx, worker_repaint_context))
            .expect("failed to start overlay thumbnail worker");

        Self {
            queue,
            repaint_context,
            results: Some(results_rx),
            worker: Some(worker),
            entries: HashMap::new(),
            resident_lru: VecDeque::new(),
            failed_lru: VecDeque::new(),
        }
    }

    /// Return a resident texture immediately, or queue a demand decode.
    pub(super) fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        self.set_repaint_context(ctx);
        if let Some(entry) = self.entries.get(path) {
            if let CacheEntry::Texture(texture) = entry {
                let texture = texture.clone();
                self.touch_resident(path);
                return Some(texture);
            }
            let speculative = matches!(entry, CacheEntry::Pending(Priority::Speculative));
            if speculative {
                if self.queue.promote(path) {
                    if let Some(CacheEntry::Pending(priority)) = self.entries.get_mut(path) {
                        *priority = Priority::Demand;
                    }
                }
            }
            return None;
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
            match result.image {
                Ok(decoded) => {
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
                Err(_) => {
                    self.entries.insert(result.path.clone(), CacheEntry::Failed);
                    self.failed_lru.push_back(result.path);
                    self.evict_failed_if_needed();
                }
            }
        }
        if completed {
            ctx.request_repaint();
        }
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
            if matches!(self.entries.get(&path), Some(CacheEntry::Failed)) {
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
        let mut cache = ThumbnailCache {
            queue: Arc::new(DecodeQueue::new()),
            repaint_context: Arc::new(Mutex::new(None)),
            results: None,
            worker: None,
            entries: HashMap::new(),
            resident_lru: VecDeque::new(),
            failed_lru: VecDeque::new(),
        };
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

        let decoded = decode_thumbnail(&path).expect("decode thumbnail test image");
        assert_eq!(decoded.size, [MAX_THUMBNAIL_DIMENSION as usize, 320]);
        assert_eq!(decoded.rgba.len(), 640 * 320 * 4);

        fs::remove_file(path).expect("remove thumbnail test image");
    }
}
