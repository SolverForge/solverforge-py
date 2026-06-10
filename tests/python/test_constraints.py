import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    SoftScore,
    Solver,
    constraint_provider,
    joiner,
    planning_entity,
    planning_list_variable,
    planning_solution,
    planning_variable,
    problem_fact,
    shadow_variable_updates,
)


class Item:
    pass


@planning_entity
class ListOwnerItem:
    values = planning_list_variable(element_collection="value_ids")

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
