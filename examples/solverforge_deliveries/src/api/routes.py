from __future__ import annotations

import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from http import HTTPStatus
from pathlib import Path
from typing import Any, NoReturn, cast

from fastapi import Body, FastAPI, HTTPException, Request, Response
from fastapi.responses import StreamingResponse
from fastapi.staticfiles import StaticFiles
from solverforge import console
from solverforge.ui import asset as solverforge_ui_asset

from ..data import demo_plan, list_demo_data
from ..domain import build_routes_snapshot, rank_delivery_insertions
from ..solver.service import DeliveryAppState, status_from_manager_error
from .dto import (
    analysis_payload,
    payload_to_plan,
    plan_to_payload,
    snapshot_payload,
    status_payload,
)
from .sse import sse_stream

STATIC_ROOT = Path(__file__).resolve().parents[2] / "static"


def raise_http_error(error: Exception) -> NoReturn:
    raise HTTPException(
        status_code=int(status_from_manager_error(error)),
        detail=str(error),
    )


@asynccontextmanager
async def solver_console_lifespan(_app: FastAPI) -> AsyncIterator[None]:
    console.init()
    yield


def create_app(
    app_state: DeliveryAppState | None = None, *, enable_console: bool = False
) -> FastAPI:
    state = app_state or DeliveryAppState()
    app = FastAPI(
        title="SolverForge Deliveries Python",
        version="0.1.0",
        lifespan=solver_console_lifespan if enable_console else None,
    )
    app.state.deliveries = state

    @app.get("/health")
    def health() -> dict[str, str]:
        return {"status": "UP"}

    @app.get("/info")
    def info() -> dict[str, str]:
        return {
            "name": "solverforge-deliveries-python",
            "version": "0.1.0",
            "solverEngine": "SolverForge Python",
        }

    @app.get("/sf/{path:path}")
    def get_solverforge_ui_asset(path: str) -> Response:
        asset = solverforge_ui_asset(path)
        if asset is None:
            raise HTTPException(status_code=HTTPStatus.NOT_FOUND, detail="not found")
        return Response(
            content=asset.bytes,
            media_type=asset.content_type,
            headers={"cache-control": asset.cache_control},
        )

    @app.get("/demo-data")
    def demo_data_ids() -> list[str]:
        return list_demo_data()

    @app.get("/demo-data/{demo_id}")
    def get_demo_data(demo_id: str) -> dict[str, Any]:
        try:
            return plan_to_payload(demo_plan(demo_id))
        except KeyError as error:
            raise HTTPException(
                status_code=HTTPStatus.NOT_FOUND, detail=str(error)
            ) from error

    @app.post("/jobs")
    def create_job(payload: dict[str, Any] = Body(...)) -> dict[str, str]:
        try:
            plan = payload_to_plan(payload)
            if not plan.deliveries or not plan.vehicles:
                raise ValueError(
                    "delivery jobs require at least one delivery and one vehicle"
                )
            record = state.create_job(plan)
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            raise HTTPException(
                status_code=HTTPStatus.BAD_REQUEST, detail=str(error)
            ) from error
        except Exception as error:
            raise_http_error(error)
        return {"id": record.id}

    @app.get("/jobs/{job_id}")
    def get_job(job_id: str) -> dict[str, Any]:
        try:
            record = state.require_job(job_id)
            return status_payload(record, state.status(record))
        except Exception as error:
            raise_http_error(error)

    @app.get("/jobs/{job_id}/status")
    def get_job_status(job_id: str) -> dict[str, Any]:
        return get_job(job_id)

    @app.get("/jobs/{job_id}/snapshot")
    def get_snapshot(
        job_id: str, snapshot_revision: int | None = None
    ) -> dict[str, Any]:
        try:
            record = state.require_job(job_id)
            plan = state.snapshot(record, snapshot_revision)
            status = state.status(record)
            return snapshot_payload(record, plan, status, snapshot_revision)
        except Exception as error:
            raise_http_error(error)

    @app.get("/jobs/{job_id}/analysis")
    def get_analysis(
        job_id: str, snapshot_revision: int | None = None
    ) -> dict[str, Any]:
        try:
            record = state.require_job(job_id)
            plan = state.snapshot(record, snapshot_revision)
            status = state.status(record)
            return analysis_payload(record, plan, status, snapshot_revision)
        except Exception as error:
            raise_http_error(error)

    @app.get("/jobs/{job_id}/routes")
    def get_routes(job_id: str, snapshot_revision: int | None = None) -> dict[str, Any]:
        try:
            record = state.require_job(job_id)
            plan = state.snapshot(record, snapshot_revision)
            status = state.status(record)
            revision = snapshot_revision
            if revision is None:
                revision = int(cast(Any, status.get("latest_snapshot_revision") or 0))
            return {
                "id": record.id,
                "jobId": record.id,
                "snapshotRevision": revision,
                **build_routes_snapshot(plan),
            }
        except Exception as error:
            raise_http_error(error)

    @app.get("/jobs/{job_id}/events")
    async def get_events(job_id: str, request: Request) -> StreamingResponse:
        try:
            record = state.require_job(job_id)
            bootstrap = state.bootstrap_event(record)
        except Exception as error:
            raise_http_error(error)
        return StreamingResponse(
            sse_stream(request, record, bootstrap),
            media_type="text/event-stream",
            headers={
                "cache-control": "no-cache",
                "connection": "keep-alive",
                "x-accel-buffering": "no",
            },
        )

    @app.post("/jobs/{job_id}/pause")
    def pause_job(job_id: str) -> Response:
        try:
            state.pause(state.require_job(job_id))
        except Exception as error:
            raise_http_error(error)
        return Response(status_code=HTTPStatus.ACCEPTED)

    @app.post("/jobs/{job_id}/resume")
    def resume_job(job_id: str) -> Response:
        try:
            state.resume(state.require_job(job_id))
        except Exception as error:
            raise_http_error(error)
        return Response(status_code=HTTPStatus.ACCEPTED)

    @app.post("/jobs/{job_id}/cancel")
    def cancel_job(job_id: str) -> Response:
        try:
            state.cancel(state.require_job(job_id))
        except Exception as error:
            raise_http_error(error)
        return Response(status_code=HTTPStatus.ACCEPTED)

    @app.delete("/jobs/{job_id}")
    def delete_job(job_id: str) -> Response:
        try:
            state.delete_job(job_id)
        except Exception as error:
            raise_http_error(error)
        return Response(status_code=HTTPStatus.NO_CONTENT)

    @app.post("/recommendations/delivery-insertions")
    def recommend_delivery_insertions(
        payload: dict[str, Any] = Body(...),
    ) -> dict[str, Any]:
        try:
            plan = payload_to_plan(payload["plan"])
            delivery_id = int(payload["deliveryId"])
            limit = min(int(payload.get("limit", 8)), 24)
            if not 0 <= delivery_id < len(plan.deliveries):
                raise ValueError(f"delivery id {delivery_id} is out of range")
            candidates = [
                {
                    **candidate,
                    "previewPlan": plan_to_payload(candidate["previewPlan"]),
                }
                for candidate in rank_delivery_insertions(plan, delivery_id, limit)
            ]
            return {"deliveryId": delivery_id, "candidates": candidates}
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            raise HTTPException(
                status_code=HTTPStatus.BAD_REQUEST, detail=str(error)
            ) from error

    @app.api_route("/{_path:path}", methods=["POST", "PUT", "PATCH"])
    def unmatched_mutation_route(_path: str) -> None:
        raise HTTPException(status_code=HTTPStatus.NOT_FOUND, detail="not found")

    app.mount("/", StaticFiles(directory=STATIC_ROOT, html=True), name="static")
    return app
