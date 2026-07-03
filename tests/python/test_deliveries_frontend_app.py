from __future__ import annotations

import json
import socket
import threading
import time
from dataclasses import dataclass
from types import SimpleNamespace
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import uvicorn

from examples.solverforge_deliveries import create_app, demo_plan
from examples.solverforge_deliveries.src.api.dto import snapshot_payload
from examples.solverforge_deliveries.src.solver.service.payload import (
    event_payload_from_native,
    status_event_payload,
)


@dataclass
class RunningServer:
    server: uvicorn.Server
    thread: threading.Thread

    def shutdown(self) -> None:
        self.server.should_exit = True
        self.thread.join(timeout=10)
        assert not self.thread.is_alive()


def request_json(base_url: str, path: str, data: dict[str, Any] | None = None) -> Any:
    status, body = request_raw(base_url, path, data)
    assert status == 200
    return json.loads(body)


def request_raw(
    base_url: str,
    path: str,
    data: dict[str, Any] | None = None,
    method: str | None = None,
) -> tuple[int, str]:
    body = None if data is None else json.dumps(data).encode("utf-8")
    request = Request(
        f"{base_url}{path}",
        data=body,
        headers={"content-type": "application/json"},
        method=method or ("GET" if data is None else "POST"),
    )
    try:
        with urlopen(request, timeout=10) as response:
            return response.status, response.read().decode("utf-8")
    except HTTPError as error:
        return error.code, error.read().decode("utf-8")


def request_text(base_url: str, path: str) -> str:
    with urlopen(f"{base_url}{path}", timeout=5) as response:
        return response.read().decode("utf-8")


def request_asset_headers(base_url: str, path: str) -> tuple[str, str]:
    with urlopen(f"{base_url}{path}", timeout=5) as response:
        assert response.status == 200
        response.read()
        return (
            response.headers.get("content-type") or "",
            response.headers.get("cache-control") or "",
        )


def test_snapshot_payload_uses_snapshot_score_as_current_score() -> None:
    plan = demo_plan("HARTFORD")
    plan.score = {"family": "hard_soft", "levels": [0, 42]}

    payload = snapshot_payload(
        SimpleNamespace(id="7"),
        plan,
        {
            "lifecycle_state": "COMPLETED",
            "terminal_reason": "TERMINATED_BY_CONFIG",
            "latest_snapshot_revision": 3,
            "current_score": {"family": "hard_soft", "levels": [-1, 10]},
            "best_score": {"family": "hard_soft", "levels": [0, 42]},
            "telemetry": None,
        },
        None,
    )

    assert payload["currentScore"] == "0hard/42soft"
    assert payload["solution"]["score"] == "0hard/42soft"


def test_event_payload_uses_solution_score_as_current_score() -> None:
    plan = demo_plan("HARTFORD")
    plan.score = {"family": "hard_soft", "levels": [0, 42]}
    native_current_score = {"family": "hard_soft", "levels": [-1, 10]}
    native_best_score = {"family": "hard_soft", "levels": [0, 42]}

    event_payload = event_payload_from_native(
        SimpleNamespace(id="7"),
        "completed",
        {
            "event_sequence": 9,
            "lifecycle_state": "COMPLETED",
            "terminal_reason": "TERMINATED_BY_CONFIG",
            "snapshot_revision": 3,
            "current_score": native_current_score,
            "best_score": native_best_score,
            "telemetry": None,
        },
        solution=plan,
    )
    bootstrap_payload = status_event_payload(
        SimpleNamespace(id="7"),
        "completed",
        {
            "event_sequence": 9,
            "lifecycle_state": "COMPLETED",
            "terminal_reason": "TERMINATED_BY_CONFIG",
            "latest_snapshot_revision": 3,
            "current_score": native_current_score,
            "best_score": native_best_score,
            "telemetry": None,
        },
        solution=plan,
    )

    for payload in (event_payload, bootstrap_payload):
        assert payload["currentScore"] == "0hard/42soft"
        assert payload["bestScore"] == "0hard/42soft"
        assert payload["solution"]["score"] == "0hard/42soft"


def start_test_server() -> tuple[RunningServer, str]:
    host = "127.0.0.1"
    port = free_port()
    config = uvicorn.Config(
        create_app(),
        host=host,
        port=port,
        log_level="warning",
        lifespan="off",
    )
    server = uvicorn.Server(config)
    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()
    base_url = f"http://{host}:{port}"
    wait_for_server(base_url)
    return RunningServer(server, thread), base_url


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_server(base_url: str) -> None:
    deadline = time.monotonic() + 10
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            if request_json(base_url, "/health") == {"status": "UP"}:
                return
        except Exception as error:
            last_error = error
        time.sleep(0.05)
    msg = f"server did not become ready: {last_error}"
    raise AssertionError(msg)


def wait_for_terminal(
    base_url: str, job_id: str, timeout_seconds: float = 15.0
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        status = request_json(base_url, f"/jobs/{job_id}/status")
        if status["lifecycleState"] in {"COMPLETED", "CANCELLED", "FAILED"}:
            return status
        time.sleep(0.05)
    msg = f"timed out waiting for job {job_id}"
    raise AssertionError(msg)


def test_deliveries_python_frontend_app_serves_static_and_solve_lifecycle() -> None:
    server, base_url = start_test_server()
    try:
        assert "sf-app" in request_text(base_url, "/")
        assert "export async function boot" in request_text(base_url, "/app/main.mjs")
        assert "sf.createBackend = function" in request_text(base_url, "/sf/sf.js")
        assert request_asset_headers(base_url, "/sf/sf.js") == (
            "application/javascript; charset=utf-8",
            "public, max-age=3600",
        )
        assert request_asset_headers(base_url, "/sf/vendor/leaflet/leaflet.js") == (
            "application/javascript; charset=utf-8",
            "public, max-age=31536000, immutable",
        )

        assert request_json(base_url, "/demo-data") == [
            "PHILADELPHIA",
            "HARTFORD",
            "FIRENZE",
        ]
        assert request_raw(base_url, "/demo-data/UNKNOWN")[0] == 404
        assert request_raw(base_url, "/jobs", {}, "POST")[0] == 400

        demo = request_json(base_url, "/demo-data/HARTFORD")
        assert len(demo["deliveries"]) == 50
        assert len(demo["vehicles"]) == 10
        assert demo["viewState"]["preview"]["unassignedDeliveryIds"] == []

        created = request_json(base_url, "/jobs", demo)
        job_id = created["id"]
        terminal = wait_for_terminal(base_url, job_id)
        assert terminal["lifecycleState"] == "COMPLETED"
        assert terminal["bestScore"] is not None

        snapshot = request_json(base_url, f"/jobs/{job_id}/snapshot")
        solution = snapshot["solution"]
        assigned = [
            delivery_id
            for vehicle in solution["vehicles"]
            for delivery_id in vehicle["deliveryOrder"]
        ]
        assert sorted(assigned) == list(range(len(solution["deliveries"])))
        assert all(
            vehicle["routeCapacityOverage"] == 0 for vehicle in solution["vehicles"]
        )

        routes = request_json(base_url, f"/jobs/{job_id}/routes")
        assert routes["routingMode"] == solution["routingMode"]
        assert routes["bounds"] is not None
        assert any(vehicle["segments"] for vehicle in routes["vehicles"])

        analysis = request_json(base_url, f"/jobs/{job_id}/analysis")
        assert len(analysis["analysis"]["constraints"]) == 4

        recommendation = request_json(
            base_url,
            "/recommendations/delivery-insertions",
            {"plan": solution, "deliveryId": 0, "limit": 3},
        )
        assert recommendation["deliveryId"] == 0
        assert recommendation["candidates"]
        assert len(recommendation["candidates"]) <= 3

        assert request_raw(base_url, f"/jobs/{job_id}", method="DELETE")[0] == 204
        assert request_raw(base_url, f"/jobs/{job_id}/status")[0] == 404
    finally:
        server.shutdown()
