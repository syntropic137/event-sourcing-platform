# VSA: Idiomatic-Rust dogfood enablement

Branch: `feat/rust-idiomatic-detection` (local only, not pushed).

Two minimal, backward-compatible patches to `vsa-core` so `vsa validate` and
`vsa manifest` understand idiomatic Rust naming conventions, enabling us to
enforce architectural boundaries on dream-ship.

## PATCH 1: language-aware public-API file (rule VSA205)

File: `vsa-core/src/validation/cross_context_rules.rs`

`ContextPublicApiExistsRule::validate` previously hardcoded `__init__.py` plus a
Python `from ... import` / `__all__` export grep. It now switches on
`config.language` via a small `PublicApiConvention` helper:

| language           | public-API file            | export check (trimmed line)      |
|--------------------|----------------------------|----------------------------------|
| rust               | `mod.rs` (fallback `lib.rs`)| line starts with `pub `          |
| typescript         | `index.ts`                 | line starts with `export `       |
| python (default)   | `__init__.py`              | `from ... import` or `__all__`   |

Python and TypeScript behavior is unchanged (Python was the historical default,
and any unrecognized language still falls through to it).

## PATCH 2: snake_case artifact detection

Files: `vsa-core/src/config.rs`, `vsa-core/src/validation/structure_rules.rs`
(plus a one-line test-fixture update in `vsa-core/src/patterns.rs`).

Added `patterns.filename_convention: snake_case | pascal_case` (default
`pascal_case`, so existing TS/Python behavior is byte-for-byte unchanged). The
command / event / aggregate / port predicates now route through two helpers:

- `matches_artifact_suffix(ctx, file_name, "Command"|"Event"|"Aggregate"|"Query")`
  - pascal_case: stem ends with the suffix (e.g. `CreateFooCommand.ts`)
  - snake_case: stem ends with `_command` / `_event` / `_aggregate` / `_query`
    (e.g. `create_foo_command.rs`)
- `matches_port_file(ctx, file_name)`
  - pascal_case: stem ends with `Port`
  - snake_case: stem ends with `_port` OR keeps the legacy `Port` suffix

Both aggregate predicates (VSA022 root rule and VSA027 folder rule) were updated
for consistency.

## VERIFY

### `cargo build -p vsa-cli --release`

Clean (both before and after). Only pre-existing warning:
`method 'check_aggregates_in_domain_root' is never used` (unrelated to this work).

### `cargo test -p vsa-core --lib`

`284 passed; 0 failed`. This includes the newly added tests:

- VSA205 Rust: `test_vsa205_rust_context_with_mod_rs_passes`,
  `..._lib_rs_fallback_passes`, `..._missing_mod_rs_errors`,
  `..._mod_rs_without_pub_errors`, plus a TypeScript `index.ts` case.
- snake_case detection: `test_snake_case_command_event_aggregate_detected`
  (`foo_command.rs` -> command, `bar_event.rs` -> event,
  `baz_aggregate.rs` -> aggregate, `knowledge_port.rs` -> port),
  `test_snake_case_port_keeps_pascal_suffix`,
  `test_snake_case_rejects_pascal_and_wrong_ext`,
  `test_pascal_case_default_unchanged`.

Note: the integration test `test_fixture_directory_structure_exists` fails in
this checkout ("Python fixtures directory should exist"). This failure is
PRE-EXISTING and unrelated: it also fails on `main` (verified by stashing these
changes). It is a missing test-fixtures directory, not a code regression.

### End-to-end against dream-ship

Config `/tmp/rust-vsa.yaml`:

```yaml
version: 1
architecture: vertical-slice
root: /home/ubuntu/deploy/dream-ship-main/ws_packages/application/src
language: rust
patterns:
  filename_convention: snake_case
```

`vsa validate` (the headline result):

| | errors | detail |
|--|--|--|
| BEFORE (main) | **3** | `Context 'slices'/'bridge'/'adapters' is missing __init__.py public API` (false positives against Rust `mod.rs` layout) |
| AFTER (patched) | **0** | validation passes; VSA205 now finds each context's `mod.rs`. The 3 false-positive errors are GONE. |

(The 3 `_shared`-folder warnings are unchanged and orthogonal to this work.)

`vsa manifest --include-domain --format json`:

- BEFORE and AFTER both report the same bounded contexts, now seen as Rust
  modules rather than choking on `__init__.py`:
  `slices`, `bridge`, `adapters` (each `context_type: invalid_module`,
  `aggregate_count: 0`).
- Domain artifact counts (aggregates / commands / events / queries): **0 / 0 / 0
  / 0** for this root.

  Why 0, honestly: (a) the minimal config has no `domain:` block, and
  `Manifest::generate_with_options` only emits the domain manifest when
  `config.domain.is_some()`; and (b) `ws_packages/application/src` is the
  application layer - its files use slice-style names (`set_goal.rs`,
  `create_compartment.rs`, `ingest_signal.rs`, `write_back.rs`) and ports named
  `ports.rs` / `knowledge_ports.rs`, none of which carry the `_command` /
  `_event` / `_aggregate` / `_port` (singular) stem suffixes. The counts are
  therefore 0 both before and after; this tree simply contains no matching
  domain artifacts.

  The snake_case detection added in PATCH 2 lives in the `structure_rules`
  predicates and is proven by the unit tests above (`foo_command.rs` etc.). The
  `manifest --include-domain` domain scanners (`aggregate_scanner` /
  `command_scanner` / `event_scanner`) are a SEPARATE PascalCase code path that
  these two minimal patches intentionally did not touch; wiring the convention
  through those scanners is a natural, in-scope follow-up if we want
  `manifest --include-domain` counts for snake_case domain layers.

## Net effect

The concrete dogfood blocker - VSA205 emitting `__init__.py` false positives on
every Rust bounded context - is fixed: `vsa validate` against dream-ship's
`application/src` now passes instead of failing with 3 errors. Rust codebases can
opt into snake_case artifact detection with `patterns.filename_convention:
snake_case`, and all existing TypeScript/Python behavior is unchanged.

## Codex review follow-up (REQUEST_CHANGES -> 3 findings fixed)

All three findings were addressed on the same branch. No push.

### Finding 1: VSA025 (RequirePortSuffixRule) hardcoded `*Port`

File: `vsa-core/src/validation/structure_rules.rs`

`RequirePortSuffixRule::validate` now routes the accept/reject decision through
the same `matches_port_file(ctx, file_name)` helper the other port predicates
use, so a valid snake_case `knowledge_port.rs` is no longer flagged under
`filename_convention: snake_case`. The error message and the `git mv`
suggestion now reflect the active convention (`_port` for snake_case, `Port`
for pascal_case). New test: `test_vsa025_snake_case_port_accepted`.

### Finding 2: manifest/domain scanners matched PascalCase only

Files: `vsa-core/src/scanners/{mod.rs, command_scanner.rs, event_scanner.rs,
query_scanner.rs, aggregate_scanner.rs, domain_scanner.rs}`,
`vsa-core/src/manifest.rs`.

Added a shared `scanners::stem_matches_suffix(stem, suffix, convention)` helper
(mirrors the detection logic in `structure_rules`). Each of the four artifact
scanners gained a `filename_convention` field (defaults to `pascal_case`, so
`::new(...)` callers and every existing test are byte-for-byte unchanged) plus a
`with_filename_convention(...)` builder. `DomainScanner` threads its convention
into all four sub-scanners, and `Manifest::generate_with_options` passes
`config.patterns.filename_convention` into `DomainScanner` at both construction
sites. New test: `test_manifest_include_domain_snake_case_rust_context` proves
nonzero command/event/aggregate counts for a snake_case Rust context, with a
pascal_case control asserting the same tree yields 0 (so the convention is what
unblocks them).

This supersedes the earlier "SEPARATE PascalCase code path ... intentionally did
not touch" note above - the scanners are now wired through.

### Finding 3: VSA205 TypeScript behavior was changed (backward-compat break)

File: `vsa-core/src/validation/cross_context_rules.rs`

The original pre-patch code applied the `__init__.py` check to ALL languages.
The idiomatic-Rust patch had changed TypeScript to `index.ts`, breaking
byte-for-byte backward compatibility. Reverted: only the new
`rust -> mod.rs (fallback lib.rs)` branch remains; every non-Rust language
(Python AND TypeScript) keeps the original `__init__.py` check. Removed the now
unused `ApiLanguage::TypeScript` variant. The TS test
(`test_vsa205_typescript_keeps_original_init_py_behavior`) now asserts the
UNCHANGED original behavior: an `index.ts` alone does NOT satisfy the rule (both
contexts error on missing `__init__.py`), and the historical `__init__.py` path
still passes.

### VERIFY (post-fix)

- `cargo build -p vsa-cli --release`: clean (`Finished release`). Only the
  pre-existing `check_aggregates_in_domain_root is never used` warning.
- `cargo test -p vsa-core --lib`: **286 passed; 0 failed** (284 prior + 2 new).
- Clippy: `cargo clippy -p vsa-core --lib` reports **12 warnings on BOTH the
  unpatched HEAD and this working tree** (verified by `git stash`), i.e. these
  fixes introduce ZERO new clippy warnings.

Pre-existing failures (OUT OF SCOPE, confirmed present on unpatched HEAD via
`git stash`, NOT touched):

- `cargo test -p vsa-core` (full, incl. integration) fails
  `integration::fixture_validation::test_fixture_directory_structure_exists`
  ("Python fixtures directory should exist") - a missing test-fixtures
  directory, fails identically without our changes.
- `cargo clippy -- -D warnings` fails on the 12 pre-existing warnings above -
  fails identically without our changes.

### End-to-end re-run (snake_case Rust context)

Fixture `/tmp/snake_rust_ctx_v1` (single `github/` context, snake_case Rust
domain: `workspace_aggregate.rs`, `commands/create_workspace_command.rs`,
`events/workspace_created_event.rs`, `queries/get_workspace_query.rs`,
`ports/knowledge_port.rs`, `mod.rs` with a `pub use`).

`vsa manifest --include-domain` (`filename_convention: snake_case`):

```
commands  : 1 ['create_workspace_command']
events    : 1 ['workspace_created_event']
aggregates: 1 ['workspace_aggregate']
queries   : 1 ['get_workspace_query']
```

Same tree with `filename_convention: pascal_case` (the pre-fix behavior):
`commands: 0 events: 0 aggregates: 0 queries: 0` - proving Finding 2 is fixed.

`vsa validate` (snake_case) now RECOGNIZES the snake_case aggregate and port:
it emits VSA022 (`workspace_aggregate.rs` at domain root should be in
`domain/aggregate_*/`) instead of silently omitting the file, and it emits NO
VSA205 `__init__.py` false positive (the Rust `mod.rs` with `pub` items is
found) and NO VSA025 error for the valid `knowledge_port.rs`.
