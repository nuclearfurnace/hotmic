# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1] - 2019-01-19
### Changed
- Controller is now clonable.

## [0.5.0] - 2019-01-19
### Added
- Revamp API to provide easier usage. (by [@tobz](https://github.com/tobz), [#14](https://github.com/nuclearfurnace/hotmic/pull/14))

## [0.4.0] - 2019-01-14
Minimum supported Rust version is now 1.31.0, courtesy of switching to the 2018 edition.

### Changed
- Switch to integer-backed metric scopes. (by [@tobz](https://github.com/tobz), [#10](https://github.com/nuclearfurnace/hotmic/pull/10))
### Added
- Add clock support via `quanta`. (by [@tobz](https://github.com/tobz), [#12](https://github.com/nuclearfurnace/hotmic/pull/12))

## [0.3.0] - 2018-12-22
### Added
- Switch to crossbeam-channel and add scopes. (by [@tobz](https://github.com/tobz), [#4](https://github.com/nuclearfurnace/hotmic/pull/4))
