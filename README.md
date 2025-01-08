# pollcatch

Helps find long Tokio polls

To use, you currently need this branch of async-profiler:

https://github.com/arielb1/async-profiler/tree/magic-bci

At a later time, this should be included in mainline async-profiler

Example usage:
```
git clone ssh://git@github.com/arielb1/async-profiler -b magic-bci
( cd async-profiler && docker run -v $PWD:/async-profiler --workdir /async-profiler $(docker build -q .) make -j64 )
export LD_LIBRARY_PATH=$PWD/async-profiler/build/lib
( cd decoder && cargo build --release )
cargo run --example simple
./decoder/target/release/pollcatch-decoder longpolls profile.jfr 5ms
```
