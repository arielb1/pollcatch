use std::{fs::File, future::Future, mem::MaybeUninit, pin::Pin, sync::{atomic, LazyLock, OnceLock}};

mod calibration;
mod stats;
mod tsc;
mod writer;

pin_project_lite::pin_project! {
    /// A future that times the time since the last poll
    pub struct PollTimingFuture<F: Future> {
        #[pin]
        inner: F
    }
}

static PERFORMANCE_WRITER: OnceLock<std::sync::mpsc::Sender<writer::Event>> = OnceLock::new();

pub fn start_performance_writer(f: File) {
    PERFORMANCE_WRITER.get_or_init(|| {
        writer::start_writer(f)
    });
}

static TIMESTAMP_PTHREAD_KEY: LazyLock<libc::c_int> = LazyLock::new(|| unsafe {
    let mut key = !0;
    if libc::pthread_key_create(&mut key, None) < 0 {
        -1
    } else {
        key as libc::c_int
    }
});

static TIMESTAMP_PTHREAD_KEY_ASYNC_SIGNAL_SAFE: std::sync::atomic::AtomicIsize =
    std::sync::atomic::AtomicIsize::new(-1);
static SIGACTION: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn empty_sigset() -> libc::sigset_t {
    let mut result: MaybeUninit<libc::sigset_t> = MaybeUninit::zeroed();
    unsafe {
        if libc::sigemptyset(result.as_mut_ptr()) != 0 {
            panic!();
        }
        result.assume_init()
    }
}

#[allow(non_camel_case_types)]
type sigaction_t = extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void);

extern "C" fn my_action(sig: libc::c_int, info: *mut libc::siginfo_t, ucontext: *mut libc::c_void) {
    unsafe {
        write_timestamp_pthread_key(1);
        let sig_fn = SIGACTION.load(atomic::Ordering::Acquire);
        if sig_fn != 0 && sig_fn != libc::SIG_DFL && sig_fn != libc::SIG_IGN {
            std::mem::transmute::<usize, sigaction_t>(sig_fn)(sig, info, ucontext);
        }
    }
}

/// Enables poll timing.
///
/// Until this function is called, poll timing will not be measured.
///
/// This function is fine if called multiple times.
pub fn enable_poll_timing(log_file: File) {
    start_performance_writer(log_file);

    let mut calibration = calibration::Calibration::default();
    calibration.calibrate(&nanotime, &tsc::now);

    if let Some(ch) = PERFORMANCE_WRITER.get() {
        ch.send(writer::Event::CalibrateTscToMonotonic {
            data: writer::CalibrationData {
                shift: calibration.scale_shift,
                mul: calibration.scale_factor,
                src_epoch: calibration.src_time,
                ref_epoch: calibration.ref_time
            }
        }).ok();
    }

    // reading a #[thread_local] is not async signal safe, which is why we use a
    // LazyLock (to synchronize writers of the pthread key), an AtomicI64
    // to synchronize readers of the pthread key, and a pthread key to synchronize threads.
    // force the pthread key
    let pthread_key = *TIMESTAMP_PTHREAD_KEY as isize;
    // and write it to the variable. Use an *atomic* write here to ensure that no thread
    // will try to access the pthread-key before it is defined.
    //
    // If there are multiple stores, they will all write the same value and happen-after
    // the pthread key initialization due to the LazyLock.
    //
    // This assumes that it's OK to use lock-free atomics from signals as per C11
    TIMESTAMP_PTHREAD_KEY_ASYNC_SIGNAL_SAFE
        .store(pthread_key, std::sync::atomic::Ordering::Release);
    unsafe {
        let act: libc::sigaction = libc::sigaction {
            sa_sigaction: my_action as usize,
            sa_mask: empty_sigset(),
            sa_flags: libc::SA_SIGINFO | libc::SA_RESTART,
            sa_restorer: None,
        };
        let mut oldact: libc::sigaction = libc::sigaction {
            sa_sigaction: 0,
            sa_mask: empty_sigset(),
            sa_flags: 0,
            sa_restorer: None,
        };
        if libc::sigaction(libc::SIGPROF, &act, &mut oldact) != 0 {
            panic!("sigaction {:?}", std::io::Error::last_os_error());
        }
        SIGACTION.store(oldact.sa_sigaction, atomic::Ordering::Release);
    }
}

/// async-signal safe. returns 0 if key is not initialized
pub fn read_timestamp_pthread_key() -> usize {
    unsafe {
        let key =
            TIMESTAMP_PTHREAD_KEY_ASYNC_SIGNAL_SAFE.load(std::sync::atomic::Ordering::Acquire);
        if key >= 0 {
            libc::pthread_getspecific(key as libc::pthread_key_t) as usize
        } else {
            0
        }
    }
}

/// Write the timestamp pthread key. no-op if key is not initialized.
pub fn write_timestamp_pthread_key(time: usize) {
    unsafe {
        let key =
            TIMESTAMP_PTHREAD_KEY_ASYNC_SIGNAL_SAFE.load(std::sync::atomic::Ordering::Acquire);
        if key >= 0 {
            libc::pthread_setspecific(key as libc::pthread_key_t, time as *const libc::c_void);
        }
    }
}

impl<F: Future> PollTimingFuture<F> {
    /// Wrap a future into a PollTimingFuture
    pub fn new(inner: F) -> Self {
        PollTimingFuture { inner }
    }
}

fn nanotime() -> u64 {
    unsafe {
        let mut ts = MaybeUninit::uninit();
        if libc::clock_gettime(libc::CLOCK_MONOTONIC, ts.as_mut_ptr()) != 0 {
            0
        } else {
            let ts = ts.assume_init();
            (ts.tv_sec as u64).wrapping_mul(1_000_000_000).wrapping_add(ts.tv_nsec as u64)
        }
    }
}

#[cold]
#[inline(never)]
fn write_timestamp(before: u64) {
    if let Some(ch) = PERFORMANCE_WRITER.get() {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) as u32 };

        let clock_end = nanotime();
        let end = tsc::now();
        ch.send(writer::Event::Poll { start: before, end, clock_end, tid }).ok();
    }
}

fn timestamping<R, F: FnOnce() -> R>(f: F) -> R {
    let before = tsc::now();
    write_timestamp_pthread_key(0);
    let res = f();
    if read_timestamp_pthread_key() == 1 {
        write_timestamp(before);
    }
    res
}

impl<F: Future> Future for PollTimingFuture<F> {
    type Output = F::Output;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        timestamping(|| this.inner.poll(cx))
    }
}


/// A tower layer that adds long poll detection
pub struct PollTimingLayer;

impl<S> tower_layer::Layer<S> for PollTimingLayer {
    type Service = PollTimingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        PollTimingService { inner }
    }
}

/// A tower service that adds long poll detection
pub struct PollTimingService<S> { inner: S }

impl<S, Request> tower_service::Service<Request> for PollTimingService<S>
    where S: tower_service::Service<Request>
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = PollTimingFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        // not timestamping here - leave that to the top-level future
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        PollTimingFuture::new(self.inner.call(req))
    }
}
