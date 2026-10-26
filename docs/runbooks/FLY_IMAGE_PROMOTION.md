# Promote tested images to Fly

Use the `Promote Fly Image` workflow after the target main commit passes CI,
security scanning and Docker Build. It copies an existing image into the
corresponding Fly registry. It does not deploy or change runtime configuration.

The workflow accepts `core`, `control-plane`, `query-service` or `prime`, runs
only on `main`, and checks the image revision label against the workflow commit
before pushing. Core uses `runtime-alpine`, preserving UID 1000 for its existing
volume. The Docker Build filter includes the root lockfile so dependency patches
rebuild Core images.

Docker Build publishes a Prime image only when Prime paths change, so a main
commit without one needs Docker Build dispatched on `main` with
`force_build=true` before promotion. Container scanning covers the four
deployment images, not Prime; Prime's dependencies are audited from
`apps/prime-mcp/Cargo.lock`. Deploy Prime with
`fly deploy --config apps/prime-mcp/fly.toml --image registry.fly.io/allsource-prime:sha-<full-commit>`.

Docker Build rebuilds an image only when its sources change: Core images on
`apps/core/**`, the root manifests and lockfile or the Core build workflow;
control-plane on `apps/control-plane/**`; query-service on `apps/query-service/**`.
To promote a component, pass `revision`: the full SHA of the newest main commit
whose Docker Build published that component's image. It must be an ancestor of
the dispatched commit, and the image's revision label must match it. Leaving it
empty uses the dispatched commit.

For a PR candidate, dispatch Security Scanning with `scan_published_images=true`
after Docker Build succeeds. Main scans run automatically when its Docker Build
finishes. Scans pull the commit-tagged images, verify their full revision labels
and refuse fixed HIGH or CRITICAL vulnerabilities. They do not rebuild images.
An image that was not rebuilt at the commit is skipped with a notice; any other
registry error fails the scan.
The full SARIF report retains all severities; a separate filtered scan enforces
the HIGH/CRITICAL refusal policy.

Core builds recover a dependency-only compiler seed from the immutable public
community cache produced by run 39928512614 at commit
`a8b94f3998563da014a29667e3c38a4478dcf5fa`. The recovery recipe ends immediately
after dependency cooking, before copying application source. The current lockfile
remains authoritative: Cargo rebuilds changed dependencies and application code.
The seed is a compiler input and must never be deployed as an application.
Its default image is pinned by digest in the Core Dockerfile. CI also passes the
recovery job's exact image digest into every Core build phase.

Dependency cooking runs up to three times with the same command. The warm and
resume phases keep the finished units when they reach their 200-second deadline
(exit 124), and the dependency phase resumes from them under 240 seconds and must
complete. A lockfile change that alters feature sets shared by datafusion measured
about 480 seconds of cooking from the seed.

Application library, binary and migration phases export separate cache tags under
five-minute CI deadlines. Server compilation has a 255-second budget inside its
285-second build phase. Server binaries use ThinLTO and 16 codegen units with
cross-module function importing disabled. Optimization level 3 and edition
features are retained throughout the dependency and application builds.
The release proof checks hard refusal,
recovery and read throughput against the previous enterprise binary before
promotion. Revision labels are applied in runtime stages so a SHA change does not
invalidate compiler layers. Each phase owns its tag, preserving later compiled
layers when an earlier phase is exported.
Every compiler phase imports only its immediate predecessor's cache graph.
Combining graphs with different snapshots of the same dependency cook can replay
earlier compilations inside a later deadline. Separate tags retain each result
after local BuildKit garbage collection.

The advisory Criterion job compiles its harness separately with the same
server-only compiler overrides, then compares candidate and base using identical
short measurement windows. It shares the target directory between both builds,
requires matching nonempty case sets, and reports changes above 15% as warnings.
Benchmark-only working manifests omit CLI targets to prevent Cargo from linking
unrelated binaries. Library dependencies, features and production manifests stay
unchanged. Native dependencies compile before the final harness codegen phase.
Deterministic instruction-count gates keep their existing 3% failure threshold.
Their library-only dependency, harness and execution phases have separate
deadlines and retain the original release optimization profile.
Prime prepares image dependencies, Core dependencies and remaining native
dependencies in separate bounded phases. The full real/skeleton fingerprint
comparison remains mandatory; Cargo rebuilds packages when feature selections
differ between phases.
Prime container builds export separate image, Parquet and Core library caches
before compiling the final binary. Each phase retains a five-minute ceiling.
Instruction dependencies cook from a generated skeleton before real application
compilation. The current manifests select dependency features in both phases.

1. Create a short-lived Fly deploy token scoped to the selected app. Store it in
   the repository Actions secret `FLY_IMAGE_PROMOTION_TOKEN`. Never print the
   token or put it in workflow inputs, command arguments or logs.
2. Dispatch `promote-fly-image.yml` on `main` with the selected component. The
   GitHub token reads GHCR; the Fly token can publish only to its app registry.
3. Confirm the run succeeded and inspect the destination manifest. Deploy the
   recorded `registry.fly.io/<app>:sha-<full-commit>` image with the matching
   committed Fly configuration. Verify the running image and health checks.
4. Delete the temporary Actions secret when the run ends, including after a
   failure. Use a new app-scoped token for another component.

The workflow checks out the dispatched main commit and refuses an image carrying
a different revision. If main advances, inspect the selected run's SHA and use
that exact SHA for deployment. Never infer the deployed revision from `latest`.

Registry copying and application deployment are separate operations. Existing
read flows, budget refusal and recovery must be checked after deployment.
The control plane and query service deploy bluegreen behind the consumer access
check: see [CONSUMER_ACCESS_GATE.md](CONSUMER_ACCESS_GATE.md).
