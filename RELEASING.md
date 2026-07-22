# Releasing

`fabric-gateway` is published to [crates.io](https://crates.io/crates/fabric-gateway).
There are two ways to publish; the first is a one-time setup that makes every
future release push-button.

## Option A — Trusted Publishing (recommended, no tokens)

crates.io supports [Trusted Publishing](https://crates.io/docs/trusted-publishing):
GitHub Actions authenticates via OIDC, so no API token is ever stored.

**One-time setup** (crates.io owner, `dviejokfs`):

1. Go to <https://crates.io/crates/fabric-gateway/settings> → *Trusted Publishing*.
2. Add a GitHub publisher:
   - Repository owner: `chainlaunch`
   - Repository name: `fabric-gateway-rs`
   - Workflow filename: `release.yml`
   - Environment: `release`

**Each release** thereafter:

```bash
# 1. Bump the version in Cargo.toml and update CHANGELOG.md.
# 2. Commit, then tag and push the tag:
git commit -am "chore(release): v0.1.1"
git tag v0.1.1
git push origin main v0.1.1
```

Pushing the `v*` tag triggers `.github/workflows/release.yml`, which verifies the
tag matches `Cargo.toml`, obtains a short-lived token via OIDC, and runs
`cargo publish`.

## Option B — manual publish with an API token

```bash
cargo login          # paste a token from https://crates.io/settings/tokens
cargo publish        # run `cargo publish --dry-run` first to sanity-check
```

## Versioning

This project follows [SemVer](https://semver.org/). Pre-1.0, breaking API
changes bump the minor version; additive/compatible changes bump the patch
version. Keep [CHANGELOG.md](CHANGELOG.md) in step with each release.
