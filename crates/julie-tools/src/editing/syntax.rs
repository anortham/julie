use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub use julie_extractors::syntax::ParsedSource;

#[derive(Debug, Clone)]
pub struct SyntaxConfig {
    pub max_source_bytes: usize,
    pub default_timeout: Duration,
    pub max_concurrent_parses: usize,
}

impl Default for SyntaxConfig {
    fn default() -> Self {
        let max_source_bytes = std::env::var("JULIE_MAX_EDIT_SOURCE_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(16 * 1024 * 1024)
            .clamp(1, 256 * 1024 * 1024);

        Self {
            max_source_bytes,
            default_timeout: Duration::from_secs(10),
            max_concurrent_parses: 4,
        }
    }
}

impl SyntaxConfig {
    pub fn validate(&self) -> Result<(), SyntaxAdapterError> {
        let max_b = (u32::MAX - 1) as usize;
        if self.max_source_bytes == 0 || self.max_source_bytes > max_b {
            let m = format!(
                "max_source_bytes must be 1..={max_b}, got {}",
                self.max_source_bytes
            );
            return Err(SyntaxAdapterError::InvalidConfiguration(m));
        }
        if self.default_timeout == Duration::ZERO {
            let m = "default_timeout must be > 0".to_string();
            return Err(SyntaxAdapterError::InvalidConfiguration(m));
        }
        if self.max_concurrent_parses == 0 {
            let m = "max_concurrent_parses must be > 0".to_string();
            return Err(SyntaxAdapterError::InvalidConfiguration(m));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SyntaxAdapterError {
    UnsupportedLanguage { path: PathBuf },
    UnsupportedContainer { path: PathBuf },
    InputTooLarge { bytes: usize },
    Cancelled,
    DeadlineExceeded,
    ParseFailed { source: anyhow::Error },
    InvalidConfiguration(String),
    WorkerPanic,
}

impl SyntaxAdapterError {
    pub fn error_kind(&self) -> &'static str {
        match self {
            Self::UnsupportedLanguage { .. } => "unsupported_language",
            Self::UnsupportedContainer { .. } => "unsupported_container",
            Self::InputTooLarge { .. } => "input_too_large",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::ParseFailed { .. } => "parse_failed",
            Self::InvalidConfiguration(_) => "invalid_configuration",
            Self::WorkerPanic => "worker_panic",
        }
    }

    pub fn is_unsupported_language(&self) -> bool {
        matches!(self, Self::UnsupportedLanguage { .. })
    }
}

impl From<julie_extractors::syntax::SyntaxError> for SyntaxAdapterError {
    fn from(err: julie_extractors::syntax::SyntaxError) -> Self {
        use julie_extractors::syntax::SyntaxError as SE;
        match err {
            SE::UnsupportedLanguage { path } => Self::UnsupportedLanguage { path },
            SE::UnsupportedContainer { path } => Self::UnsupportedContainer { path },
            SE::InputTooLarge { bytes } => Self::InputTooLarge { bytes },
            SE::Cancelled => Self::Cancelled,
            SE::DeadlineExceeded => Self::DeadlineExceeded,
            SE::ParseFailed { source } => Self::ParseFailed { source },
        }
    }
}

impl std::fmt::Display for SyntaxAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedLanguage { path } => {
                write!(f, "unsupported language: {}", path.display())
            }
            Self::UnsupportedContainer { path } => {
                write!(f, "unsupported container: {}", path.display())
            }
            Self::InputTooLarge { bytes } => write!(f, "input too large: {bytes} bytes"),
            Self::Cancelled => write!(f, "syntax parsing was cancelled"),
            Self::DeadlineExceeded => write!(f, "syntax parsing deadline exceeded"),
            Self::ParseFailed { source } => write!(f, "syntax parse failed: {source}"),
            Self::InvalidConfiguration(msg) => write!(f, "invalid configuration: {msg}"),
            Self::WorkerPanic => write!(f, "syntax parser worker panicked"),
        }
    }
}

impl std::error::Error for SyntaxAdapterError {}

struct LimiterState {
    available: usize,
    active_cancelled: Vec<std::thread::JoinHandle<()>>,
}

pub struct SyntaxLimiter {
    state: Mutex<LimiterState>,
    cvar: Condvar,
}

impl SyntaxLimiter {
    pub fn new(max_permits: usize) -> Self {
        Self {
            state: Mutex::new(LimiterState {
                available: max_permits,
                active_cancelled: Vec::new(),
            }),
            cvar: Condvar::new(),
        }
    }

    fn reap_finished_cancelled(state: &mut LimiterState) {
        let mut i = 0;
        while i < state.active_cancelled.len() {
            if state.active_cancelled[i].is_finished() {
                let worker = state.active_cancelled.swap_remove(i);
                let _ = worker.join();
            } else {
                i += 1;
            }
        }
    }

    pub fn acquire(
        &self,
        deadline: Option<Instant>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<(), SyntaxAdapterError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SyntaxAdapterError::WorkerPanic)?;
        Self::reap_finished_cancelled(&mut state);

        while state.available == 0 {
            if cancelled.is_some_and(|c| c.load(Ordering::Acquire)) {
                return Err(SyntaxAdapterError::Cancelled);
            }
            let timeout = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return Err(SyntaxAdapterError::DeadlineExceeded);
                    }
                    d.duration_since(now).min(Duration::from_millis(50))
                }
                None => Duration::from_millis(50),
            };
            let (next_state, _) = self
                .cvar
                .wait_timeout(state, timeout)
                .map_err(|_| SyntaxAdapterError::WorkerPanic)?;
            state = next_state;
            Self::reap_finished_cancelled(&mut state);
        }

        state.available -= 1;
        Ok(())
    }

    pub fn release(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.available += 1;
            self.cvar.notify_one();
        }
    }

    pub fn register_cancelled(limiter: Arc<Self>, join_handle: std::thread::JoinHandle<()>) {
        let handle_cell = Arc::new(Mutex::new(Some(join_handle)));
        let handle_for_spawn = Arc::clone(&handle_cell);
        let limiter_clone = Arc::clone(&limiter);
        let spawn_res = std::thread::Builder::new()
            .name("syntax-parser-cleanup".to_string())
            .spawn(move || {
                let h = handle_for_spawn.lock().ok().and_then(|mut g| g.take());
                if let Some(h) = h {
                    let _ = h.join();
                }
                limiter_clone.release();
            });

        match spawn_res {
            Ok(cleanup_handle) => {
                if let Ok(mut state) = limiter.state.lock() {
                    state.active_cancelled.push(cleanup_handle);
                }
            }
            Err(_) => {
                let h = handle_cell.lock().ok().and_then(|mut g| g.take());
                if let Some(h) = h {
                    let _ = h.join();
                }
                limiter.release();
            }
        }
    }

    pub fn drain_cancelled(&self) {
        let mut workers = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            workers = std::mem::take(&mut state.active_cancelled);
        }
        for worker in workers {
            let _ = worker.join();
        }
    }

    pub fn available_permits(&self) -> usize {
        self.state.lock().map(|s| s.available).unwrap_or(0)
    }
}

static GLOBAL_LIMITER: std::sync::OnceLock<Arc<SyntaxLimiter>> = std::sync::OnceLock::new();

pub fn shared_limiter() -> Arc<SyntaxLimiter> {
    GLOBAL_LIMITER
        .get_or_init(|| Arc::new(SyntaxLimiter::new(4)))
        .clone()
}

#[derive(Clone)]
pub struct SyntaxAdapter {
    config: SyntaxConfig,
    limiter: Arc<SyntaxLimiter>,
}

impl Default for SyntaxAdapter {
    fn default() -> Self {
        Self {
            config: SyntaxConfig::default(),
            limiter: shared_limiter(),
        }
    }
}

impl SyntaxAdapter {
    pub fn new(config: SyntaxConfig) -> Result<Self, SyntaxAdapterError> {
        config.validate()?;
        let limiter = Arc::new(SyntaxLimiter::new(config.max_concurrent_parses));
        Ok(Self { config, limiter })
    }

    pub fn with_shared_limiter(config: SyntaxConfig) -> Result<Self, SyntaxAdapterError> {
        config.validate()?;
        Ok(Self {
            config,
            limiter: shared_limiter(),
        })
    }

    pub fn drain_cancelled_workers(&self) {
        self.limiter.drain_cancelled();
        shared_limiter().drain_cancelled();
    }

    pub fn available_permits(&self) -> usize {
        self.limiter.available_permits()
    }

    pub fn limiter(&self) -> &Arc<SyntaxLimiter> {
        &self.limiter
    }

    fn prepare_request(
        &self,
        content: &str,
        deadline: Option<Instant>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Instant, SyntaxAdapterError> {
        self.config.validate()?;
        if cancelled.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(SyntaxAdapterError::Cancelled);
        }
        let effective_deadline = match deadline {
            Some(d) => d,
            None => match Instant::now().checked_add(self.config.default_timeout) {
                Some(d) => d,
                None => {
                    return Err(SyntaxAdapterError::InvalidConfiguration(
                        "timeout overflow".to_string(),
                    ));
                }
            },
        };
        if Instant::now() >= effective_deadline {
            return Err(SyntaxAdapterError::DeadlineExceeded);
        }
        if content.len() > self.config.max_source_bytes {
            return Err(SyntaxAdapterError::InputTooLarge {
                bytes: content.len(),
            });
        }
        self.limiter.acquire(Some(effective_deadline), cancelled)?;
        Ok(effective_deadline)
    }

    fn execute_in_worker<T, F>(
        &self,
        name: &str,
        effective_deadline: Instant,
        cancelled: Option<&AtomicBool>,
        f: F,
    ) -> Result<T, SyntaxAdapterError>
    where
        T: Send + 'static,
        F: FnOnce(&julie_extractors::syntax::SyntaxOptions<'_>) -> Result<T, SyntaxAdapterError>
            + Send
            + 'static,
    {
        let limiter = Arc::clone(&self.limiter);
        let max_source_bytes = self.config.max_source_bytes;
        let cancelled_arc = cancelled.map(|c| Arc::new(AtomicBool::new(c.load(Ordering::Acquire))));
        let cancelled_worker_ref = cancelled_arc.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Result<T, SyntaxAdapterError>>(1);

        let join_handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let options = julie_extractors::syntax::SyntaxOptions {
                    deadline: Some(effective_deadline),
                    cancelled: cancelled_worker_ref.as_deref(),
                    max_source_bytes,
                };
                let result = f(&options);
                let _ = tx.send(result);
            })
            .map_err(|e| {
                limiter.release();
                SyntaxAdapterError::ParseFailed { source: e.into() }
            })?;

        self.await_worker(
            rx,
            join_handle,
            effective_deadline,
            cancelled,
            cancelled_arc,
        )
    }

    pub fn parse_source(
        &self,
        file_path: &Path,
        content: &str,
        deadline: Option<Instant>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<ParsedSource, SyntaxAdapterError> {
        let deadline = self.prepare_request(content, deadline, cancelled)?;
        let path = file_path.to_path_buf();
        let source = content.to_string();
        self.execute_in_worker("syntax-parser", deadline, cancelled, move |opts| {
            julie_extractors::syntax::parse_source_with_options(&path, &source, opts)
                .map_err(SyntaxAdapterError::from)
        })
    }

    pub fn parse_and_extract(
        &self,
        file_path: &Path,
        content: &str,
        workspace_root: &Path,
        deadline: Option<Instant>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<(ParsedSource, julie_extractors::ExtractionResults), SyntaxAdapterError> {
        self.parse_and_extract_with_sync(
            file_path,
            content,
            workspace_root,
            deadline,
            cancelled,
            || {},
        )
    }

    pub fn parse_and_extract_with_sync<F>(
        &self,
        file_path: &Path,
        content: &str,
        workspace_root: &Path,
        deadline: Option<Instant>,
        cancelled: Option<&AtomicBool>,
        sync_hook: F,
    ) -> Result<(ParsedSource, julie_extractors::ExtractionResults), SyntaxAdapterError>
    where
        F: FnOnce() + Send + 'static,
    {
        let deadline = self.prepare_request(content, deadline, cancelled)?;
        let path = file_path.to_path_buf();
        let ws = workspace_root.to_path_buf();
        let source = content.to_string();
        self.execute_in_worker(
            "syntax-parser-extractor",
            deadline,
            cancelled,
            move |opts| {
                sync_hook();
                let parsed =
                    julie_extractors::syntax::parse_source_with_options(&path, &source, opts)?;
                if opts.cancelled.is_some_and(|c| c.load(Ordering::Acquire)) {
                    return Err(SyntaxAdapterError::Cancelled);
                }
                if opts.deadline.is_some_and(|d| Instant::now() >= d) {
                    return Err(SyntaxAdapterError::DeadlineExceeded);
                }
                let path_str = path.to_string_lossy().to_string();
                let extracted = julie_extractors::extract_canonical(&path_str, &source, &ws)
                    .map_err(|e| SyntaxAdapterError::ParseFailed { source: e })?;
                Ok((parsed, extracted))
            },
        )
    }

    fn await_worker<T>(
        &self,
        rx: std::sync::mpsc::Receiver<Result<T, SyntaxAdapterError>>,
        join_handle: std::thread::JoinHandle<()>,
        effective_deadline: Instant,
        cancelled: Option<&AtomicBool>,
        cancelled_arc: Option<Arc<AtomicBool>>,
    ) -> Result<T, SyntaxAdapterError> {
        loop {
            let now = Instant::now();
            let is_cancelled = cancelled.is_some_and(|c| c.load(Ordering::Acquire));
            let is_expired = now >= effective_deadline;
            if is_cancelled || is_expired {
                if let Some(ref cw) = cancelled_arc {
                    cw.store(true, Ordering::Release);
                }
                SyntaxLimiter::register_cancelled(Arc::clone(&self.limiter), join_handle);
                return Err(if is_cancelled {
                    SyntaxAdapterError::Cancelled
                } else {
                    SyntaxAdapterError::DeadlineExceeded
                });
            }

            let poll_duration =
                Duration::from_millis(20).min(effective_deadline.duration_since(now));
            match rx.recv_timeout(poll_duration) {
                Ok(result) => {
                    let _ = join_handle.join();
                    self.limiter.release();
                    if cancelled.is_some_and(|c| c.load(Ordering::Acquire)) {
                        return Err(SyntaxAdapterError::Cancelled);
                    }
                    return result;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = join_handle.join();
                    self.limiter.release();
                    return Err(SyntaxAdapterError::WorkerPanic);
                }
            }
        }
    }
}
