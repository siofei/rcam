# S1-B2b public evidence

Final run: s1b2b-20260915-125200. macOS arm64 local headless implementation gates only.
See docs/S1_B2B_REVIEW.md for scope, case IDs, results and remaining work.

- gates/: stdout/stderr and command exit codes. 12-source-manifest.log is the final source manifest check.
- workflow/: synthetic source/output Gerbers and real service JSON request/response sequences.
- tested-source-hashes.json: tested Rust, crate manifests, shaders and locked workspace/toolchain files.
- release.sha256: the local release binary hash; no binary included here.
- acceptance-results.json: local stage result; full case/platform slots remain unexecuted.
- development/: retained failures and intermediate results, not final stage acceptance.

Only the absolute workspace root in JSON/log/text is replaced with `$WORKSPACE`.
Unredacted originals remain under evidence/ in the local workspace. Gerber bytes and tested
source files are unchanged. No private Gerber samples or fonts are copied.
The order-guard baseline is a debug measurement, not release performance acceptance.
