# hotmic

[![conduct-badge][]][conduct] [![travis-badge][]][travis] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![libraries-io-badge][]][libraries-io] [![cargo-make-badge][]][cargo-make] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[travis-badge]: https://img.shields.io/travis/nuclearfurnace/hotmic/master.svg
[downloads-badge]: https://img.shields.io/crates/d/hotmic.svg
[release-badge]: https://img.shields.io/crates/v/hotmic.svg
[license-badge]: https://img.shields.io/crates/l/hotmic.svg
[docs-badge]: https://docs.rs/hotmic/badge.svg
[cargo-make-badge]: https://img.shields.io/badge/built%20with-cargo--make-yellow.svg
[cargo-make]: https://sagiegurari.github.io/cargo-make/
[libraries-io-badge]: https://img.shields.io/librariesio/github/nuclearfurnace/hotmic.svg
[libraries-io]: https://libraries.io/cargo/hotmic
[conduct]: https://github.com/nuclearfurnace/hotmic/blob/master/CODE_OF_CONDUCT.md
[travis]: https://travis-ci.org/nuclearfurnace/hotmic
[crate]: https://crates.io/crates/hotmic
[docs]: https://docs.rs/hotmic

__hotmic__ is a high-speed metrics collection library, based on [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam-channel).  It is heavily inspired by [tic](https://github.com/brayniac/tic).

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

## usage

The API documentation of this library can be found at [docs.rs/hotmic](https://docs.rs/hotmic/).

## general features
- based on `crossbeam-channel`, so it's blazingly fast
- supports counters, gauges, and histograms
- provides dynamic faceting: what portion of metric data should be recorded, and in what way
- control mechanism to allow any caller to retrieve metric snapshots at any time
- scoped metrics (one metric with different prefixes)

## performance

This section used to have way higher numbers, and a full comparison vs `tic`, but based on recent refactoring, the numbers are off.  Here's a quick look at the current performance of `hotmic`:

    # RUST_LOG=debug target/release/examples/benchmark --duration 30 --producers 1 --capacity 4096
    [2019-01-20T00:49:06Z INFO  benchmark] rate: 4991107.330041891 samples per second
    [2019-01-20T00:49:06Z INFO  benchmark] latency (ns): p50: 389 p90: 422 p99: 520 p999: 783 max: 2077695
    [2019-01-20T00:49:07Z INFO  benchmark] total metrics pushed: 296547696

The latency values are measured from the perspective of the thread sending into the metric sink.  This section will contain better data -- including visual aids! -- in the near future.
