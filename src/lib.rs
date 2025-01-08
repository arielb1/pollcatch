use std::{future::Future, pin::Pin, sync::LazyLock};

mod tsc;

pin_project_lite::pin_project! {
    /// A future that times the time since the last poll
    pub struct PollTimingFuture<F: Future> {
        #[pin]
        inner: F
    }
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

/// Enables poll timing.
///
/// Until this function is called, poll timing will not be measured.
///
/// This function is fine if called multiple times.
pub fn enable_poll_timing() {
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

/// asprof function you'll need to install using `asprof_set_helper(pollcatch::asprof_helper_fn)`
pub extern "C" fn asprof_helper_fn() -> u64 {
    let cur = read_timestamp_pthread_key();
    if cur == 0 {
        return 0;
    }
    let now = tsc::now() as usize;
    now.wrapping_sub(cur) as u64
}

impl<F: Future> PollTimingFuture<F> {
    /// Wrap a future into a PollTimingFuture
    pub fn new(inner: F) -> Self {
        PollTimingFuture { inner }
    }
}

fn timestamping<R, F: FnOnce() -> R>(f: F) -> R {
    write_timestamp_pthread_key(tsc::now() as usize);
    let res = f();
    write_timestamp_pthread_key(0);
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
