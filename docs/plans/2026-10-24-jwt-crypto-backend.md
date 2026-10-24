# JWT crypto backend

Core uses HS256 tokens through `jsonwebtoken` 11.1. Its RustCrypto backend brings in `crypto-bigint` 0.5.5 through elliptic-curve dependencies, triggering RUSTSEC-2026-0387. The existing security audit refuses this dependency.

Select `jsonwebtoken`'s AWS-LC backend instead. AWS-LC already exists in the workspace dependency graph. Preserve token algorithms, issuer validation, signing secrets and claim handling. Remove the unused RustCrypto elliptic-curve dependency chain from the lockfile; keep the advisory policy unchanged.

Validation covers independently signed HS256 tokens through Core's `AuthManager`, rejection of wrong signatures, expired tokens and algorithm mismatch, existing authentication tests, and the unchanged dependency audit. Deployment must use the validated image and preserve existing configuration and capacity.

Local results: 54 security tests and one JWT compatibility test passed. Scoped Clippy passed with warnings denied. The CI advisory-audit command passed with its existing exceptions unchanged. A separate `cargo deny` comparison against the pre-change graph found no added errors and removed the crypto-bigint and RSA vulnerability findings; existing license, archive-extraction and unmaintained-dependency failures remain in that separate policy.
