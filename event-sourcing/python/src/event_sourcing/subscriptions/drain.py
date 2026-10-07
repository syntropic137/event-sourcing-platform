"""
Background drain for a ProcessManager's to-do list.

A ProcessManager's ``process_pending()`` runs external side effects and can
take seconds. The coordinator used to await it inline while dispatching an
event, and every projection at head shares one track and so one cursor, so a
single slow ProcessManager held every read model to its pace (#1528 - a 6-25s
drain kept 28 projections at about one event per 10s).

``ProcessManagerDrain`` takes the drain off the cursor. The coordinator only
``wake()``s it after delivering a live event; the drain runs on a task of its
own. That task gives three properties by construction:

- Single-flight: one task per ProcessManager, so ``process_pending()`` never
  runs twice at once for the same instance.
- Coalescing: a wake is a flag, not a queue. Any number of wakes while a
  drain is running cause exactly one follow-up drain, which sees every item
  written meanwhile.
- Gated at the point of call: ``may_run`` is asked immediately before each
  ``process_pending()``, with no await in between, so a wake raised while
  live and consumed after the track re-entered catch-up does nothing
  (ADR-025: never process during replay).

See docs/adrs/ADR-025-process-manager-pattern.md (amendment for #1528).
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable

    from event_sourcing.core.process_manager import ProcessManager

logger = logging.getLogger(__name__)


class ProcessManagerDrain:
    """Runs one ProcessManager's ``process_pending()`` off the dispatch path.

    The task is started lazily on the first ``wake()``, because a drain is
    created with the coordinator and that may happen outside a running loop.
    ``close()`` stops it; a later ``wake()`` starts a fresh one, so a
    coordinator can be stopped and started again.
    """

    def __init__(self, process_manager: ProcessManager, may_run: Callable[[], bool]) -> None:
        """
        Args:
            process_manager: The ProcessManager whose to-do list this drains.
            may_run: True when processing is allowed right now - for the
                coordinator, when this ProcessManager's track is live.
        """
        self._process_manager = process_manager
        self._may_run = may_run
        self._wake = asyncio.Event()
        self._idle = asyncio.Event()
        self._idle.set()
        self._task: asyncio.Task[None] | None = None

    def wake(self) -> None:
        """Ask for a drain. Never blocks; repeated wakes coalesce."""
        self._wake.set()
        self._idle.clear()
        if self._task is None or self._task.done():
            self._task = asyncio.get_running_loop().create_task(
                self._run(),
                name=f"process-manager-drain:{self._process_manager.get_name()}",
            )

    async def settled(self) -> None:
        """Wait until no drain is running and no wake is pending."""
        await self._idle.wait()

    async def close(self) -> None:
        """Cancel the drain task, wait for it to finish, and drop any pending wake.

        An in-flight ``process_pending()`` is cancelled. That is safe because
        the ProcessManager contract requires it to be idempotent: the next
        live drain picks up whatever this one left pending.
        """
        task, self._task = self._task, None
        self._wake.clear()
        if task is not None:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task
        self._idle.set()

    async def _run(self) -> None:
        while True:
            await self._wake.wait()
            self._wake.clear()
            await self._drain_once()
            if not self._wake.is_set():
                self._idle.set()

    async def _drain_once(self) -> None:
        name = self._process_manager.get_name()
        # Checked here, at the moment of the call, and not when the wake was
        # raised: the track may have gone back into catch-up in between.
        if not self._may_run():
            return
        try:
            processed = await self._process_manager.process_pending()
        except Exception:
            # A failed drain must not end the task, or this ProcessManager
            # would silently stop processing for the life of the coordinator.
            # Items it left pending are retried on the next wake.
            logger.exception(
                "ProcessManager.process_pending() failed",
                extra={"projection_name": name},
            )
            return
        if processed > 0:
            logger.info(
                "ProcessManager processed pending items",
                extra={"projection_name": name, "items_processed": processed},
            )
