"""AllSource Event Store Python SDK."""

from allsource_client.client import AllSourceClient, ReadModel
from allsource_client.async_client import AllSourceAsyncClient, AsyncReadModel
from allsource_client.fold import EventFolder, fold_events
from allsource_client.types import (
    AllSourceError,
    EntityState,
    EpochChangedError,
    Event,
    EventList,
    FeedCursor,
    FeedPage,
    InvalidViewError,
    NotModified,
    PrimeProjection,
    PrimeProjectionAck,
    PrimeProvenance,
    PrimeSnapshot,
    RateLimitedError,
    ReadModelCommit,
    ReadModelPage,
    SyncCheckpoint,
    SyncDelete,
    SyncItem,
    SyncReset,
    SyncUpsert,
    VersionConflictError,
    ViewChanges,
    ViewEntitiesPage,
    ViewInfo,
    ViewLatest,
    ViewNotFound,
    ViewResetError,
    ViewRow,
    ViewSummary,
)

__all__ = [
    "AllSourceClient",
    "AllSourceAsyncClient",
    "ReadModel",
    "AsyncReadModel",
    "EventFolder",
    "fold_events",
    "AllSourceError",
    "EntityState",
    "EpochChangedError",
    "Event",
    "EventList",
    "FeedCursor",
    "FeedPage",
    "InvalidViewError",
    "NotModified",
    "PrimeProjection",
    "PrimeProjectionAck",
    "PrimeProvenance",
    "PrimeSnapshot",
    "RateLimitedError",
    "ReadModelCommit",
    "ReadModelPage",
    "SyncCheckpoint",
    "SyncDelete",
    "SyncItem",
    "SyncReset",
    "SyncUpsert",
    "VersionConflictError",
    "ViewChanges",
    "ViewEntitiesPage",
    "ViewInfo",
    "ViewLatest",
    "ViewNotFound",
    "ViewResetError",
    "ViewRow",
    "ViewSummary",
]

# Read from the installed distribution metadata so this can never drift from
# the version in pyproject.toml. It previously hard-coded "0.1.0" while the
# package shipped as 0.22.0 — the kind of wrong answer that gets baked into a
# release and then quoted back at you in bug reports.
from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("allsource-client")
except PackageNotFoundError:  # running from a source checkout, not installed
    __version__ = "0.0.0+local"
