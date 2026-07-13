from functools import partial
from math import copysign
import sys
from types import FunctionType, ModuleType
from typing import Callable, cast

import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    SoftScore,
    Solver,
    constraint_provider,
    indexed_presence,
    joiner,
    planning_entity,
    planning_list_variable,
    planning_solution,
    planning_variable,
    problem_fact,
    shadow_variable_updates,
)
from solverforge.model import _COMPILED_SCHEMA_CACHE, _compiled_schema_for_solution


class Item:
    pass


class IdentityKey:
    def __eq__(self, other: object) -> bool:
        return self is other

    def __repr__(self) -> str:
        return "same-key-representation"


@planning_entity
class AttributeLeft:
    def __init__(self, key: object) -> None:
        self.key = key
        self.value = 0

    value = planning_variable(value_range_provider="values")

    @property
    def derived_value(self) -> int:
        return int(self.value)


@planning_entity
class AttributeRight:
    def __init__(self, key: object) -> None:
        self.key = key
        self.value = 0

    value = planning_variable(value_range_provider="values")


@constraint_provider
def attribute_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AttributeLeft)
        .join(AttributeRight, joiner.equal_bi("key", "key"))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("attribute join")
    ]


@planning_solution(score=HardSoftScore, constraints=attribute_join_constraints)
class AttributeJoinPlan:
    left_rows: list[AttributeLeft]
    right_rows: list[AttributeRight]

    def __init__(
        self,
        left_keys: list[object] | None = None,
        right_keys: list[object] | None = None,
    ) -> None:
        self.left_rows = [AttributeLeft(key) for key in (left_keys or ["a", "b"])]
        self.right_rows = [AttributeRight(key) for key in (right_keys or ["a", "c"])]
        self.values = [0]
        self.score = None


@constraint_provider
def callback_attribute_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AttributeLeft)
        .join(
            AttributeRight,
            joiner.equal_bi(lambda left: left.key, lambda right: right.key),
        )
        .penalize(HardSoftScore.ONE_SOFT)
        .named("callback attribute join")
    ]


@planning_solution(score=HardSoftScore, constraints=callback_attribute_join_constraints)
class CallbackAttributeJoinPlan(AttributeJoinPlan):
    pass


@planning_entity
class ModuleNamespaceCacheRow:
    value = planning_variable(value_range_provider="values")

    def __init__(self, value: int) -> None:
        self.value = value


_module_namespace_cache_callback: Callable[[ModuleNamespaceCacheRow], bool] | None = (
    None
)


@constraint_provider
def module_namespace_cache_constraints(factory):
    callback = _module_namespace_cache_callback
    assert callback is not None
    return [
        factory.for_each(ModuleNamespaceCacheRow)
        .filter(callback)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("module namespace cache")
    ]


@planning_solution(score=HardSoftScore, constraints=module_namespace_cache_constraints)
class ModuleNamespaceCachePlan:
    rows: list[ModuleNamespaceCacheRow]

    def __init__(self) -> None:
        self.rows = [ModuleNamespaceCacheRow(1)]
        self.values = [0, 1]
        self.score = None


def _module_namespace_cache_callback_template(row: ModuleNamespaceCacheRow) -> bool:
    return int(row.value) > int(globals()["offset"])


def _callback_in_module_namespace(
    name: str, offset: int
) -> Callable[[ModuleNamespaceCacheRow], bool]:
    namespace = ModuleType(name)
    namespace.offset = offset
    sys.modules[name] = namespace
    callback = FunctionType(
        _module_namespace_cache_callback_template.__code__,
        namespace.__dict__,
        "predicate",
    )
    callback.__module__ = name
    return cast(Callable[[ModuleNamespaceCacheRow], bool], callback)


def test_compiled_schema_cache_reuses_provider_created_callback_shape() -> None:
    first = _compiled_schema_for_solution(CallbackAttributeJoinPlan())
    second = _compiled_schema_for_solution(CallbackAttributeJoinPlan())

    assert first is second


def test_compiled_schema_cache_distinguishes_callback_module_namespaces() -> None:
    global _module_namespace_cache_callback
    first_module = "solverforge_test_cache_namespace_one"
    second_module = "solverforge_test_cache_namespace_two"
    _COMPILED_SCHEMA_CACHE.pop(ModuleNamespaceCachePlan, None)
    try:
        first_callback = _callback_in_module_namespace(first_module, offset=0)
        second_callback = _callback_in_module_namespace(second_module, offset=1)

        _module_namespace_cache_callback = first_callback
        first = _compiled_schema_for_solution(ModuleNamespaceCachePlan())
        assert first is _compiled_schema_for_solution(ModuleNamespaceCachePlan())
        assert Solver.analyze(ModuleNamespaceCachePlan()) == {
            "family": "hard_soft",
            "levels": [0, -1],
        }

        _module_namespace_cache_callback = second_callback
        second = _compiled_schema_for_solution(ModuleNamespaceCachePlan())

        assert first is not second
        assert Solver.analyze(ModuleNamespaceCachePlan()) == {
            "family": "hard_soft",
            "levels": [0, 0],
        }
    finally:
        _module_namespace_cache_callback = None
        _COMPILED_SCHEMA_CACHE.pop(ModuleNamespaceCachePlan, None)
        sys.modules.pop(first_module, None)
        sys.modules.pop(second_module, None)


@constraint_provider
def recursive_callback_constraints(factory: ConstraintFactory):
    def recursive_filter(item: AttributeLeft) -> bool:
        if item.value == 0:
            return False
        return recursive_filter(item)

    return [
        factory.for_each(AttributeLeft)
        .filter(recursive_filter)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("recursive callback")
    ]


@planning_solution(score=HardSoftScore, constraints=recursive_callback_constraints)
class RecursiveCallbackPlan:
    left_rows: list[AttributeLeft]

    def __init__(self) -> None:
        self.left_rows = [AttributeLeft("key")]
        self.values = [0]
        self.score = None


def test_compiled_schema_cache_handles_recursive_provider_callbacks() -> None:
    first = _compiled_schema_for_solution(RecursiveCallbackPlan())
    second = _compiled_schema_for_solution(RecursiveCallbackPlan())

    assert first is not second
    assert Solver.analyze(RecursiveCallbackPlan()) == {
        "family": "hard_soft",
        "levels": [0, 0],
    }


function_attribute_threshold = -1


@constraint_provider
def function_attribute_constraints(factory: ConstraintFactory):
    def predicate(item: AttributeLeft) -> bool:
        return int(item.value) > int(getattr(predicate, "threshold"))

    setattr(predicate, "threshold", function_attribute_threshold)
    return [
        factory.for_each(AttributeLeft)
        .filter(predicate)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("function attribute")
    ]


@planning_solution(score=HardSoftScore, constraints=function_attribute_constraints)
class FunctionAttributePlan(RecursiveCallbackPlan):
    pass


def test_compiled_schema_cache_does_not_reuse_function_attributes() -> None:
    global function_attribute_threshold
    function_attribute_threshold = -1
    assert Solver.analyze(FunctionAttributePlan()) == {
        "family": "hard_soft",
        "levels": [0, -1],
    }
    function_attribute_threshold = 1
    assert Solver.analyze(FunctionAttributePlan()) == {
        "family": "hard_soft",
        "levels": [0, 0],
    }


def partial_filter(limit: int, item: AttributeLeft) -> bool:
    return int(item.value) > limit


@constraint_provider
def partial_callback_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AttributeLeft)
        .filter(partial(partial_filter, 0))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("partial callback")
    ]


@planning_solution(score=HardSoftScore, constraints=partial_callback_constraints)
class PartialCallbackPlan(RecursiveCallbackPlan):
    pass


def test_compiled_schema_cache_does_not_retain_partial_callbacks() -> None:
    cached_before = len(_COMPILED_SCHEMA_CACHE.get(PartialCallbackPlan, {}))
    first = _compiled_schema_for_solution(PartialCallbackPlan())
    second = _compiled_schema_for_solution(PartialCallbackPlan())

    assert first is not second
    assert len(_COMPILED_SCHEMA_CACHE.get(PartialCallbackPlan, {})) == cached_before


mutable_capture_key: list[object] = []


@constraint_provider
def mutable_capture_constraints(factory: ConstraintFactory):
    captured = mutable_capture_key
    return [
        factory.for_each(AttributeLeft)
        .filter(lambda item: item.key is captured)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("mutable capture")
    ]


@planning_solution(score=HardSoftScore, constraints=mutable_capture_constraints)
class MutableCapturePlan:
    left_rows: list[AttributeLeft]

    def __init__(self) -> None:
        self.left_rows = [AttributeLeft(mutable_capture_key)]
        self.values = [0]
        self.score = None


def test_compiled_schema_cache_does_not_reuse_distinct_mutable_captures() -> None:
    global mutable_capture_key
    mutable_capture_key = []
    first = Solver.analyze(MutableCapturePlan())
    mutable_capture_key = []
    second = Solver.analyze(MutableCapturePlan())

    assert first == second == {"family": "hard_soft", "levels": [0, -1]}


identity_capture_key: tuple[int, int] = (10**30, 10**30 + 1)


@constraint_provider
def identity_capture_constraints(factory: ConstraintFactory):
    captured = identity_capture_key
    return [
        factory.for_each(AttributeLeft)
        .filter(lambda item: item.key is captured)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("identity capture")
    ]


@planning_solution(score=HardSoftScore, constraints=identity_capture_constraints)
class IdentityCapturePlan(MutableCapturePlan):
    def __init__(self) -> None:
        self.left_rows = [AttributeLeft(identity_capture_key)]
        self.values = [0]
        self.score = None


def test_compiled_schema_cache_does_not_reuse_equal_identity_captures() -> None:
    global identity_capture_key
    identity_capture_key = tuple([10**30, 10**30 + 1])
    first = Solver.analyze(IdentityCapturePlan())
    identity_capture_key = tuple([10**30, 10**30 + 1])
    second = Solver.analyze(IdentityCapturePlan())

    assert first == second == {"family": "hard_soft", "levels": [0, -1]}


signed_zero_capture = 0.0


@constraint_provider
def signed_zero_capture_constraints(factory: ConstraintFactory):
    captured = signed_zero_capture
    return [
        factory.for_each(AttributeLeft)
        .filter(lambda _item: copysign(1.0, captured) > 0.0)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("signed zero capture")
    ]


@planning_solution(score=HardSoftScore, constraints=signed_zero_capture_constraints)
class SignedZeroCapturePlan(MutableCapturePlan):
    pass


def test_compiled_schema_cache_does_not_conflate_signed_zero_captures() -> None:
    global signed_zero_capture
    signed_zero_capture = 0.0
    assert Solver.analyze(SignedZeroCapturePlan()) == {
        "family": "hard_soft",
        "levels": [0, -1],
    }
    signed_zero_capture = -0.0
    assert Solver.analyze(SignedZeroCapturePlan()) == {
        "family": "hard_soft",
        "levels": [0, 0],
    }


def test_native_scalar_join_falls_back_before_narrowing_large_values() -> None:
    native_plan = AttributeJoinPlan(["unused"], [-1])
    callback_plan = CallbackAttributeJoinPlan(["unused"], [-1])
    for plan in (native_plan, callback_plan):
        plan.left_rows[0].value = 2**64 - 1
        plan.values = [2**64 - 1]

    assert (
        Solver.analyze(native_plan)
        == Solver.analyze(callback_plan)
        == {
            "family": "hard_soft",
            "levels": [0, 0],
        }
    )


@constraint_provider
def computed_attribute_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AttributeLeft)
        .join(AttributeRight, joiner.equal_bi("value", "key"))
        .reward(HardSoftScore.of_soft(10))
        .named("planning value match"),
        factory.for_each(AttributeLeft)
        .join(AttributeRight, joiner.equal_bi("derived_value", "key"))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("computed value match"),
    ]


@planning_solution(score=HardSoftScore, constraints=computed_attribute_join_constraints)
class ComputedAttributeJoinPlan:
    left_rows: list[AttributeLeft]
    right_rows: list[AttributeRight]

    def __init__(self) -> None:
        self.left_rows = [AttributeLeft(0)]
        self.right_rows = [AttributeRight(1)]
        self.values = [0, 1]
        self.score = None


@planning_entity
class DescriptorBackedLeft:
    value = planning_variable(value_range_provider="values")

    def __init__(self) -> None:
        self.key = 1
        self.value = 0

    @property
    def key(self) -> int:
        return int(self.__dict__["key"]) + 1

    @key.setter
    def key(self, value: int) -> None:
        self.__dict__["key"] = value


@planning_entity
class DescriptorBackedRight:
    value = planning_variable(value_range_provider="values")

    def __init__(self) -> None:
        self.key = 1
        self.value = 0


@constraint_provider
def descriptor_string_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(DescriptorBackedLeft)
        .join(DescriptorBackedRight, joiner.equal_bi("key", "key"))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("descriptor string join")
    ]


@constraint_provider
def descriptor_callback_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(DescriptorBackedLeft)
        .join(
            DescriptorBackedRight,
            joiner.equal_bi(lambda row: row.key, lambda row: row.key),
        )
        .penalize(HardSoftScore.ONE_SOFT)
        .named("descriptor callback join")
    ]


@planning_solution(score=HardSoftScore, constraints=descriptor_string_join_constraints)
class DescriptorStringJoinPlan:
    left_rows: list[DescriptorBackedLeft]
    right_rows: list[DescriptorBackedRight]

    def __init__(self) -> None:
        self.left_rows = [DescriptorBackedLeft()]
        self.right_rows = [DescriptorBackedRight()]
        self.values = [0]
        self.score = None


@planning_solution(
    score=HardSoftScore, constraints=descriptor_callback_join_constraints
)
class DescriptorCallbackJoinPlan(DescriptorStringJoinPlan):
    pass


def test_string_key_join_preserves_data_descriptor_attribute_semantics() -> None:
    string_score = Solver.analyze(DescriptorStringJoinPlan())
    callback_score = Solver.analyze(DescriptorCallbackJoinPlan())

    assert (
        string_score
        == callback_score
        == {
            "family": "hard_soft",
            "levels": [0, 0],
        }
    )


@planning_entity
class OverriddenLookupLeft:
    value = planning_variable(value_range_provider="values")

    def __init__(self) -> None:
        self.key = 1
        self.value = 0

    def __getattribute__(self, name: str) -> object:
        value = object.__getattribute__(self, name)
        if name == "key":
            return int(value) + 1
        return value


@planning_entity
class OverriddenLookupRight:
    value = planning_variable(value_range_provider="values")

    def __init__(self) -> None:
        self.key = 1
        self.value = 0


@constraint_provider
def overridden_lookup_string_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(OverriddenLookupLeft)
        .join(OverriddenLookupRight, joiner.equal_bi("key", "key"))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("overridden lookup string join")
    ]


@constraint_provider
def overridden_lookup_callback_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(OverriddenLookupLeft)
        .join(
            OverriddenLookupRight,
            joiner.equal_bi(lambda left: left.key, lambda right: right.key),
        )
        .penalize(HardSoftScore.ONE_SOFT)
        .named("overridden lookup callback join")
    ]


@planning_solution(
    score=HardSoftScore, constraints=overridden_lookup_string_constraints
)
class OverriddenLookupStringPlan:
    left_rows: list[OverriddenLookupLeft]
    right_rows: list[OverriddenLookupRight]

    def __init__(self) -> None:
        self.left_rows = [OverriddenLookupLeft()]
        self.right_rows = [OverriddenLookupRight()]
        self.values = [0]
        self.score = None


@planning_solution(
    score=HardSoftScore, constraints=overridden_lookup_callback_constraints
)
class OverriddenLookupCallbackPlan(OverriddenLookupStringPlan):
    pass


def test_string_key_join_preserves_overridden_attribute_lookup() -> None:
    string_score = Solver.analyze(OverriddenLookupStringPlan())
    callback_score = Solver.analyze(OverriddenLookupCallbackPlan())

    assert (
        string_score
        == callback_score
        == {
            "family": "hard_soft",
            "levels": [0, 0],
        }
    )


@planning_entity
class ListOwnerItem:
    values = planning_list_variable(element_collection="value_ids")

    def __init__(self) -> None:
        self.values = []


def precedence_owner(solution: object, element: int) -> int:
    return int(solution.owner_by_value[int(element)])


def precedence_duration(solution: object, element: int) -> int:
    return int(solution.duration_by_value[int(element)])


def precedence_successors(solution: object, element: int) -> list[int]:
    return list(solution.successors_by_value[int(element)])


@planning_entity
class PrecedenceRoute:
    values = planning_list_variable(
        element_collection="precedence_value_ids",
        element_owner=precedence_owner,
        precedence_duration=precedence_duration,
        precedence_successors=precedence_successors,
    )

    def __init__(self) -> None:
        self.values = []


def test_constraint_plan_uses_callback_surface_only() -> None:
    constraint = (
        ConstraintFactory()
        .for_each(Item)
        .filter(lambda item: True)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("always")
    )
    native = constraint.to_native()
    assert native["entity_type"] == "Item"
    assert native["impact"] == "penalty"
    assert callable(native["filters"][0])


def test_binary_constraint_plan_uses_callback_join_surface() -> None:
    constraint = (
        ConstraintFactory()
        .for_each(Item)
        .join(Item, lambda left, right: left is not right)
        .filter(lambda left, right: True)
        .penalize(HardSoftScore.ONE_HARD)
        .named("pair")
    )
    native = constraint.to_native()
    assert native["arity"] == 2
    assert native["right_entity_type"] == "Item"
    assert callable(native["joiners"][0])
    assert callable(native["filters"][0])


def test_binary_constraint_plan_accepts_string_key_joiner() -> None:
    constraint = (
        ConstraintFactory()
        .for_each(Item)
        .join(Item, joiner.equal_bi("left_key", "right_key"))
        .penalize(HardSoftScore.ONE_SOFT)
        .named("attribute pair")
    )
    native = constraint.to_native()
    assert native["joiners"][0] == {
        "type": "equal_attr",
        "left_attr": "left_key",
        "right_attr": "right_key",
    }


def test_string_key_join_scores_directly() -> None:
    score = Solver.analyze(AttributeJoinPlan())

    assert score == {"family": "hard_soft", "levels": [0, -1]}


def test_string_key_join_scores_none_int_and_string_keys() -> None:
    score = Solver.analyze(AttributeJoinPlan([None, 1, "x", "missing"], [None, 1, "x"]))

    assert score == {"family": "hard_soft", "levels": [0, -3]}


def test_string_key_join_uses_python_for_unsupported_key_values() -> None:
    native = Solver.analyze(AttributeJoinPlan([1.5, 2.5], [1.5]))
    callback = Solver.analyze(CallbackAttributeJoinPlan([1.5, 2.5], [1.5]))

    assert native == callback == {"family": "hard_soft", "levels": [0, -1]}


def test_string_key_join_preserves_list_and_tuple_equality_semantics() -> None:
    native = Solver.analyze(AttributeJoinPlan([[1]], [(1,)]))
    callback = Solver.analyze(CallbackAttributeJoinPlan([[1]], [(1,)]))

    assert native == callback == {"family": "hard_soft", "levels": [0, 0]}


def test_string_key_join_preserves_custom_object_equality_semantics() -> None:
    native = Solver.analyze(AttributeJoinPlan([IdentityKey()], [IdentityKey()]))
    callback = Solver.analyze(
        CallbackAttributeJoinPlan([IdentityKey()], [IdentityKey()])
    )

    assert native == callback == {"family": "hard_soft", "levels": [0, 0]}


def test_computed_string_key_join_keeps_solve_and_analyze_scores_equal() -> None:
    plan = Solver.solve(
        ComputedAttributeJoinPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "change_move_selector",
                        "selection_order": "original",
                        "entity_class": "AttributeLeft",
                        "variable_name": "value",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert plan.left_rows[0].value == 1
    assert plan.score == Solver.analyze(plan)
    assert plan.score == {"family": "hard_soft", "levels": [0, 9]}


def test_grouped_constraint_plan_uses_callback_group_key_and_weight() -> None:
    def group_filter(key: object, count: int) -> bool:
        return count > 1

    constraint = (
        ConstraintFactory()
        .for_each(Item)
        .group_by(lambda item: item.key)
        .filter(group_filter)
        .penalize(lambda key, count: HardSoftScore.of_soft(abs(count - 1)))
        .named("load")
    )
    native = constraint.to_native()
    assert native["arity"] == 1
    assert callable(native["group_key"])
    assert native["filters"] == []
    assert native["group_filters"] == [group_filter]
    assert callable(native["weight_callback"])


def test_indexed_presence_group_collector_plan_uses_callback_index() -> None:
    constraint = (
        ConstraintFactory()
        .for_each(Item)
        .group_by(lambda item: item.key, indexed_presence(lambda item: item.day))
        .penalize(lambda key, presence: HardSoftScore.of_soft(presence.count()))
        .named("presence")
    )

    native = constraint.to_native()

    assert callable(native["group_key"])
    assert native["group_collector"]["type"] == "indexed_presence"
    assert callable(native["group_collector"]["index"])
    assert callable(native["weight_callback"])


def test_callback_weight_placeholder_uses_factory_score_family() -> None:
    constraint = (
        ConstraintFactory(score_family="hard_soft")
        .for_each(Item)
        .penalize(lambda item: HardSoftScore.ONE_HARD)
        .named("hard callback")
    )

    native = constraint.to_native()

    assert native["weight"] == {"family": "hard_soft", "levels": [1, 0]}
    assert callable(native["weight_callback"])


def test_sequence_weight_uses_factory_score_family() -> None:
    constraint = (
        ConstraintFactory(score_family="hard_soft")
        .for_each(Item)
        .penalize([1, 0])
        .named("hard sequence")
    )

    native = constraint.to_native()

    assert native["weight"] == {"family": "hard_soft", "levels": [1, 0]}


def test_unassigned_list_element_plan_uses_owner_list_variable_metadata() -> None:
    constraint = (
        ConstraintFactory(score_family="hard_soft")
        .for_each_unassigned_element(ListOwnerItem, "values")
        .filter(lambda value: value > 0)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned values")
    )

    native = constraint.to_native()

    assert native["constraint_type"] == "list_unassigned_element"
    assert native["entity_type"] == "ListOwnerItem"
    assert native["variable_name"] == "values"
    assert native["element_collection"] == "value_ids"
    assert callable(native["filters"][0])
    assert native["weight"] == {"family": "hard_soft", "levels": [1, 0]}


def test_list_precedence_makespan_plan_uses_owner_list_variable_metadata() -> None:
    constraint = (
        ConstraintFactory(score_family="hard_soft")
        .list_precedence_makespan(PrecedenceRoute, "values")
        .named("precedence makespan")
    )

    native = constraint.to_native()

    assert native["constraint_type"] == "list_precedence_makespan"
    assert native["entity_type"] == "PrecedenceRoute"
    assert native["variable_name"] == "values"
    assert native["element_collection"] == "precedence_value_ids"
    assert callable(native["element_owner"])
    assert callable(native["precedence_duration"])
    assert callable(native["precedence_successors"])
    assert native["weight"] == {"family": "hard_soft", "levels": [0, 0]}


@pytest.mark.parametrize(
    "method_name", ["join", "if_exists", "if_not_exists", "group_by", "flattened"]
)
def test_advanced_stream_methods_are_explicitly_unsupported(method_name: str) -> None:
    method = getattr(ConstraintFactory(), method_name)
    with pytest.raises(NotImplementedError):
        method(Item)


class IdentityReprKey:
    def __repr__(self) -> str:
        return "same"


@planning_entity
class GroupItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self, key: str) -> None:
        self.key = key
        self.value = 0


@constraint_provider
def grouped_key_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(GroupItem)
        .group_by(lambda item: IdentityReprKey())
        .penalize(lambda key, count: HardSoftScore.of_soft(abs(count - 1)))
        .named("grouped key")
    ]


@planning_solution(score=HardSoftScore, constraints=grouped_key_constraints)
class GroupedKeyPlan:
    items: list[GroupItem]

    def __init__(self) -> None:
        self.items = [GroupItem("left"), GroupItem("right")]
        self.values = [0]
        self.score = None


def test_group_by_preserves_python_key_equality_not_repr() -> None:
    plan = GroupedKeyPlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, 0]}
    assert plan.score == score


@planning_entity
class PresenceShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, nurse: int | None, day: int) -> None:
        self.nurse = nurse
        self.day = day


@constraint_provider
def indexed_presence_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(PresenceShift)
        .filter(lambda shift: shift.nurse is not None)
        .group_by(
            lambda shift: shift.nurse,
            indexed_presence(lambda shift: shift.day),
        )
        .penalize(
            lambda _nurse, presence: HardSoftScore.of_soft(
                sum(max(0, run.point_count() - 2) for run in presence.runs().runs())
                + presence.complement_runs(0, 7).len()
                + (1 if presence.any_in(5, 7) else 0)
            )
        )
        .named("indexed presence")
    ]


@planning_solution(score=HardSoftScore, constraints=indexed_presence_constraints)
class IndexedPresencePlan:
    shifts: list[PresenceShift]

    def __init__(self) -> None:
        self.shifts = [
            PresenceShift(1, 0),
            PresenceShift(1, 1),
            PresenceShift(1, 2),
            PresenceShift(1, 5),
            PresenceShift(None, 6),
        ]
        self.nurses = [0, 1]
        self.score = None


def test_indexed_presence_group_collector_scores_runs_and_ranges() -> None:
    plan = IndexedPresencePlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, -4]}
    assert plan.score == score


@planning_entity
class SequenceScoreItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0


@constraint_provider
def fixed_sequence_score_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(SequenceScoreItem)
        .penalize([1, 0])
        .named("fixed sequence score")
    ]


@planning_solution(score=HardSoftScore, constraints=fixed_sequence_score_constraints)
class FixedSequenceScorePlan:
    items: list[SequenceScoreItem]

    def __init__(self) -> None:
        self.items = [SequenceScoreItem()]
        self.values = [0]
        self.score = None


def test_fixed_sequence_score_weight_scores_with_solution_family() -> None:
    plan = FixedSequenceScorePlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [-1, 0]}


@constraint_provider
def callback_sequence_score_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(SequenceScoreItem)
        .penalize(lambda item: [1, 0])
        .named("callback sequence score")
    ]


@planning_solution(score=HardSoftScore, constraints=callback_sequence_score_constraints)
class CallbackSequenceScorePlan:
    items: list[SequenceScoreItem]

    def __init__(self) -> None:
        self.items = [SequenceScoreItem()]
        self.values = [0]
        self.score = None


def test_callback_sequence_score_weight_scores_with_solution_family() -> None:
    plan = CallbackSequenceScorePlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [-1, 0]}


shadow_refresh_calls: list[tuple[str, int]] = []


def first_shadow_listener(solution: object, entity_index: int) -> dict[str, int]:
    del solution
    shadow_refresh_calls.append(("first", entity_index))
    return {}


def second_shadow_listener(solution: object, entity_index: int) -> dict[str, int]:
    del solution
    shadow_refresh_calls.append(("second", entity_index))
    return {}


@planning_entity
class MultiShadowItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0


@planning_solution(
    score=HardSoftScore,
    shadow_updates=[
        shadow_variable_updates(
            list_owner="items", post_update_listener=first_shadow_listener
        ),
        shadow_variable_updates(
            list_owner="items", post_update_listener=second_shadow_listener
        ),
    ],
)
class MultiShadowPlan:
    items: list[MultiShadowItem]

    def __init__(self) -> None:
        self.items = [MultiShadowItem(), MultiShadowItem()]
        self.values = [0]
        self.score = None


def test_refresh_all_shadows_runs_each_listener_once_per_entity() -> None:
    shadow_refresh_calls.clear()

    Solver.analyze(MultiShadowPlan())

    assert shadow_refresh_calls == [
        ("first", 0),
        ("second", 0),
        ("first", 1),
        ("second", 1),
    ]


dirty_sync_value_writes = 0
dirty_sync_second_seen: list[int] = []


def dirty_sync_first_listener(solution: object, entity_index: int) -> dict[str, int]:
    return {"first_shadow": entity_index + 10}


def dirty_sync_second_listener(solution: object, entity_index: int) -> dict[str, int]:
    value = int(solution.dirty_sync_items[entity_index].first_shadow)
    dirty_sync_second_seen.append(value)
    return {"second_shadow": value + 1}


@planning_entity
class DirtySyncItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0
        self.first_shadow = 0
        self.second_shadow = 0

    def __setattr__(self, name: str, value: object) -> None:
        global dirty_sync_value_writes
        if name == "__solverforge_value":
            dirty_sync_value_writes += 1
        super().__setattr__(name, value)


@planning_solution(
    score=HardSoftScore,
    shadow_updates=[
        shadow_variable_updates(
            list_owner="dirty_sync_items",
            post_update_listener=dirty_sync_first_listener,
        ),
        shadow_variable_updates(
            list_owner="dirty_sync_items",
            post_update_listener=dirty_sync_second_listener,
        ),
    ],
)
class DirtySyncPlan:
    dirty_sync_items: list[DirtySyncItem]

    def __init__(self) -> None:
        self.dirty_sync_items = [DirtySyncItem() for _ in range(4)]
        self.values = [0]
        self.score = None


def test_shadow_callbacks_see_prior_listener_updates_without_full_resync() -> None:
    global dirty_sync_value_writes
    dirty_sync_second_seen.clear()
    plan = DirtySyncPlan()
    dirty_sync_value_writes = 0

    Solver.analyze(plan)

    assert dirty_sync_second_seen == [10, 11, 12, 13]
    assert [item.first_shadow for item in plan.dirty_sync_items] == [10, 11, 12, 13]
    assert [item.second_shadow for item in plan.dirty_sync_items] == [11, 12, 13, 14]
    assert dirty_sync_value_writes <= (4 * len(plan.dirty_sync_items)) + 1


@planning_entity
class BalanceCallbackItem:
    worker = planning_variable(value_range_provider="workers", allows_unassigned=True)

    def __init__(self, worker: int) -> None:
        self.worker = worker


@constraint_provider
def balance_callback_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(BalanceCallbackItem)
        .balance(lambda item: item.worker)
        .penalize(lambda: SoftScore.of(10))
        .named("balance callback")
    ]


@planning_solution(score=SoftScore, constraints=balance_callback_constraints)
class BalanceCallbackPlan:
    items: list[BalanceCallbackItem]

    def __init__(self) -> None:
        self.items = [
            BalanceCallbackItem(0),
            BalanceCallbackItem(0),
            BalanceCallbackItem(1),
        ]
        self.workers = [0, 1]
        self.score = None


def test_balance_constraint_applies_callback_weight() -> None:
    plan = BalanceCallbackPlan()

    score = Solver.analyze(plan)

    assert score == {"family": "soft", "levels": [-5]}


@constraint_provider
def grouped_filter_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(GroupItem)
        .group_by(lambda item: item.key)
        .filter(lambda key, count: count > 1)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("large group")
    ]


@planning_solution(score=HardSoftScore, constraints=grouped_filter_constraints)
class GroupedFilterPlan:
    items: list[GroupItem]

    def __init__(self) -> None:
        self.items = [GroupItem("shared"), GroupItem("shared"), GroupItem("single")]
        self.values = [0]
        self.score = None


def test_group_by_filters_apply_to_group_key_and_count() -> None:
    plan = GroupedFilterPlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, -1]}
    assert plan.score == score


class EqualDifferentReprKey:
    def __init__(self, value: str, label: str) -> None:
        self.value = value
        self.label = label

    def __eq__(self, other: object) -> bool:
        return isinstance(other, EqualDifferentReprKey) and self.value == other.value

    def __repr__(self) -> str:
        return f"{self.value}:{self.label}"


@problem_fact
class JoinFact:
    def __init__(self, key: object) -> None:
        self.key = key


@planning_entity
class JoinItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self, key: object) -> None:
        self.key = key
        self.value = 0


@constraint_provider
def equal_join_key_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(JoinItem)
        .join(
            JoinFact,
            joiner.equal_bi(
                lambda item: EqualDifferentReprKey(item.key, "left"),
                lambda fact: EqualDifferentReprKey(fact.key, "right"),
            ),
        )
        .penalize(HardSoftScore.ONE_SOFT)
        .named("equal key")
    ]


@planning_solution(score=HardSoftScore, constraints=equal_join_key_constraints)
class EqualJoinKeyPlan:
    items: list[JoinItem]
    facts: list[JoinFact]

    def __init__(self) -> None:
        self.items = [JoinItem("same")]
        self.facts = [JoinFact("same")]
        self.values = [0]
        self.score = None


def test_equal_join_preserves_python_key_equality_not_repr() -> None:
    plan = EqualJoinKeyPlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, -1]}
    assert plan.score == score


@planning_entity
class ReadOnlyMetricItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0

    @property
    def load(self) -> int:
        return 5


@constraint_provider
def read_only_property_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ReadOnlyMetricItem)
        .filter(lambda item: item.load == 5)
        .penalize(HardSoftScore.ONE_HARD)
        .named("read only metric")
    ]


@planning_solution(score=HardSoftScore, constraints=read_only_property_constraints)
class ReadOnlyMetricPlan:
    items: list[ReadOnlyMetricItem]

    def __init__(self) -> None:
        self.items = [ReadOnlyMetricItem()]
        self.values = [0]
        self.score = None


def test_read_only_property_values_are_available_to_constraint_callbacks() -> None:
    plan = ReadOnlyMetricPlan()

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [-1, 0]}


@planning_entity
class MixedRuntimeItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0
        self.computed_total = 0


class SpecializedRuntimeItem(MixedRuntimeItem):
    @property
    def specialized_metric(self) -> int:
        return 11


@problem_fact
class MixedRuntimeFact:
    pass


class SpecializedRuntimeFact(MixedRuntimeFact):
    @property
    def specialized_metric(self) -> int:
        return 7


def mixed_runtime_listener(solution: object, entity_index: int) -> dict[str, int]:
    del entity_index
    return {
        "computed_total": (
            int(solution.mixed_runtime_items[1].specialized_metric)
            + int(solution.mixed_runtime_facts[1].specialized_metric)
        )
    }


@planning_solution(
    score=HardSoftScore,
    shadow_updates=shadow_variable_updates(
        list_owner="mixed_runtime_items",
        post_update_listener=mixed_runtime_listener,
    ),
)
class MixedRuntimePlan:
    mixed_runtime_items: list[MixedRuntimeItem]
    mixed_runtime_facts: list[MixedRuntimeFact]

    def __init__(self) -> None:
        self.mixed_runtime_items = [MixedRuntimeItem(), SpecializedRuntimeItem()]
        self.mixed_runtime_facts = [MixedRuntimeFact(), SpecializedRuntimeFact()]
        self.values = [0, 1]
        self.score = None


def test_detached_callback_views_import_computed_fields_from_every_runtime_row() -> (
    None
):
    plan = Solver.solve(
        MixedRuntimePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "move_selector": {
                        "type": "change_move_selector",
                        "entity_class": "MixedRuntimeItem",
                        "variable_name": "value",
                    },
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [item.computed_total for item in plan.mixed_runtime_items] == [18, 18]


def shadow_export_listener(solution: object, entity_index: int) -> dict[str, int]:
    return {"route_total": int(solution.shadow_export_items[entity_index].value) + 5}


@planning_entity
class ShadowExportItem:
    value = planning_variable(value_range_provider="values", allows_unassigned=True)

    def __init__(self) -> None:
        self.value = 0
        self.route_total = 0.0
        self.transient_cache = object()


@constraint_provider
def shadow_export_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ShadowExportItem)
        .filter(lambda item: item.route_total != 5)
        .penalize(HardSoftScore.ONE_HARD)
        .named("shadow export")
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=shadow_export_constraints,
    shadow_updates=shadow_variable_updates(
        list_owner="shadow_export_items",
        post_update_listener=shadow_export_listener,
    ),
)
class ShadowExportPlan:
    shadow_export_items: list[ShadowExportItem]

    def __init__(self) -> None:
        self.shadow_export_items = [ShadowExportItem()]
        self.values = [0]
        self.score = None


def test_analyze_exports_only_native_owned_shadow_fields() -> None:
    plan = ShadowExportPlan()
    original_cache = plan.shadow_export_items[0].transient_cache

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, 0]}
    assert plan.score == score
    assert plan.shadow_export_items[0].route_total == 5
    assert type(plan.shadow_export_items[0].route_total) is int
    assert plan.shadow_export_items[0].transient_cache is original_cache
