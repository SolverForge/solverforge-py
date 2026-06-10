from __future__ import annotations

from dataclasses import dataclass

from . import _native


@dataclass(frozen=True)
class UiAsset:
    path: str
    content_type: str
    cache_control: str
    bytes: bytes


def asset(path: str) -> UiAsset | None:
    native = _native.ui_asset(path)
    if native is None:
        return None
    return UiAsset(
        path=str(native["path"]),
        content_type=str(native["content_type"]),
        cache_control=str(native["cache_control"]),
        bytes=bytes(native["bytes"]),
    )


def asset_paths() -> list[str]:
    return list(_native.ui_asset_paths())
