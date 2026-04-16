# Changelog

All notable changes to the Python Event Sourcing SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.14.0] - 2026-04-16

### Added

- `HistoricalPoller.process()` now accepts an `is_replay: bool = False` keyword
  argument. The base class sets `is_replay=True` when invoking `process()` on
  the cold-start path (events that survived the `_started_at` timestamp fence
  during the first poll for a source). Subclasses may use this flag to mark
  events as unprimed so downstream consumers skip side-effectful work such as
  trigger evaluation. Default value preserves Liskov compatibility for
  existing subclasses.

### Changed

- Cold-start branch in `HistoricalPoller.poll()` now passes `is_replay=True` to
  `process()`. Warm-start (steady-state) polling continues to pass the default
  `is_replay=False`.

### Context

Consumers of the GitHub Events API pattern (and similar re-delivering APIs)
need a signal that the current batch is cold-start replay, separate from the
timestamp fence that only kicks in on the first poll per source. Relying on
mutable state such as a `primed_sources` set proved race-prone: the framework
primes the source before calling `process()`, so any "am I primed yet?"
check observed from inside the subclass was always True. The `is_replay`
kwarg delivers the signal directly from `poll()` to `process()` without
going through mutated framework state.
