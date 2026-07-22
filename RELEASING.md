# Releasing

`fabric-gateway` is published to [crates.io](https://crates.io/crates/fabric-gateway)
**manually**. There is no CI release workflow — a maintainer runs `cargo publish`
from their machine so publishing is a deliberate, reviewed step.

## Publish a release

```bash
# 1. Bump the version in Cargo.toml and update CHANGELOG.md.
# 2. Sanity-check the build and the package contents:
cargo publish --dry-run

# 3. Publish (needs a crates.io token; `cargo login` once to store it):
cargo login          # paste a token from https://crates.io/settings/tokens
cargo publish

# 4. Tag the release and push:
git commit -am "chore(release): v0.1.1"
git tag v0.1.1
git push origin main v0.1.1
```

The `v*` tag is just a marker in git history — nothing is triggered by it.

## Versioning

This project follows [SemVer](https://semver.org/). Pre-1.0, breaking API
changes bump the minor version; additive/compatible changes bump the patch
version. Keep [CHANGELOG.md](CHANGELOG.md) in step with each release.
