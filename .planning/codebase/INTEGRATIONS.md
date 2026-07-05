# External Integrations

**Analysis Date:** 2026-07-05

## APIs & External Services

**Code Quality & Parsing:**
- tree-sitter (no network integration) - Compile-time library for grammar parsing; grammars statically linked into binaries

**No external service integrations:** code2graph is a pure library with no runtime API calls, network requests, or service dependencies. All parsing and resolution is in-process.

## Data Storage

**Databases:**
- None. Library is stateless; no persistence layer.

**File Storage:**
- None. Library accepts source code as in-memory strings; does not read/write files (caller's responsibility).

**Caching:**
- None. All extraction and resolution is deterministic and synchronous.

## Authentication & Identity

**Auth Provider:**
- None. No authentication required for library usage.

## Monitoring & Observability

**Error Tracking:**
- None. Library errors are returned as `Result<T, code2graph::Error>` in Rust; caught by bindings and converted to exceptions (Python) or `napi::Error` (Node.js).

**Logs:**
- None. Library produces no output streams or logs. Caller controls visibility.

## CI/CD & Deployment

**Hosting:**
- GitHub: Repository hosted at `github.com/nodedb-lab/code2graph`
- CI runners: GitHub Actions (ubuntu-latest, macos-latest, windows-latest)

**CI Pipeline:**

**Workflows:**

1. **`.github/workflows/ci.yml`** - PR gate
   - Triggered: On PR to `main` (non-draft only), or manual dispatch
   - Concurrency: Cancels superseded runs on same ref
   - Gate job: `ci.yml:test.yml` (reuses test suite from test.yml)
   - Purpose: Prevents merging without passing lint + test

2. **`.github/workflows/test.yml`** - Reusable test suite
   - Used by: `ci.yml` (PR gate) and `release.yml` (pre-publish validation)
   - Jobs:
     - `lint`: rustfmt, clippy, rustdoc (all features)
     - `test`: cargo test on Ubuntu/macOS/Windows (excludes cdylib crates)
     - `bindings`: Build Python wheel (maturin) and Node addon (napi)
       - Validates committed `bindings/node/index.js` and `index.d.ts` match generated outputs
   - File: `.github/workflows/test.yml`

3. **`.github/workflows/release.yml`** - Tag-driven release automation
   - Triggered: On push of tag matching `v*` (semver: `vX.Y.Z` or `vX.Y.Z-beta.N`)
   - Concurrency: Single release at a time; no cancellation
   - Jobs (sequential with dependencies):
     1. `validate-version`: Verify tag format; extract version string
     2. `ci`: Run test suite (must pass before publishing)
     3. `publish-crates`: Publish `code2graph` to crates.io
     4. `build-sdist`: Build Python source distribution (sdist)
     5. `build-wheels`: Build Python wheels (6 matrix targets: Linux gnu/musl, macOS x86_64/aarch64, Windows)
     6. `publish-pypi`: Publish wheels + sdist to PyPI (Trusted Publishing / OIDC)
     7. `build-node`: Build Node.js native modules (6 matrix targets)
     8. `publish-npm`: Assemble platform-specific npm packages, publish to npm
     9. `github-release`: Create GitHub release with auto-generated notes
   - File: `.github/workflows/release.yml`

## Environment Configuration

**Required env vars (Release CI):**

*Crates.io Publishing:*
- `CARGO_REGISTRY_TOKEN` - GitHub Actions secret for crates.io API token
  - Set in: GitHub repo → Settings → Secrets and variables → Actions → `CARGO_REGISTRY_TOKEN`
  - Used by: `release.yml:publish-crates` (line 78)

*PyPI Publishing:*
- No static token required. Uses Trusted Publishing (OIDC) with GitHub OIDC provider.
  - Configured in: GitHub PyPI project's Trusted Publishers settings
  - Used by: `release.yml:publish-pypi` (line 174, `pypa/gh-action-pypi-publish@release/v1`)

*npm Publishing:*
- `NPM_TOKEN` - GitHub Actions secret for npm registry token
  - Set in: GitHub repo → Settings → Secrets and variables → Actions → `NPM_TOKEN`
  - Used by: `release.yml:publish-npm` (line 274)
- `NPM_CONFIG_PROVENANCE` - Set to `"true"` in release.yml (line 275) to enable npm provenance attestation

**Build Environment Variables:**

*Python Wheels (C compilation):*
- `CFLAGS=-std=gnu11` - Set in Docker for manylinux cross-compile builds to enable C11 (tree-sitter grammars require C99 for-loops and C11 static_assert)
  - Applied: `.github/workflows/release.yml` line 154 (Linux targets only)

*Node.js Addon (C compilation):*
- `CFLAGS=-std=gnu11` - Set for non-Windows targets to enable C11
  - Applied: `.github/workflows/release.yml` line 230 (Linux/macOS only; Windows MSVC omitted)

## Webhooks & Callbacks

**Incoming:**
- None. Pure library; no webhook endpoints.

**Outgoing:**
- None. No outbound notifications or callbacks from library code.

## Publishing & Registry Strategy

**Multi-ecosystem Release Coordinated by Version Tag:**

The release pipeline publishes from a **single source version** (workspace root `Cargo.toml [workspace.package] version`) to three registries:

1. **Crates.io** (Rust package: `code2graph`)
   - Published by: `release.yml:publish-crates` (line 76-86)
   - Idempotent check: Queries crates.io API before publishing; skips if version already exists
   - Command: `cargo publish -p code2graph --allow-dirty --no-verify`

2. **PyPI** (Python package: `code2graph-rs`)
   - Published by: `release.yml:publish-pypi` (lines 160-174)
   - Built by: `build-wheels` and `build-sdist` matrix jobs
   - Wheel matrix (6 targets):
     - `x86_64-unknown-linux-gnu` (manylinux auto)
     - `aarch64-unknown-linux-gnu` (manylinux auto)
     - `x86_64-unknown-linux-musl` (musllinux_1_2 with zig)
     - `x86_64-apple-darwin` (cross-compiled from macos-latest)
     - `aarch64-apple-darwin` (native on macos-latest)
     - `x86_64-pc-windows-msvc` (native on windows-latest)
   - Version stamping: Converts Rust semver to PyPI semver (e.g., `0.1.0-alpha.1` → `0.1.0a1`)
     - Script: `.github/workflows/release.yml` lines 97, 141-143
   - Source dist: Built on Ubuntu for portability

3. **npm** (Node.js package: `@nodedb-lab/code2graph`)
   - Published by: `release.yml:publish-npm` (lines 237-288)
   - Built by: `build-node` matrix job (same 6 targets as Python)
   - Publishing: Assemble platform-specific packages via `napi` CLI; publish main package + platform packages
     - Platform package names: `@nodedb-lab/code2graph-{target}` (e.g., `@nodedb-lab/code2graph-linux-x64`)
     - Main package: Detects platform at install time and downloads matching `.node` module
   - Idempotent: Checks if version already published on npm before uploading (lines 281-282)

## Cross-Platform Build Matrix

**Python Wheels (maturin):**

| Target | OS | Runner | C Standard | Manylinux |
|--------|----|----|---|---|
| `x86_64-unknown-linux-gnu` | Linux | ubuntu-latest | gnu11 (docker) | auto |
| `aarch64-unknown-linux-gnu` | Linux | ubuntu-latest | gnu11 (docker) | auto |
| `x86_64-unknown-linux-musl` | Linux | ubuntu-latest | (N/A zig) | musllinux_1_2 |
| `x86_64-apple-darwin` | macOS | macos-latest (AS) | native | N/A |
| `aarch64-apple-darwin` | macOS | macos-latest | native | N/A |
| `x86_64-pc-windows-msvc` | Windows | windows-latest | native | N/A |

**Node.js Modules (napi-rs):**

| Target | Runner | Special Toolchain |
|--------|--------|---|
| `x86_64-unknown-linux-gnu` | ubuntu-latest | none |
| `aarch64-unknown-linux-gnu` | ubuntu-latest | `@napi-rs/cross-toolchain` (aarch64 cross-GCC) |
| `x86_64-unknown-linux-musl` | ubuntu-latest | `cargo-zigbuild` + `ziglang` (musl cross-compile) |
| `x86_64-apple-darwin` | macos-latest | Cross-compile on AS runner (no Intel runners) |
| `aarch64-apple-darwin` | macos-latest | Native build |
| `x86_64-pc-windows-msvc` | windows-latest | Native MSVC |

## Release Trigger & Validation

**Tag Format:**
- Pattern: `v[0-9]+.[0-9]+.[0-9]+` optionally followed by `-[a-zA-Z]+.[0-9]+`
- Examples: `v0.1.0`, `v1.2.3-beta.1`, `v0.2.0-alpha.2`
- Validation: Performed by `validate-version` job (regex + cargo metadata check)
- File: `.github/workflows/release.yml` lines 31-56

**Version Stamping:**
- Crates.io: Takes version from tag; stamps into `Cargo.toml` line 0
- PyPI: Converts tag to PEP 440 format (alpha/beta/rc); stamps into `bindings/python/pyproject.toml`
- npm: Stamps same version into `bindings/node/package.json`

**Prerelease Detection:**
- If tag contains `-`, marked as prerelease in GitHub release
- File: `.github/workflows/release.yml` lines 50-54

## Development Workflow

**PR Gate:**
- File: `.github/workflows/ci.yml`
- Runs test suite (via `.github/workflows/test.yml`) on every non-draft PR
- Prevents merge if lint, test, or binding build fails

**Local CI Equivalent:**

```bash
# Run exact suite that CI runs on PR:
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --no-deps --all-features
cargo test --doc --all-features
cargo test --workspace --all-features --exclude code2graph-py --exclude code2graph-node

# Build and check Python bindings:
cd bindings/python
pip install maturin
maturin build --release -m Cargo.toml

# Build and check Node bindings:
cd bindings/node
npm ci
npx napi build --release --platform
npx napi build --release --platform  # Then verify:
git diff --exit-code -- index.js index.d.ts
```

---

*Integration audit: 2026-07-05*
