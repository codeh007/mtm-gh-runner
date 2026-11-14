# mtm-gh-runner

A Linux x86-64 runner launcher and GitHub Actions runner image. The npm package includes a glibc binary and does not support macOS, Windows, or other CPU architectures.

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
docker pull ghcr.io/codeh007/mtm-gh-runner:0.3.3
```

Image tags match the Cargo and npm versions and are immutable. Build locally with `npm run image`.

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
