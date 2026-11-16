"""Run engine. Stub: replaced by the engine work package.

Public interface used by other modules:
- engine_registry.active_runs() -> list of objects with .run_id, .endpoint_id, .prompt_set_id
- await engine_registry.recover()   on startup: mark interrupted runs paused, reset in-flight samples
- await engine_registry.shutdown()  on shutdown: stop workers, persist counters
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class ActiveRun:
    run_id: int
    endpoint_id: int | None
    prompt_set_id: int


class EngineRegistry:
    def active_runs(self) -> list[ActiveRun]:
        return []

    async def recover(self) -> None:
        return None

    async def shutdown(self) -> None:
        return None


engine_registry = EngineRegistry()
