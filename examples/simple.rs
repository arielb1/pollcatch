use std::ffi::CStr;
use tokio;

#[allow(non_camel_case_types)]
pub type asprof_error_t = *const std::ffi::c_char;
#[allow(non_camel_case_types)]
pub type asprof_writer_t = unsafe extern "C" fn(buf: *const std::ffi::c_char, size: usize);

unsafe extern "C" fn my_output_callback(buf: *const std::ffi::c_char, size: usize) {
    let buf = String::from_utf8_lossy(std::slice::from_raw_parts(buf.cast(), size));
    println!("[CALLBACK] {}", buf);
}

#[inline(never)]
#[allow(deprecated)]
pub fn accidentally_slow() {
    std::thread::sleep_ms(10);
    std::hint::black_box(0);
}

#[inline(never)]
pub fn as_() {
    accidentally_slow();
    std::hint::black_box(0);
}

#[inline(never)]
pub fn not_accidentally_slow() {
    std::thread::sleep(std::time::Duration::from_micros(100));
    std::hint::black_box(0);
}

#[inline(never)]
pub fn nas() {
    not_accidentally_slow();
    std::hint::black_box(0);
}

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let cmd = c"start,jfr,timeout=3s,event=wall,cstack=dwarf,loglevel=debug,file=profile.jfr";
    let err;
    pollcatch::enable_poll_timing();
    let lib;
    let asprof_init: libloading::Symbol<unsafe extern "C" fn()>;
    let asprof_error_str: libloading::Symbol<unsafe extern "C" fn(err: asprof_error_t) -> *const std::ffi::c_char>;
    let asprof_execute: libloading::Symbol<unsafe extern "C" fn(
        command: *const std::ffi::c_char,
        output_callback: asprof_writer_t,
    ) -> asprof_error_t>;
    let asprof_set_helper: libloading::Symbol<unsafe extern "C" fn(helper: extern "C" fn() -> u64)>;
    unsafe {
        lib = libloading::Library::new("libasyncProfiler.so")?;
        asprof_init = lib.get(b"asprof_init")?;
        asprof_error_str = lib.get(b"asprof_error_str")?;
        asprof_execute = lib.get(b"asprof_execute")?;
        asprof_set_helper = lib.get(b"asprof_set_helper")?;

        asprof_init();
        asprof_set_helper(pollcatch::asprof_helper_fn);
        err = asprof_execute(cmd.as_ptr().cast(), my_output_callback);    
    }

    if !err.is_null() {
        unsafe {
            let err = asprof_error_str(err);
            println!(
                "{}",
                String::from_utf8_lossy(CStr::from_ptr(err).to_bytes())
            );
        }
    }

    let mut ts = vec![];

    for _ in 0..16 {
        ts.push(tokio::task::spawn(pollcatch::PollTimingFuture::new(async move {
            for i in 0..20_000u64 { // 100 us * 20_000 = 2s
                tokio::task::yield_now().await;
                if i % 1000 == 0 {
                    as_();
                } else {
                    nas();
                }
            }
        })));
    }
    for t in ts {
        t.await.ok();
    }

    Ok(())
}
