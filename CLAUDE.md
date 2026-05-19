# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project: HFS (Heavy / Honest File Storage)

A high-performance Git LFS alternative built around content-defined chunking. The crate name, binary, on-disk directory, pointer header, and git-filter name all use lowercase `hfs`; prose and brand uses `HFS`. There is no rename pending — historical context: the project was briefly called `ppp`, then `infuse`, before settling on HFS.

## Common commands

```bash
cargo build --release            # build (release needed for benches)
cargo run -- status              # invoke a subcommand from source
cargo run -- track "*.bin"
cargo test                       # run unit + integration tests
cargo test end_to_end_small_file # run one test by substring (matches across tests/integration.rs)
cargo test -- --nocapture        # show stdout/stderr from tests

cargo bench --bench micro        # Criterion micro-benchmarks → target/criterion/*.html
cargo bench --bench dedup        # storage-savings report vs LFS-style whole-file blobs
cargo bench --bench e2e_harness  # end-to-end wall-clock vs git-lfs (needs git-lfs on PATH)
```

`autobenches = false` in `Cargo.toml` — only the three benches declared with `[[bench]]` exist; don't expect auto-discovery.

All integration tests live in one file (`tests/integration.rs`); the `cargo test <name>` substring filter matches across its `#[test]` functions.

The `filter-process` subcommand is invoked by Git itself (long-running filter protocol over stdin/stdout pkt-lines) — never run it from a terminal. To exercise it, go through `git add` / `git checkout` on a tracked file after `hfs init`.

## Architecture

The pipeline is **file → FastCDC chunks → BLAKE3 hash → zstd compress → CAS object**, with a small **manifest** (MessagePack list of chunk refs) per file and a tiny **pointer file** (text, what Git actually stores) keyed by the manifest hash. Smudge reverses it. See `docs/architecture.md` for the on-disk layout and data shapes.

Cross-cutting facts worth knowing before editing:

- **`src/cas/store.rs` is the durability boundary.** All writes go through `tmp/` and atomic rename, then end up in two-hex-prefix sharded dirs under `.hfs/objects/` and `.hfs/manifests/`. Same hash → same content → put is a no-op. Don't add write paths that bypass this.
- **The filter is one long-running process per `git add`/`git checkout`, not one per file** (`src/filter/process.rs`). It speaks Git's `process` protocol v2 via pkt-line (`src/filter/pktline.rs`): handshake → capability negotiation → command loop (`command=clean` or `command=smudge`, then metadata, then content packets terminated by flush). This is why startup cost can't be amortized across the loop body — keep per-command work cheap and avoid global state surprises.
- **Pointers vs manifests are two different objects.** Pointer = ~256-byte text in git history beginning with `hfs v1`, contains the *manifest* BLAKE3 hash. Manifest = MessagePack blob in `.hfs/manifests/`, contains an ordered list of *chunk* hashes + offsets + sizes. Chunks live in `.hfs/objects/`. Touching any of these three formats is a wire/storage compat change.
- **Sync vs async split.** CAS, manifest, pointer, filter are all sync. Only push/pull/clone (`src/cli/{push,pull,clone}.rs` + `src/transfer/engine.rs` + `src/backend/`) are async — `main.rs` spins up a Tokio runtime just for those three commands. The transfer engine uses a 32-permit semaphore for concurrent chunk transfers; backends implement `async_trait Backend` (`push_chunk` / `pull_chunk` / `has_chunk` / `list_chunks`) in `src/backend/mod.rs`.
- **`pull` vs `clone` both fetch chunks** but differ in scope: `pull` resolves manifests referenced by current pointer files and fetches missing chunks; `clone` is run after `git clone` and walks every pointer file in the working tree to fetch both manifests and chunks.
- **Config + repo discovery walks up from cwd** (`Config::find_hfs_dir` in `src/config.rs`) looking for `.hfs/`. Default chunk range is 256 KiB / 1 MiB avg / 4 MiB max, zstd level 3 — changing these defaults changes dedup behavior for new commits but does not migrate existing chunks.
- **CLI surface = one module per command under `src/cli/`**, wired from `src/main.rs`. New commands go there; keep the runtime-bootstrap distinction (sync direct call vs `rt.block_on(...)`) consistent with whether the command uses the backend.

## On-disk + git contract

These strings are part of the format and changing them breaks existing repos:

- `.hfs/` directory name (set in `src/cli/init.rs`, walked by `Config::find_hfs_dir`).
- Pointer header `"hfs v1"` (in `src/pointer.rs:13`).
- Git filter name `filter.hfs.process` / `filter.hfs.required` (set in `src/cli/init.rs`).
- `.gitattributes` filter token: `filter=hfs diff=hfs merge=hfs` (written by `hfs track`, checked by `hfs untrack`, `status`, and `ls-files`).

## Commit style

Format: `<type>(optional scope): <description>`

- Description is a single short line — no multiline messages, no `Co-Authored-By` trailers
- Types: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`
- Examples: `feat(cas): add chunk dedup on write`, `fix(filter): handle flush on empty file`

## What's intentionally NOT in this repo

- No CI config, no formatter config, no lint config beyond Rust defaults — `cargo fmt` and `cargo clippy` are fine to run but there's no enforced style.
- No git hooks or pre-commit. The "filter" referred to throughout is Git's clean/smudge filter, not a commit hook.
- No Cursor or Copilot rules.
