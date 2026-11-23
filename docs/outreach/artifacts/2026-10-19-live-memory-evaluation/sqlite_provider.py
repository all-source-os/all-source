"""Synthetic standalone provider for Hermes outage evaluation; not a vendor adapter."""
import json
import sqlite3
from pathlib import Path

from agent.memory_provider import MemoryProvider


class ProbeSQLiteProvider(MemoryProvider):
    @property
    def name(self):
        return "outage-probe-sqlite"

    def is_available(self):
        return True

    def initialize(self, session_id, **kwargs):
        self.path = Path(kwargs["hermes_home"]) / "outage-probe.sqlite3"
        with sqlite3.connect(self.path) as db:
            db.execute("CREATE TABLE IF NOT EXISTS memories (key TEXT PRIMARY KEY, value TEXT NOT NULL)")

    def get_tool_schemas(self):
        return [
            {"name": "probe_memory_write", "description": "Durably store exact key and value in external SQLite memory.",
             "parameters": {"type": "object", "properties": {"key": {"type": "string"}, "value": {"type": "string"}}, "required": ["key", "value"]}},
            {"name": "probe_memory_recall", "description": "Recall external SQLite memory. Omit key to list all stored facts.",
             "parameters": {"type": "object", "properties": {"key": {"type": "string"}}, "required": []}},
        ]

    def handle_tool_call(self, tool_name, args, **kwargs):
        with sqlite3.connect(self.path) as db:
            if tool_name == "probe_memory_write":
                db.execute("INSERT OR REPLACE INTO memories VALUES (?, ?)", (args["key"], args["value"]))
                return json.dumps({"success": True, "destination": "external_sqlite", "key": args["key"]})
            if tool_name == "probe_memory_recall":
                rows = db.execute("SELECT key, value FROM memories WHERE key = ?", (args["key"],)).fetchall() if args.get("key") else db.execute("SELECT key, value FROM memories ORDER BY key").fetchall()
                return json.dumps({"success": True, "destination": "external_sqlite", "memories": dict(rows)})
        raise ValueError(tool_name)

    def system_prompt_block(self):
        return "External outage-probe-sqlite memory is active. Use probe_memory_write and probe_memory_recall when explicitly requested."


def register(ctx):
    ctx.register_memory_provider(ProbeSQLiteProvider())
