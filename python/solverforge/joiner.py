from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class EqualJoiner:
    left_key: Callable[..., object]
    right_key: Callable[..., object]

    def to_native(self) -> dict[str, object]:
        return {
            "type": "equal",
            "left_key": self.left_key,
            "right_key": self.right_key,
        }


def equal(key: Callable[..., object]) -> EqualJoiner:
    return EqualJoiner(key, key)


def equal_bi(
    left_key: Callable[..., object],
    right_key: Callable[..., object],
) -> EqualJoiner:
    return EqualJoiner(left_key, right_key)
