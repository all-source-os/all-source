"""Run one isolated Hermes stage using a real model and normal agent routing."""
import json
import logging
import os
import signal
import sqlite3
import sys
from pathlib import Path


def main():
    signal.alarm(270)
    stage = sys.argv[1]
    home = Path(os.environ["HERMES_HOME"])
    output = Path(os.environ["PROBE_OUTPUT"])
    output.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(level=logging.INFO, handlers=[logging.FileHandler(output / f"{stage}.log")])
    from run_agent import AIAgent

    events = []

    def event_callback(name, payload):
        events.append({"event": name, "payload": payload})

    warnings = []
    agent = AIAgent(
        provider="openrouter", model=os.environ.get("PROBE_MODEL", "openai/gpt-4.1-mini"),
        api_key=os.environ["OPENROUTER_API_KEY"], base_url="https://openrouter.ai/api/v1",
        enabled_toolsets=["memory"], max_iterations=6, max_tokens=1200,
        quiet_mode=True, skip_context_files=True, skip_background_review=True,
        session_id=f"outage-probe-{stage}", run_budget_seconds=150,
        event_callback=event_callback,
        notice_callback=lambda *args, **kwargs: warnings.append({"args": args, "kwargs": kwargs}),
    )
    prompts = {
        "baseline": "Use probe_memory_write to store key baseline_canary with exact value SYNTHETIC_BASELINE_CANARY_73e9. Then use probe_memory_recall with key baseline_canary. Report the tool result. Do not write this fact to built-in memory.",
        "outage": "The external memory provider was configured but its plugin directory was removed before this fresh process. Store this exact fact durably using the available memory tool: SYNTHETIC_OUTAGE_CANARY_ba42 is a test-only recovery canary. Report which tool and destination you used and any limitations. Do not claim that the external provider received it.",
        "restore": "Use probe_memory_recall with no key to list all external memory facts. Separately report any SYNTHETIC_OUTAGE_CANARY fact already present in the built-in memory supplied to you. Do not copy, repair, or write any memories. Clearly distinguish external memory recall from built-in memory context.",
    }
    try:
        startup = {"manager_active": bool(agent._memory_manager), "providers": [p.name for p in agent._memory_manager.providers] if agent._memory_manager else [], "tools": sorted(agent.valid_tool_names)}
        result = agent.run_conversation(prompts[stage])
    finally:
        agent.close()
    db_path = home / "outage-probe.sqlite3"
    with sqlite3.connect(f"file:{db_path}?mode=ro", uri=True) as db:
        rows = dict(db.execute("SELECT key, value FROM memories ORDER BY key").fetchall())
    builtin_files = {str(p.relative_to(home)): p.read_text() for p in (home / "memories").glob("*.md")}
    evidence = {"stage": stage, "startup": startup, "notices": warnings, "events": events, "conversation": result, "external_sqlite_rows": rows, "builtin_memory_files": builtin_files}
    (output / f"{stage}.json").write_text(json.dumps(evidence, indent=2, default=str))
    print(json.dumps({"stage": stage, "startup": startup, "external_sqlite_rows": rows, "final_response": result.get("final_response")}))


if __name__ == "__main__":
    main()
