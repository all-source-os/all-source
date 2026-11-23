"""Read the saved transcript through nanobot's own session tools, without a model."""
import asyncio
import json
from pathlib import Path

from nanobot.agent.tools.sessions import SearchSessionsTool, ReadSessionTool
from nanobot.session.manager import SessionManager


async def main():
    root = Path('/evidence/serial-bounded')
    corpus = json.loads(Path('/harness/corpus.json').read_text())
    results = []
    for case, query in zip(corpus['cases'], ['light mode', 'mobile onboarding', 'Option 2']):
        case_root = root / case['name']
        sessions = SessionManager(case_root / 'workspace', sessions_root=case_root / 'sessions')
        search = json.loads(await SearchSessionsTool(sessions).execute(query=query))
        read = json.loads(await ReadSessionTool(sessions).execute(session_key=case['session_key']))
        results.append({'case': case['name'], 'query': query, 'search': search, 'read': read})
    (root / 'session-store-control.json').write_text(json.dumps(results, indent=2))
    print(json.dumps([{'case': r['case'], 'matching_sessions': len(r['search']['results'])} for r in results]))


asyncio.run(main())
