# pollcatch

Helps find long Tokio poll times. Tokio tasks that have long poll times can delay the Tokio runtime and cause
high tail latencies to other tasks running on the same executor, so it's useful to have a tool catching them.

To use this, you need to attach async-profiler to your program. For it to work, you can't load it with LD_PRELOAD
but must use the API calls and add a call to the `asprof_set_helper` (added in my branch) that enables the timestamp
tracing.

To use, you currently need this branch of async-profiler:

https://github.com/arielb1/async-profiler/tree/magic-bci

This adds a little bit of overhead of adding a thread-local timer to the wrapped future.

At a later time, this should be included in mainline async-profiler

Example usage:
```
# get async-profiler
git clone ssh://git@github.com/arielb1/async-profiler -b magic-bci

# compile my branch of async-profiler
( cd async-profiler && docker run -v $PWD:/async-profiler --workdir /async-profiler $(docker build -q .) make -j64 )

# point LD_LIBRARY_PATH to it
export LD_LIBRARY_PATH=$PWD/async-profiler/build/lib

# build the decoder
( cd decoder && cargo build --release )

# run the example, which has an `accidentally_slow` function
cargo run --example simple

# this generates a `profile.jfr` containing the profile data,
# you can then use the decoder to decode the long polls
./decoder/target/release/pollcatch-decoder longpolls profile.jfr 5ms
```

## Example output

As you can see, there is an `accidentally_slow` function that calls `sleep_ms` :-(.

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

