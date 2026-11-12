# Releasing to npm

The root package contains the JavaScript loader, types and documentation. Each
native binary belongs to a separate platform package. Publish those platform
packages before publishing the root package at the same version.

## Repository and package contents

Keep Rust source, generated loader/types, both lockfiles, build configuration,
tests, the portable benchmark, allocation example and release scripts in Git.
Generated binaries, `npm/`, downloaded `artifacts/`, packed `release/` tarballs,
local investigation files, credentials and dependency directories are ignored.

The root npm tarball contains `index.js`, `index.d.ts`, README, changelog, license
and third-party notices. Platform tarballs contain one native binary and their
license notices. Rust source, tests, benchmarks and release tooling stay in Git.

## Version and build

Set the same version in package.json and Cargo.toml, then update package-lock.json
and Cargo.lock. This release is 2.0.0. Keep generated loader/types in sync with the
Rust exports by running the release build before tagging.

Review third-party notices whenever Cargo.lock changes; preparation copies those
notices into every platform package.

Run the CI workflow from the intended release commit with publication disabled.
It checks Rust, builds every configured target, tests the native hosts and prepares
all npm packages. Download the `npm-release` artifact and inspect its tarballs.
The target list must match package.json and the build matrix.

For a local release workspace, collect every target binary under `artifacts/`,
then run:

```sh
npm ci
npm run release:prepare
npm run release:check
npm exec -- napi prepublish -t npm --no-gh-release --dry-run
npm run pack:preview
```

`release:prepare` generates platform manifests, collects binaries, copies license
notices and writes exact-version optional dependencies into the root manifest.
It does not publish. Missing or stale artifacts fail preparation. The root
`prepack` hook repeats the read-only check before a normal pack or publish.

Do not commit the generated optional dependency edits from this release workspace;
the source manifest and lockfile describe the development checkout. Preparation
creates the final npm manifest after the platform builds have finished.

## Publish

Configure the repository's `NPM_TOKEN` secret with access to the root package and
its platform packages. Run the CI workflow from the reviewed release commit with
`publish` enabled. It publishes the platform packages, then the root package, with
provenance enabled. Publication requires an explicit workflow dispatch; pushing a
branch or tag only builds and prepares packages.

To publish manually from a prepared workspace:

```sh
npm run release:check
npm exec -- napi prepublish -t npm --no-gh-release
npm publish --access public
```

The N-API prepublish command publishes platform packages. It is intentionally not
an npm lifecycle hook. Its own `--dry-run` flag suppresses publication; npm's
publish dry-run is not a substitute for that flag.

A stable 2.0.0 release uses the `latest` tag. For a future prerelease, set npm's tag
to `next` for both the platform and root publication steps.

## After publication

Check that every optional dependency version exists in the registry and install
the root package in a fresh project with optional dependencies enabled. Test
CommonJS and ES module imports, `getOpusVersion()`, and one encode/decode roundtrip.
The version string must be `libopus 1.6.1`.

If publication stops partway through, keep the same artifacts and inspect which
name/version pairs already exist. npm versions cannot be overwritten. Resume
with the unchanged packages; do not rebuild different binaries under a version
that has already been published.
