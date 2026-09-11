"""Two-process LangGraph checkpoint restart proof.

Run `write`, stop process, then run `read` with same AllSource API key.
"""

from __future__ import annotations

import argparse
import operator
import os
from typing import Annotated, TypedDict

from langgraph.graph import END, START, StateGraph

from langgraph.checkpoint.allsource import AllSourceSaver


class State(TypedDict):
    messages: Annotated[list[str], operator.add]


def persist(state: State) -> State:
    del state
    return {"messages": ["persisted"]}


def build_graph(checkpointer: AllSourceSaver):
    builder = StateGraph(State)
    builder.add_node("persist", persist)
    builder.add_edge(START, "persist")
    builder.add_edge("persist", END)
    return builder.compile(checkpointer=checkpointer)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=("write", "read"))
    parser.add_argument("--thread-id", default="allsource-restart-proof")
    parser.add_argument(
        "--base-url",
        default=os.environ.get("ALLSOURCE_BASE_URL", "https://api.all-source.xyz"),
    )
    args = parser.parse_args()
    api_key = os.environ.get("ALLSOURCE_API_KEY")
    if not api_key:
        parser.error("ALLSOURCE_API_KEY is required")

    config = {"configurable": {"thread_id": args.thread_id}}
    with AllSourceSaver(api_key=api_key, base_url=args.base_url) as checkpointer:
        graph = build_graph(checkpointer)
        if args.phase == "write":
            result = graph.invoke({"messages": ["start"]}, config)
            print(f"Stored before restart: {result['messages']}")
            return

        state = graph.get_state(config)
        print(f"Recovered after restart: {state.values['messages']}")


if __name__ == "__main__":
    main()
