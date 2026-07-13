import time
from pathlib import Path

from examples.solverforge_deliveries import (
    DELIVERIES_SOLVER_CONFIG,
    DeliveryPlan,
    demo_plan,
    fixture_payload,
    plan_from_payload,
    solve_demo,
)
from examples.solverforge_deliveries.src.api.dto import (
    constraint_analysis,
    plan_to_payload,
)
from examples.solverforge_deliveries.src.domain import Delivery, Vehicle
from examples.solverforge_deliveries.src.domain.metrics import (
    CAPACITY_HARD_WEIGHT,
    build_preview,
    rank_delivery_insertions,
    score_components,
)
from solverforge import Solver, SolverManager
from solverforge.model import build_schema

DELIVERIES_EXAMPLE_ROOT = (
    Path(__file__).parents[2] / "examples" / "solverforge_deliveries"
)


def assert_complete_routes(plan: DeliveryPlan) -> None:
    assigned = [
        delivery for vehicle in plan.vehicles for delivery in vehicle.delivery_order
    ]
    assert len(assigned) == len(plan.deliveries)
    assert sorted(assigned) == list(range(len(plan.deliveries)))
    for vehicle in plan.vehicles:
        assert vehicle.route_capacity_overage == 0
        assert vehicle.route_total_demand == sum(
            plan.deliveries[delivery_id].demand
            for delivery_id in vehicle.delivery_order
        )


def test_solverforge_deliveries_python_model_solves_default_routes() -> None:
    plan = solve_demo()

    assert_complete_routes(plan)
    assert plan.score["family"] == "hard_soft"
    assert plan.score["levels"][0] == 0


def test_solverforge_deliveries_schema_exposes_cvrp_and_shadow_hooks() -> None:
    schema = build_schema(demo_plan("HARTFORD"))
    vehicle = schema["entities"][0]
    delivery_order = next(
        field for field in vehicle["fields"] if field["name"] == "delivery_order"
    )

    assert vehicle["type_name"] == "Vehicle"
    assert delivery_order["kind"] == "planning_list_variable"
    assert delivery_order["element_collection"] == "delivery_indices"
    metadata = delivery_order["list_metadata"]
    assert metadata["route"]["depot"] == {"kind": "row", "field": "depot"}
    assert metadata["savings"]["metric_class"] == {
        "kind": "row",
        "field": "metric_class",
    }
    assert metadata["route"]["distance"] == {
        "kind": "row",
        "field": "distance_matrix",
    }
    assert metadata["route"]["feasible"]["kind"] == "solution"
    assert callable(metadata["route"]["feasible"]["callback"])
    assert schema["shadow_updates"][0]["list_owner"] == "vehicles"
    assert callable(schema["shadow_updates"][0]["post_update_listener"])


def test_solverforge_deliveries_config_uses_list_initializer_and_cvrp_polish() -> None:
    config = DELIVERIES_SOLVER_CONFIG.to_dict()

    assert config["random_seed"] == 42
    assert [phase["construction_heuristic_type"] for phase in config["phases"][:2]] == [
        "list_cheapest_insertion",
        "list_k_opt",
    ]
    assert config["phases"][0]["entity_class"] == "Vehicle"
    assert config["phases"][0]["variable_name"] == "delivery_order"
    assert config["phases"][1]["k"] == 2


def test_solverforge_deliveries_payload_round_trips_unassigned_seed() -> None:
    payload = fixture_payload("HARTFORD")
    plan = plan_from_payload(payload)

    assert payload["score"] is None
    assert len(payload["deliveries"]) == 50
    assert len(payload["vehicles"]) == 10
    assert payload["viewState"]["preview"]["unassignedDeliveryIds"] == list(range(50))
    assert [vehicle.delivery_order for vehicle in plan.vehicles] == [
        [] for _ in range(10)
    ]
    assert plan_to_payload(plan)["vehicles"] == payload["vehicles"]


def test_solverforge_deliveries_analyze_penalizes_unassigned_deliveries() -> None:
    plan = demo_plan("HARTFORD")

    score = Solver.analyze(plan)
    expected_hard, expected_soft = score_components(plan)

    assert expected_hard == -(len(plan.deliveries) * 1_000_000)
    assert score == {"family": "hard_soft", "levels": [expected_hard, expected_soft]}


def test_solverforge_deliveries_refreshes_route_matrix_after_coordinate_edits() -> None:
    plan = DeliveryPlan(
        name="coordinate-edit",
        deliveries=[
            Delivery(
                id=0,
                label="A",
                kind="retail",
                lat=0.0,
                lng=0.0,
                demand=1,
                min_start_time=0,
                max_end_time=1_000_000,
                service_duration=0,
            )
        ],
        vehicles=[
            Vehicle(
                id=0,
                name="Van 1",
                capacity=10,
                home_lat=0.0,
                home_lng=0.0,
                departure_time=0,
                delivery_order=[0],
            )
        ],
    )
    original_matrix = plan.vehicles[0].distance_matrix

    plan.deliveries[0].lat = 10.0
    moved_delivery_score = score_components(plan)
    moved_delivery_matrix = plan.vehicles[0].distance_matrix

    assert moved_delivery_matrix is not original_matrix
    assert moved_delivery_score[1] == build_preview(plan)["softScore"] < 0
    assert Solver.analyze(plan)["levels"][-1] == moved_delivery_score[1]

    plan.vehicles[0].home_lat = 10.0

    assert plan.vehicles[0].distance_matrix is not moved_delivery_matrix
    assert score_components(plan)[1] == build_preview(plan)["softScore"] == 0


def test_solverforge_deliveries_solve_assigns_initially_unassigned_delivery() -> None:
    plan = DeliveryPlan(
        name="unassigned-start",
        deliveries=[
            Delivery(
                id=0,
                label="A",
                kind="retail",
                lat=41.8,
                lng=-72.7,
                demand=1,
                min_start_time=0,
                max_end_time=100_000,
                service_duration=60,
            )
        ],
        vehicles=[
            Vehicle(
                id=0,
                name="Van 1",
                capacity=10,
                home_lat=41.8,
                home_lng=-72.7,
                departure_time=0,
                delivery_order=[],
            )
        ],
    )

    solved = Solver.solve(plan, DELIVERIES_SOLVER_CONFIG)

    assert solved.vehicles[0].delivery_order == [0]
    assert solved.score == {"family": "hard_soft", "levels": [0, 0]}


def test_solverforge_deliveries_manager_scores_cross_vehicle_list_updates() -> None:
    def make_plan() -> DeliveryPlan:
        return DeliveryPlan(
            name="cross-vehicle-unassigned",
            deliveries=[
                Delivery(
                    id=0,
                    label="A",
                    kind="retail",
                    lat=42.0,
                    lng=-73.0,
                    demand=1,
                    min_start_time=0,
                    max_end_time=100_000_000,
                    service_duration=0,
                )
            ],
            vehicles=[
                Vehicle(
                    id=0,
                    name="Far",
                    capacity=10,
                    home_lat=0.0,
                    home_lng=0.0,
                    departure_time=0,
                    delivery_order=[],
                ),
                Vehicle(
                    id=1,
                    name="Near",
                    capacity=10,
                    home_lat=42.0,
                    home_lng=-73.0,
                    departure_time=0,
                    delivery_order=[],
                ),
            ],
        )

    solved = Solver.solve(make_plan(), DELIVERIES_SOLVER_CONFIG)

    assert [vehicle.delivery_order for vehicle in solved.vehicles] == [[], [0]]
    assert solved.score == {"family": "hard_soft", "levels": [0, 0]}

    manager = SolverManager(DELIVERIES_SOLVER_CONFIG)
    handle = manager.solve(make_plan())
    deadline = time.monotonic() + 5
    while True:
        status = manager.get_status(handle.job_id)
        if status["lifecycle_state"] in {"COMPLETED", "FAILED", "CANCELLED"}:
            break
        assert time.monotonic() < deadline
        time.sleep(0.01)

    snapshot = manager.snapshot(handle.job_id)

    assert status["lifecycle_state"] == "COMPLETED"
    assert status["best_score"] == {"family": "hard_soft", "levels": [0, 0]}
    assert [vehicle.delivery_order for vehicle in snapshot.vehicles] == [[], [0]]
    assert snapshot.score == {"family": "hard_soft", "levels": [0, 0]}


def test_solverforge_deliveries_preview_uses_capacity_hard_weight() -> None:
    plan = DeliveryPlan(
        name="overloaded",
        deliveries=[
            Delivery(
                id=0,
                label="A",
                kind="retail",
                lat=41.8,
                lng=-72.7,
                demand=3,
                min_start_time=0,
                max_end_time=100_000,
                service_duration=60,
            )
        ],
        vehicles=[
            Vehicle(
                id=0,
                name="Van 1",
                capacity=1,
                home_lat=41.8,
                home_lng=-72.7,
                departure_time=0,
                delivery_order=[0],
            )
        ],
    )

    expected_hard = -(2 * CAPACITY_HARD_WEIGHT)

    assert score_components(plan)[0] == expected_hard
    assert build_preview(plan)["hardScore"] == expected_hard


def test_solverforge_deliveries_recommends_same_route_assigned_insertions() -> None:
    plan = DeliveryPlan(
        name="same-route-recommendations",
        deliveries=[
            Delivery(
                id=0,
                label="A",
                kind="retail",
                lat=41.8,
                lng=-72.7,
                demand=1,
                min_start_time=0,
                max_end_time=100_000,
                service_duration=60,
            ),
            Delivery(
                id=1,
                label="B",
                kind="retail",
                lat=41.81,
                lng=-72.71,
                demand=1,
                min_start_time=0,
                max_end_time=100_000,
                service_duration=60,
            ),
        ],
        vehicles=[
            Vehicle(
                id=0,
                name="Van 1",
                capacity=10,
                home_lat=41.8,
                home_lng=-72.7,
                departure_time=0,
                delivery_order=[0, 1],
            )
        ],
    )

    candidates = rank_delivery_insertions(plan, 0, 10)

    assert len(candidates) == 2
    assert {candidate["vehicleId"] for candidate in candidates} == {0}
    assert {candidate["insertIndex"] for candidate in candidates} == {0, 1}
    for candidate in candidates:
        preview_order = candidate["previewPlan"].vehicles[0].delivery_order
        assert preview_order.count(0) == 1
        assert sorted(preview_order) == [0, 1]
        assert preview_order[candidate["insertIndex"]] == 0


def test_solverforge_deliveries_analysis_reports_unassigned_seed_match_count() -> None:
    plan = demo_plan("HARTFORD")

    rows = constraint_analysis(plan)

    assert rows["All Deliveries Assigned"]["matchCount"] == len(plan.deliveries)


def test_solverforge_deliveries_file_tree_matches_example_ownership_shape() -> None:
    expected = {
        "src/api/dto.py",
        "src/api/mod.py",
        "src/api/routes.py",
        "src/api/sse.py",
        "src/constraints/mod.py",
        "src/data/data_seed/entrypoints.py",
        "src/data/data_seed/locations.json",
        "src/data/mod.py",
        "src/domain/delivery.py",
        "src/domain/metrics.py",
        "src/domain/mod.py",
        "src/domain/plan.py",
        "src/domain/vehicle.py",
        "src/lib.py",
        "src/main.py",
        "src/solver/mod.py",
        "src/solver/service/mod.py",
        "src/solver/service/payload.py",
    }

    missing = [
        path
        for path in sorted(expected)
        if not (DELIVERIES_EXAMPLE_ROOT / path).is_file()
    ]

    assert missing == []
