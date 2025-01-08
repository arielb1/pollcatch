# pollcatch

Helps find long Tokio polls

To use, you currently need this branch of async-profiler:

https://github.com/arielb1/async-profiler/tree/magic-bci

This adds a little bit of overhead of adding a thread-local timer to the wrapped future.

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

## Example output

```
[2148483.851781] poll of 22992us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148483.851813] poll of 24381us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148484.201697] poll of 9426us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148484.201732] poll of 10244us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148485.451685] poll of 24028us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148485.451722] poll of 21967us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148485.801682] poll of 8271us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2148485.801715] poll of 5966us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)
```