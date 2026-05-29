from __future__ import annotations

import json
import socket
import threading
import time
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import uvicorn

from examples.solverforge_hospital import create_app


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


def read_first_sse_event(base_url: str, job_id: str, timeout_seconds: float = 10.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    with urlopen(f"{base_url}/jobs/{job_id}/events", timeout=5) as response:
        assert (response.headers.get("content-type") or "").startswith("text/event-stream")
        assert response.headers.get("x-accel-buffering") == "no"
        while time.monotonic() < deadline:
            line = response.readline().decode("utf-8")
            if not line.startswith("data: "):
                continue
            payload = json.loads(line.removeprefix("data: "))
            assert isinstance(payload, dict)
            return payload
    msg = f"timed out waiting for first SSE event from job {job_id}"
    raise AssertionError(msg)


def read_initial_sse_activity(base_url: str, job_id: str) -> tuple[dict[str, Any], bool]:
    with urlopen(f"{base_url}/jobs/{job_id}/events", timeout=5) as response:
        assert (response.headers.get("content-type") or "").startswith("text/event-stream")
        assert response.headers.get("x-accel-buffering") == "no"
        first_event: dict[str, Any] | None = None
        saw_keep_alive = False
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            line = response.readline().decode("utf-8")
            if line.startswith("data: ") and first_event is None:
                payload = json.loads(line.removeprefix("data: "))
                assert isinstance(payload, dict)
                first_event = payload
            elif line.startswith(": keep-alive"):
                saw_keep_alive = True
            if first_event is not None and saw_keep_alive:
                return first_event, saw_keep_alive
    msg = f"timed out waiting for initial SSE activity from job {job_id}"
    raise AssertionError(msg)


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


def wait_for_status(
    base_url: str,
    job_id: str,
    predicate: Any,
    timeout_seconds: float = 10.0,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        status = request_json(base_url, f"/jobs/{job_id}/status")
        if predicate(status):
            return status
        time.sleep(0.05)
    msg = f"timed out waiting for job {job_id}"
    raise AssertionError(msg)


def wait_for_terminal(base_url: str, job_id: str, timeout_seconds: float = 40.0) -> dict[str, Any]:
    return wait_for_status(
        base_url,
        job_id,
        lambda status: status["lifecycleState"] in {"COMPLETED", "CANCELLED", "FAILED"},
        timeout_seconds=timeout_seconds,
    )


def test_hospital_python_frontend_app_serves_static_and_solve_lifecycle() -> None:
    server, base_url = start_test_server()
    try:
        assert "sf-app" in request_text(base_url, "/")
        assert "function createBackend" in request_text(base_url, "/sf/sf.js")
        assert "bootApp" in request_text(base_url, "/app/main.mjs")

        assert request_json(base_url, "/health") == {"status": "UP"}
        assert request_json(base_url, "/demo-data") == ["LARGE"]
        assert request_raw(base_url, "/jobs", {}, "POST")[0] == 400
        assert request_raw(base_url, "/schedules", {}, "POST")[0] == 404

        demo = request_json(base_url, "/demo-data/LARGE")
        assert len(demo["employees"]) == 50
        assert len(demo["shifts"]) == 688
        assert demo["score"] is None
        assert all(shift["employeeIdx"] is None for shift in demo["shifts"])

        created = request_json(base_url, "/jobs", demo)
        job_id = created["id"]
        status = request_json(base_url, f"/jobs/{job_id}/status")
        assert status["id"] == job_id
        assert status["jobId"] == job_id
        assert status["lifecycleState"] in {"SOLVING", "COMPLETED"}
        assert "checkpointAvailable" in status
        assert "telemetry" in status
        initial_event, saw_keep_alive = read_initial_sse_activity(base_url, job_id)
        assert initial_event["eventType"] in {"progress", "best_solution"}
        assert saw_keep_alive

        terminal = wait_for_terminal(base_url, job_id)
        assert terminal["lifecycleState"] == "COMPLETED"
        assert terminal["terminalReason"] == "terminated_by_config"
        assert terminal["bestScore"].startswith("0hard/")
        assert terminal["telemetry"]["stepCount"] > len(demo["shifts"])
        assert terminal["telemetry"]["movesGenerated"] > len(demo["shifts"]) * len(demo["employees"])

        snapshot = request_json(base_url, f"/jobs/{job_id}/snapshot")
        assert snapshot["solution"]["score"].startswith("0hard/")
        assert len(snapshot["solution"]["employees"]) == 50
        assert len(snapshot["solution"]["shifts"]) == 688
        assert all(
            shift["employeeIdx"] is not None
            for shift in snapshot["solution"]["shifts"]
        )

        analysis = request_json(base_url, f"/jobs/{job_id}/analysis")
        assert analysis["analysis"]["score"].startswith("0hard/")
        assert len(analysis["analysis"]["constraints"]) == 9

        event = read_first_sse_event(base_url, job_id)
        assert event["eventType"] == "completed"
        assert event["solution"] is not None

        assert request_raw(base_url, f"/jobs/{job_id}/cancel", {}, "POST")[0] == 409
        assert request_raw(base_url, f"/jobs/{job_id}", method="DELETE")[0] == 204
        assert request_raw(base_url, f"/jobs/{job_id}/status")[0] == 404
    finally:
        server.shutdown()


def test_hospital_python_frontend_app_retained_async_controls() -> None:
    server, base_url = start_test_server()
    try:
        demo = request_json(base_url, "/demo-data/LARGE")
        job_id = request_json(base_url, "/jobs", demo)["id"]

        wait_for_status(
            base_url,
            job_id,
            lambda status: status["lifecycleState"] in {"SOLVING", "PAUSE_REQUESTED"},
        )
        assert request_raw(base_url, f"/jobs/{job_id}", method="DELETE")[0] == 409

        assert request_raw(base_url, f"/jobs/{job_id}/pause", {}, "POST")[0] == 202
        paused = wait_for_status(
            base_url,
            job_id,
            lambda status: status["lifecycleState"] == "PAUSED",
        )
        assert paused["snapshotRevision"] is not None
        assert request_raw(base_url, f"/jobs/{job_id}", method="DELETE")[0] == 409

        paused_snapshot = request_json(base_url, f"/jobs/{job_id}/snapshot")
        assert paused_snapshot["lifecycleState"] == "PAUSED"
        assert paused_snapshot["solution"]["shifts"]

        assert request_raw(base_url, f"/jobs/{job_id}/resume", {}, "POST")[0] == 202
        wait_for_status(
            base_url,
            job_id,
            lambda status: status["lifecycleState"] in {"SOLVING", "PAUSE_REQUESTED"},
        )

        assert request_raw(base_url, f"/jobs/{job_id}/cancel", {}, "POST")[0] == 202
        terminal = wait_for_terminal(base_url, job_id)
        assert terminal["lifecycleState"] == "CANCELLED"
        event = read_first_sse_event(base_url, job_id)
        assert event["eventType"] == "cancelled"
        assert request_raw(base_url, f"/jobs/{job_id}", method="DELETE")[0] == 204
        assert request_raw(base_url, f"/jobs/{job_id}/status")[0] == 404
    finally:
        server.shutdown()
