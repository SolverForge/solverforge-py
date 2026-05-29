from __future__ import annotations

from . import _native


def init() -> None:
    _native.init_console()
