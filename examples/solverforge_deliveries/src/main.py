from __future__ import annotations

import uvicorn
from solverforge import console

from .api.routes import create_app

__all__ = ["app", "create_app", "serve"]

app = create_app(enable_console=True)


def serve(host: str = "127.0.0.1", port: int = 7861) -> None:
    console.init()
    print(f"Serving SolverForge Deliveries Python at http://{host}:{port}")
    uvicorn.run(
        create_app(enable_console=True),
        host=host,
        port=port,
        log_level="info",
        access_log=False,
    )
