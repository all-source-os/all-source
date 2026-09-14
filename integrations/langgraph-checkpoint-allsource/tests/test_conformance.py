from __future__ import annotations

from langgraph.checkpoint.conformance import checkpointer_test, validate
from test_saver import FakeClient

from langgraph.checkpoint.allsource import AllSourceSaver


@checkpointer_test(name="AllSourceSaver")
async def allsource_checkpointer():
    saver = AllSourceSaver(client=FakeClient())
    try:
        yield saver
    finally:
        saver.close()


async def test_langgraph_base_conformance() -> None:
    report = await validate(allsource_checkpointer)
    required = {"put", "put_writes", "get_tuple", "list", "delete_thread"}

    for capability in required:
        result = report.results[capability]
        assert result.detected is True
        assert result.passed is True, result.failures
