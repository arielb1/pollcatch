# pollcatch

Helps find long Tokio polls

To use, you currently need this branch of async-profiler:

https://github.com/arielb1/async-profiler/tree/magic-bci

At a later time, this should be included in mainline async-profiler

Example usage:
```
LD_LIBRARY_PATH=<path to libasyncProfiler.so>
( cd decoder && cargo build --release )
cargo run --example simple
./decoder/target/release/pollcatch-decoder longpolls profile.jfr 5ms
```
