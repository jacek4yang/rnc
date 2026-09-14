# Repository instructions

- All code and repository changes must be made on a feature/fix branch and
  submitted through a GitHub pull request. Never push directly to main, force
  push main, bypass required checks, or weaken branch protection to merge.
- Keep the required CI job names `check (windows-latest)` and
  `check (ubuntu-latest)` stable unless branch protection is updated with them.
- Use squash merge only after required checks pass and discussions are resolved.
  A separate human approval is not required under the current solo-maintainer
  policy. Do not manufacture approval using another account.
- Follow CONTRIBUTING.md for validation and architectural requirements.
- Keep network bytes independent of encoding. Redirected stdin/stdout remain raw;
  console conversions belong at the Windows terminal boundary.
- Do not commit binaries, dist/, target/, secrets or Python caches. Keep Cargo.lock
  and third-party license notices in Git. Benchmark claims require recorded data.
- Do not create or publish release tags or assets unless requested; ordinary CI
  artifacts and local packaging are allowed as part of validation.
