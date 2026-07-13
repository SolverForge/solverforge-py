from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

JoinKey = Callable[..., object] | str


@dataclass(frozen=True)
class EqualJoiner:
    left_key: JoinKey
    right_key: JoinKey

    def to_native(self) -> dict[str, object]:
        if isinstance(self.left_key, str) and isinstance(self.right_key, str):
            return {
                "type": "equal_attr",
                "left_attr": self.left_key,
                "right_attr": self.right_key,
            }
        if not callable(self.left_key) or not callable(self.right_key):
            msg = "joiner.equal keys must both be callables or both be attribute names"
            raise TypeError(msg)
        return {
            "type": "equal",
            "left_key": self.left_key,
            "right_key": self.right_key,
        }


def equal(key: JoinKey) -> EqualJoiner:
    return EqualJoiner(key, key)


def equal_bi(
    left_key: JoinKey,
    right_key: JoinKey,
) -> EqualJoiner:
    return EqualJoiner(left_key, right_key)
