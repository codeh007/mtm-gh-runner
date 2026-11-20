# mtm-gh-runner

A Linux x86-64 runner launcher and GitHub Actions runner image. The npm package requires glibc 2.35 or later.

## Install

```sh
npm install --global mtm-gh-runner
```

The host needs Docker and GitHub CLI (`gh`). Set `GH_TOKEN` to a fine-grained token with `Administration: write` on the target repository. Keep that token scoped to the intended repository, then set:

```sh
export GH_TOKEN
export MTM_GH_RUNNER_REPOSITORY=owner/repo
export MTM_GH_RUNNER_LABELS=self-hosted,linux,x64
export MTM_GH_RUNNER_GROUP_ID=1
mtm-gh-runner run
```

Set `MTM_GH_RUNNER_GROUP_ID` to a runner group available to the target repository. Set labels to match the target repository's workflows. For gomtm, use `self-hosted,linux,x64,gomtm` and group ID `1`.

`MTM_GH_RUNNER_CONCURRENCY` optionally fixes the number of runner slots. Without it, the launcher sizes slots from available CPU and memory.

The launcher uses the host `gh` credentials only to request just-in-time runner configuration and remove runner registrations. It does not pass `GH_TOKEN` to jobs. Jobs run in privileged Docker-in-Docker containers, so use this runner only for repositories whose workflows you trust. Job containers do not receive the host Docker socket. Per-slot tool caches persist between jobs.

## Runner image

The image contains the GitHub Actions runner and the toolchain used by gomtm workflows. The launcher starts it for one job at a time.

```sh
docker pull ghcr.io/codeh007/mtm-gh-runner:0.3.5
```

Image tags match the Cargo and npm versions. Keep published tags available for callers pinned to an older version. Build locally with `npm run image`.

## Release

Keep the GHCR package public. If the standalone repository does not inherit write access, grant `codeh007/mtm-gh-runner` the **Write** role under [Manage Actions access](https://docs.github.com/en/packages/learn-github-packages/configuring-a-packages-access-control-and-visibility#ensuring-workflow-access-to-your-package).

Push one `vX.Y.Z` tag at a time. The tag must match the Cargo and npm versions. The [publish workflow](.github/workflows/publish.yml) builds and tests both candidates, publishes the amd64 image, and verifies that an anonymous pull matches the tag's commit before publishing to npm. A rerun reuses a matching image and skips npm publishing if the registry already has the same tarball integrity.

Set the `NPM_TOKEN` Actions secret to an unexpired [npm granular access token](https://docs.npmjs.com/creating-and-viewing-access-tokens/) with publish permission for `mtm-gh-runner` and **Bypass 2FA** enabled. Rotate it before expiry.

To publish the already-built `v0.3.5` package, run the publish workflow manually from `main` while the original release artifact is available. This does not rebuild or push its image. Use tags for subsequent releases.

## Development

Requires Rust 1.90+, Node.js, and Docker. Install Cargo About with its CLI feature before building:

```sh
cargo install cargo-about --locked --features cli
```

Then run:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
npm run build
npm pack --dry-run
npm run image
```
