# Contributing

Use the latest stable Rust toolchain. Windows console/shell tests also require
Python 3.10 or later (standard library only). The executable itself has no Python
dependency. See [README](README.md) for platform setup and architecture.

## Branches and pull requests

`main` is protected, including for administrators. All changes go through a
feature/fix branch and a pull request; never push code directly to `main`, force
push it, or disable protections to complete a change.

```sh
git switch main
git pull --ff-only
git switch -c fix/short-description
# Edit and validate.
git add <changed-files>
git commit -m "fix: describe the resulting behavior"
git push -u origin HEAD
gh pr create --base main
```

Before merging, both `check (windows-latest)` and `check (ubuntu-latest)` must pass,
the branch must be up to date with main, and review conversations must be
resolved. Merge with **Squash and merge**; merged branches are deleted automatically.
Do not rename the required CI jobs without updating branch protection to match.

This is currently a solo-maintainer repository. A PR is mandatory, but another
person's approval is not mandatory, because a PR author cannot approve their own
PR. CODEOWNERS routes review requests to the maintainer. If more maintainers join,
the owner can increase the required approval count without changing this workflow.

## Required checks

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
cargo test --release --locked
```

On Windows, `scripts/verify.ps1` runs the core checks and exercises the release
executable through real native consoles and cmd.exe pipes. GitHub CI runs debug
and release tests on Windows and Linux and uploads a Windows ZIP and checksums.
Artifacts are available from successful workflow runs for 14 days.

Keep raw socket transport independent of text encoding. Add regression coverage
for behavioral fixes. Changes affecting Unicode or native handles need Windows
validation, including redirected handles. Explain the invariants around any new
unsafe code. Benchmark performance changes before making performance claims;
record the environment and avoid treating a loopback result as a WAN guarantee.

## Scope and packaging

Keep the CLI small and compatible with the documented netcat subset. Do not add
command execution, a runtime, new protocol layers, or dependencies without an
explicit use case and discussion in the PR.

`./scripts/package.ps1` builds the standalone Windows package. Generated binaries,
ZIPs, build directories and Python caches stay out of Git. Update third-party
license notices when changing dependencies used by the Windows build, and record
user-visible changes in [CHANGELOG.md](CHANGELOG.md).

Report reproducible bugs through the Issue forms. For vulnerabilities, follow
[SECURITY.md](SECURITY.md). Please keep discussions respectful and technical;
personal attacks, harassment and disclosure of others' private data are not welcome.
