# hotmic

[![conduct-badge][]][conduct] [![travis-badge][]][travis] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![libraries-io-badge][]][libraries-io] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[travis-badge]: https://img.shields.io/travis/nuclearfurnace/hotmic/master.svg
[downloads-badge]: https://img.shields.io/crates/d/hotmic.svg
[release-badge]: https://img.shields.io/crates/v/hotmic.svg
[license-badge]: https://img.shields.io/crates/l/hotmic.svg
[docs-badge]: https://docs.rs/hotmic/badge.svg
[cargo-make-badge]: https://img.shields.io/badge/built%20with-cargo--make-yellow.svg
[libraries-io-badge]: https://img.shields.io/librariesio/github/sagiegurari/cargo-make.svg
[libraries-io]: https://libraries.io/cargo/cargo-make
[conduct]: https://github.com/nuclearfurnace/hotmic/blob/master/CODE_OF_CONDUCT.md
[travis]: https://travis-ci.org/nuclearfurnace/hotmic
[crate]: https://crates.io/crates/hotmic
[docs]: https://docs.rs/hotmic

__hotmic__ is a high-speed metrics collection library, based on [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam-channel).  It is shameless fork of [tic](https://github.com/brayniac/tic) with some internal changes to support `crossbeam-channel` and to fit my needs better.  This project would not be possible without `tic`!

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

    # RUST_LOG=debug target/release/examples/benchmark --duration 30 --producers 1 --capacity 128
    [2018-12-23T01:55:55Z INFO  benchmark] latency (ns): p50: 387 p90: 433 p99: 494 p999: 697 max: 838655
    [2018-12-23T01:55:56Z INFO  benchmark] total metrics pushed: 147845436

The latency values are measured from the perspective of the thread sending into the metric sink.  This section will contain better data -- including visual aids! -- in the near future.

(hotmic fcc9f5c26e77a6493b83b224ba189f1824ae1a53, December 2018)

## wall of recognition

Again, this project is a fork of `tic`, and I want to personally thank @brayniac for creating `tic` and for being a gracious open source contributor and steward.  He has many crates you should check out -- many centered around high-performance metrics -- and I can personally attest to his graciousness in issues/PRs.

## license

Per the flexible `tic` licensing terms, __hotmic__ is released solely under the MIT license. ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT)

Attribution information for `tic` can be found in the same license file.
