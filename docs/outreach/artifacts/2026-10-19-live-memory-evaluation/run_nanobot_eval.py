"""Native upstream integration: real archive, Dream, then separate-process recall.

Python is required to call nanobot's Python runtime without reimplementing it.
Run one case/stage per externally bounded process. No configuration is loaded
from the operator's HOME; only OPENROUTER_API_KEY enters the isolated runtime.
"""
import argparse
import asyncio
import json
import os
import signal
from pathlib import Path
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--source', required=True)
parser.add_argument('--corpus', required=True)
parser.add_argument('--root', required=True)
parser.add_argument('--case', type=int, required=True)
parser.add_argument('--stage', choices=['consolidate', 'recall', 'retrieval'], required=True)
parser.add_argument('--model', required=True)
args = parser.parse_args()
signal.alarm(270)
sys.path.insert(0, args.source)

from nanobot.agent.loop import AgentLoop
from nanobot.agent.tools.registry import ToolRegistry
from nanobot.bus.queue import MessageBus
from nanobot.providers.openai_compat_provider import OpenAICompatProvider
from nanobot.providers.generation import GenerationSettings
from nanobot.session.manager import Session, SessionManager

case = json.loads(Path(args.corpus).read_text())['cases'][args.case]
root = Path(args.root) / case['name']
workspace = root / 'workspace'
workspace.mkdir(parents=True, exist_ok=True)
from loguru import logger
logger.remove()
logger.add(root / f'{args.stage}.log', level='INFO')
output = root / f'{args.stage}.json'
records = {'case': case, 'model': args.model, 'stage': args.stage,
           'generation': {'max_tokens': 1024, 'temperature': 0},
           'source_ref': '8778c697744f8334c5794cd37502e8eb77ee5585',
           'calls': [], 'boundaries': [],
           'source_id_limit': 'Corpus IDs retained as metadata; native memory formatter omits IDs. Require truthful source description; do not require unavailable exact IDs.'}

def save():
    output.write_text(json.dumps(records, indent=2, default=str))

def snapshot(label, store):
    records['boundaries'].append({'label': label, 'dream_cursor': store.get_last_dream_cursor(),
        'files': {name: (workspace / name).read_text() if (workspace / name).exists() else None
                  for name in ['SOUL.md', 'USER.md', 'memory/MEMORY.md', 'memory/history.jsonl']}})
    save()

async def main():
    provider = OpenAICompatProvider(api_key=os.environ['OPENROUTER_API_KEY'],
        api_base='https://openrouter.ai/api/v1', default_model=args.model)
    provider.generation = GenerationSettings(max_tokens=1024, temperature=0)
    original = provider.chat_stream_with_retry

    async def traced(*pos, **kw):
        call = {'messages': kw.get('messages'), 'model': kw.get('model'),
                'tools': kw.get('tools'), 'status': 'started'}
        records['calls'].append(call)
        save()
        try:
            result = await asyncio.wait_for(original(*pos, **kw), timeout=90)
            call.update(status='completed', response=vars(result))
            save()
            return result
        except BaseException as error:
            call.update(status='failed', error_type=type(error).__name__)
            save()
            raise

    provider.chat_stream_with_retry = traced
    sessions = None
    if args.stage == 'retrieval':
        sessions = SessionManager(workspace, sessions_root=root / 'sessions')
        source_session = Session(key=case['session_key'])
        for message in case['messages'] + case['second_cycle_messages']:
            source_session.add_message(message['role'], message['content'])
        sessions.save(source_session)
        records['retrieval_arm'] = 'Original synthetic transcript persisted through native SessionManager; retrieval explicitly requested.'
    loop = AgentLoop(MessageBus(), provider, workspace, model=args.model,
                     session_manager=sessions,
                     restrict_to_workspace=True, tool_registry=ToolRegistry(),
                     max_iterations=8, provider_retry_mode='standard')
    store = loop.context.memory
    try:
        snapshot('initial', store)
        if args.stage == 'consolidate':
            previous_summary = None
            for index, key in enumerate(['messages', 'second_cycle_messages'], 1):
                source = case[key]
                archive_history = loop.context.build_messages(
                    history=[{'role': x['role'], 'content': x['content']} for x in source],
                    current_message=None,
                    session_summary=({'text': previous_summary, 'last_active': '2026-10-19'}
                                     if previous_summary else None),
                )
                summary = await loop.consolidator.archiver.archive(source,
                    runtime=loop.llm_runtime(), session_key=case['session_key'],
                    history=archive_history,
                    request_tools=[], previous_summary=previous_summary, persist=True)
                previous_summary = summary
                records[f'archive_{index}'] = summary
                snapshot(f'archive_{index}', store)
                built = store.build_dream_prompt()
                if built is None:
                    raise RuntimeError('Native archive produced no Dream input')
                prompt, cursor = built
                response = await loop.process_direct(prompt,
                    session_key=f'dream:evaluation-{index}', ephemeral=True,
                    tools=store.build_dream_tools())
                completed = store.dream_run_completed(response)
                records[f'dream_{index}'] = {'prompt': prompt,
                    'response': vars(response) if response else None, 'completed': completed}
                if completed:
                    store.set_last_dream_cursor(cursor)
                snapshot(f'dream_{index}', store)
                if not completed:
                    raise RuntimeError(f'Dream cycle {index} did not complete')
        else:
            query = case['fresh_session_query']
            if args.stage == 'retrieval':
                query += ' Use search_sessions and read_session to find the original statement in saved conversations. Distinguish derived memory from the original message; if you cannot find it, say so.'
            response = await loop.process_direct(query,
                session_key='evaluation:fresh-recall',
                tools=(loop.tools if args.stage == 'retrieval' else ToolRegistry()))
            records['recall'] = vars(response) if response else None
            snapshot('fresh_process_recall', store)
        records['status'] = 'completed'
    finally:
        await loop.aclose()
        save()

try:
    asyncio.run(asyncio.wait_for(main(), timeout=285))
except BaseException as error:
    records.update(status='failed', error_type=type(error).__name__)
    save()
    raise
