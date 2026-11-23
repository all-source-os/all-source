# Live memory evaluation artifacts

Date: 2026-10-19. [Report and updated drafts](../../2026-10-19-live-memory-evaluation.md).

## Evidence index

- [Results summary](./results-summary.json): human-reviewed case scoring and exact recall text.
- [Manifest](./manifest.json): source refs, image digests, settings and collection limits.
- [Execution receipts](./execution-receipts.json): recorded stage commands/settings and exit outcomes; Docker container IDs were not retained.
- [Nanobot runtime](./nanobot-runtime.json), [installed dependencies](./nanobot-dependencies.json), [requirements](./nanobot-requirements.txt).
- [Hermes runtime](./hermes-runtime.json), [installed dependencies](./hermes-dependencies.json), [requirements](./hermes-requirements.txt).
- [Corpus](./corpus.json), [native nanobot harness](./run_nanobot_eval.py), [native history control](./session_control.py).
- [Hermes harness](./hermes_probe_stage.py), [synthetic provider](./sqlite_provider.py), [profile config](./hermes-config.yaml), [plugin manifest](./plugin.yaml).
- [Correction archive/Dream trace](./nanobot/explicit-correction-over-inference/consolidate.json), [fresh projection recall](./nanobot/explicit-correction-over-inference/recall.json), [history-enabled diagnostic](./nanobot/explicit-correction-over-inference/retrieval.json).
- [Scope archive/Dream trace](./nanobot/situational-preference-stays-scoped/consolidate.json), [fresh projection recall](./nanobot/situational-preference-stays-scoped/recall.json), [history-enabled diagnostic](./nanobot/situational-preference-stays-scoped/retrieval.json).
- [Delivery archive/Dream trace](./nanobot/retain-complete-proposal-reference/consolidate.json), [fresh projection recall](./nanobot/retain-complete-proposal-reference/recall.json), [history-enabled diagnostic](./nanobot/retain-complete-proposal-reference/retrieval.json).
- [Native original-session control](./nanobot/session-store-control.json): seeded originals read through nanobot's own engine.
- Hermes [baseline](./hermes/baseline.json), [outage](./hermes/outage.json), [restore](./hermes/restore.json), [startup warning stdout](./hermes/outage-stdout.log).

Full traces contain synthetic messages, native prompts, tool requests/results and usage as returned by the runtime. No model key, production home, database, inbox or messaging credential is included. Corpus IDs are evaluation metadata; native session timestamps reflect fixture insertion.

## Reproduction

Use the pinned public sources in manifest and Python image digests. Source and dependencies are not vendored here. Install each upstream manifest's runtime dependencies as wheels into its own scratch dependency directory; recorded installed versions identify this run. Do not share the operator's real home or profile.

For nanobot, put run_nanobot_eval.py and corpus.json in a scratch harness directory, with upstream root and dependency directory separate. For each case 0, 1, 2, run consolidate, then recall, then optional retrieval in a **new container** for each stage. Reuse only that case's evidence directory.

The actual command shape is below. Replace the four /absolute paths and provide an owner-created mode-0600 env file containing only OPENROUTER_API_KEY. Do not commit the env file. Every stage must remain serial to avoid the provider's in-flight reservation limit.

~~~console
gtimeout -k 5 300 docker run --rm --read-only --tmpfs /tmp --tmpfs /home/probe --env HOME=/home/probe --env PYTHONPATH=/src:/deps --env PYTHONDONTWRITEBYTECODE=1 --env-file /absolute/model.env --memory 2g --cpus 2 --pids-limit 128 --cap-drop ALL --security-opt no-new-privileges --mount type=bind,src=/absolute/nanobot,dst=/src,readonly --mount type=bind,src=/absolute/nanobot-deps,dst=/deps,readonly --mount type=bind,src=/absolute/harness,dst=/harness,readonly --mount type=bind,src=/absolute/evidence,dst=/evidence python@sha256:a6e34c598f2467ed0e9a8d349809fcd8b5c603269512df273a0bb1784edc11b1 python /harness/run_nanobot_eval.py --source /src --corpus /harness/corpus.json --root /evidence/serial-bounded --model openai/gpt-4.1-mini --stage consolidate --case 0
~~~

Native process alarm is 270 seconds, provider call timeout 90, stage async budget 285, external deadline 300. Reproduce recall/retrieval by changing only stage and case. The corpus correction/scope/value assertions require human review of outputs; script success alone does not establish a pass. Inspect provider finish reasons, tool results, memory files and expected source text.

For Hermes, use its Python 3.14 image, source, dependency directory, harness and disposable profile. Install sqlite_provider.py at profile/plugins/outage-probe-sqlite/__init__.py with plugin.yaml beside it and hermes-config.yaml as profile/config.yaml. Set HERMES_HOME=/profile, PROBE_OUTPUT=/evidence, PROBE_MODEL=openai/gpt-4.1-mini, PYTHONPATH=/src:/deps and HOME=/home/probe. Mount only those scratch directories. Run hermes_probe_stage.py baseline, remove only the disposable plugin directory, run outage in another container, restore the same plugin and run restore in another container. Reuse profile volume and evidence directory; no production mounts or Docker socket.

The synthetic plugin reads/writes SQLite through sqlite3 and normal MemoryProvider methods. It has no mirroring hook. This is not a production vendor migration test.

Python is used for native upstream integration, not a reimplementation of the engines. No Rust comparison or AllSource adapter is included.

## Interpretation boundaries

The projection recall arm has history tools disabled. The retrieval arm separately seeds the original corpus through native SessionManager immediately before retrieval, so it proves source lookup availability, not original capture across the preceding archive/Dream lifecycle. No claim about production history preservation follows from that control.

The Hermes outage prompt tells the model that the plugin is missing and asks it to distinguish destinations. Startup warning is independent runtime evidence; the model's final explanation is not evidence of unprompted discovery. No power-loss, concurrency, load, production adapter or current-main comparison was performed.
