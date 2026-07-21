# Contributing to fabric-gateway (Rust)

Thanks for your interest in contributing!

## Development setup

You need a recent stable Rust toolchain and the Protocol Buffers compiler
(`protoc`), which `build.rs` uses to generate the gateway wire types.

```bash
# macOS:        brew install protobuf
# Debian/Ubuntu: apt-get install -y protobuf-compiler libprotobuf-dev

# clone and enter the repo, then:
cargo build --workspace --all-targets
```

## Before opening a PR

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass — CI enforces the same commands.

> The cross-language parity tests under `tests/parity.rs` skip automatically
> unless the Go harness fixtures are present. To run them, see
> [`parity-harness/README.md`](parity-harness/README.md) and
> [`PARITY.md`](PARITY.md).

## Making changes

1. Fork the repo and create a branch off `main`.
2. Keep PRs focused: one logical change per PR.
3. Add or update tests for behavior you change. Anything touching the wire
   format, transaction-ID derivation, or signature encoding must stay
   byte-compatible with fabric-gateway-go — add or extend a parity/unit test.
4. Write a clear PR description: what changed and why, not just what.
5. Link any related issue.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/) where
practical (`feat:`, `fix:`, `docs:`, `chore:`, ...).

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be kind.

## Questions

Open a [Discussion](../../discussions) or an issue tagged `question`.
