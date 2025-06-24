# pollcatch

Helps find long Tokio poll times. Tokio tasks that have long poll times can delay the Tokio runtime and cause
high tail latencies to other tasks running on the same executor, so it's useful to have a tool catching them.

To use this, you need to attach async-profiler to your program. For it to work, you can't load it with LD_PRELOAD
but must use the async-profiler API calls (`asprof_init` and `asprof_execute`). The information is currently published only in JFR mode
(to a JFR file you can read offline), and you can use the decoder provided in this package to find the polls
exceeding some threshold.

The current library does SIGPROF chaining to make things work, which is evil but works. I'm trying to get async-profiler to give me some pointer to avoid needing that.

This adds a little bit of overhead of adding a thread-local timer to the wrapped future.

At a later time, this should be included in mainline async-profiler

Example usage:
```
# get async-profiler
git clone ssh://git@github.com/arielb1/async-profiler

# compile it
( cd async-profiler && docker run -v $PWD:/async-profiler --workdir /async-profiler $(docker build -q .) make -j64 )

# point LD_LIBRARY_PATH to it
export LD_LIBRARY_PATH=$PWD/async-profiler/build/lib

# build the decoder
( cd decoder && cargo build --release )

# run the example, which has an `accidentally_slow` function
cargo run --example simple

# this generates a `profile.jfr` containing the profile data,
# you can then use the decoder to decode the long polls
./decoder/target/release/pollcatch-decoder longpolls profile.jfr 5ms --pr-file performance.pr
```

## Example output

As you can see, there is an `accidentally_slow` function that calls `sleep_ms` :-(.

```
[2739944.717933] thread 65314 - poll of 9982us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.967912] thread 65314 - poll of 8123us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739944.717939] thread 65315 - poll of 9667us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.317903] thread 65318 - poll of 5290us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072099] thread 65300 - poll of 6935us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.967915] thread 65319 - poll of 8047us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.321930] thread 65300 - poll of 8758us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739944.717939] thread 65316 - poll of 9515us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072111] thread 65303 - poll of 6884us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.321935] thread 65303 - poll of 7549us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739944.717941] thread 65317 - poll of 9898us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.320692] thread 65322 - poll of 7056us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072109] thread 65304 - poll of 7211us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.321940] thread 65304 - poll of 8825us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739944.717957] thread 65320 - poll of 9880us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072113] thread 65307 - poll of 6463us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.967918] thread 65320 - poll of 6194us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.967934] thread 65321 - poll of 9052us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072120] thread 65309 - poll of 6730us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072118] thread 65308 - poll of 7214us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.321953] thread 65308 - poll of 8381us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739945.072123] thread 65310 - poll of 7036us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)

[2739946.321957] thread 65310 - poll of 7906us
 -   1: libpthread-2.26.so.__nanosleep
 -   2: simple.std::thread::sleep_ms
 -   3: simple.simple::accidentally_slow
 -   4: simple.simple::as_
 -   5: simple.simple::main::{{closure}}::{{closure}}
 -  56 more frame(s) (pass --stack-depth=61 to show)
```

