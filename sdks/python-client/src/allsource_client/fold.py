"""Client-side folding of an event sequence into a state."""

from __future__ import annotations

from typing import Iterable, Optional, Protocol, TypeVar

from allsource_client.types import Event

S_co = TypeVar("S_co", covariant=True)
S = TypeVar("S")


class EventFolder(Protocol[S_co]):
    """Accumulates state from events, oldest first."""

    def apply(self, event: Event) -> bool:
        """Apply one event; return whether it was relevant."""
        ...

    def finalize(self) -> Optional[S_co]:
        """Return the state, or `None` when no relevant event was applied."""
        ...


def fold_events(folder: EventFolder[S], events: Iterable[Event]) -> Optional[S]:
    """Apply every event to `folder` in order and return its final state."""
    for event in events:
        folder.apply(event)
    return folder.finalize()
