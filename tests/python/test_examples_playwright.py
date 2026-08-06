from __future__ import annotations

import base64
import os
import re
import shutil
import socket
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlsplit

import uvicorn
from playwright.sync_api import Browser, Page, Playwright, sync_playwright
from playwright.sync_api import expect as playwright_expect

from examples.solverforge_deliveries import create_app as create_deliveries_app
from examples.solverforge_hospital import create_app as create_hospital_app

SCORE_TEXT_RE = re.compile(r"-?\d+(?:\.\d+)?hard/-?\d+(?:\.\d+)?soft")
OPENSTREETMAP_TILE_HOSTS = {
    "a.tile.openstreetmap.org",
    "b.tile.openstreetmap.org",
    "c.tile.openstreetmap.org",
}
TRANSPARENT_PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMB/ax"
    "p06QAAAAASUVORK5CYII="
)


@dataclass
class RunningServer:
    server: uvicorn.Server
    thread: threading.Thread

    def shutdown(self) -> None:
        self.server.should_exit = True
        self.thread.join(timeout=10)
        assert not self.thread.is_alive()


def start_test_server(app_factory: Callable[[], Any]) -> tuple[RunningServer, str]:
    host = "127.0.0.1"
    port = free_port()
    config = uvicorn.Config(
        app_factory(),
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
    import json
    from urllib.request import urlopen

    deadline = time.monotonic() + 10
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with urlopen(f"{base_url}/health", timeout=2) as response:
                if json.loads(response.read().decode("utf-8")) == {"status": "UP"}:
                    return
        except Exception as error:
            last_error = error
        time.sleep(0.05)
    msg = f"server did not become ready: {last_error}"
    raise AssertionError(msg)


def chromium_executable() -> str | None:
    configured = os.environ.get("SOLVERFORGE_PLAYWRIGHT_CHROMIUM")
    if configured:
        return configured
    return (
        shutil.which("chromium")
        or shutil.which("chromium-browser")
        or shutil.which("google-chrome")
        or shutil.which("chrome")
    )


def launch_browser(playwright: Playwright) -> Browser:
    executable_path = chromium_executable()
    kwargs: dict[str, Any] = {"headless": True}
    if executable_path:
        kwargs["executable_path"] = executable_path
    return playwright.chromium.launch(**kwargs)


def collect_browser_errors(page: Page) -> list[str]:
    errors: list[str] = []
    page.on(
        "console",
        lambda message: (
            errors.append(message.text) if message.type == "error" else None
        ),
    )
    page.on("pageerror", lambda error: errors.append(str(error)))
    return errors


def is_openstreetmap_tile_url(url: str) -> bool:
    parsed = urlsplit(url)
    return (
        parsed.scheme == "https"
        and parsed.netloc in OPENSTREETMAP_TILE_HOSTS
        and parsed.path.endswith(".png")
    )


def stub_external_map_tiles(page: Page, base_url: str) -> list[str]:
    base_origin = urlsplit(base_url).netloc
    external_urls: list[str] = []

    def handle_route(route: Any) -> None:
        request = route.request
        parsed = urlsplit(request.url)
        if parsed.scheme in {"http", "https"} and parsed.netloc != base_origin:
            external_urls.append(request.url)
            if request.resource_type == "image" and is_openstreetmap_tile_url(
                request.url
            ):
                route.fulfill(
                    status=200,
                    content_type="image/png",
                    body=TRANSPARENT_PNG,
                )
            else:
                route.abort()
            return
        route.continue_()

    page.route("**/*", handle_route)
    return external_urls


def test_browser_imports_solverforge_ui_module_assets() -> None:
    from urllib.error import HTTPError
    from urllib.request import urlopen

    server, base_url = start_test_server(create_hospital_app)
    try:
        with sync_playwright() as playwright:
            browser = launch_browser(playwright)
            try:
                page = browser.new_page()
                browser_errors = collect_browser_errors(page)

                page.goto(base_url, wait_until="networkidle")
                result = page.evaluate("""async () => {
                      const module = await import('/sf/sf.mjs');
                      return {
                        version: module.version,
                        createBackendType: typeof module.createBackend,
                        defaultMatches: module.default.createBackend === module.createBackend,
                      };
                    }""")

                assert result == {
                    "version": "0.7.0",
                    "createBackendType": "function",
                    "defaultMatches": True,
                }
                assert browser_errors == []

                for path in (
                    "/sf/sf.0.6.6.css",
                    "/sf/sf.0.6.6.js",
                    "/sf/sf.0.6.6.mjs",
                    "/sf/sf.0.7.0.mjs",
                ):
                    try:
                        urlopen(f"{base_url}{path}", timeout=2)
                    except HTTPError as error:
                        assert error.code == 404
                    else:
                        raise AssertionError(f"{path} should not be served")
            finally:
                browser.close()
    finally:
        server.shutdown()


def test_hospital_browser_solves_and_opens_analysis_modal() -> None:
    server, base_url = start_test_server(create_hospital_app)
    try:
        with sync_playwright() as playwright:
            browser = launch_browser(playwright)
            try:
                page = browser.new_page()
                browser_errors = collect_browser_errors(page)

                page.goto(base_url, wait_until="networkidle")
                playwright_expect(page.locator(".sf-header-title")).to_have_text(
                    "SolverForge Hospital"
                )
                playwright_expect(
                    page.locator(".sf-nav-btn", has_text="By location")
                ).to_be_visible()
                playwright_expect(
                    page.locator(".sf-nav-btn", has_text="By employee")
                ).to_be_visible()
                playwright_expect(page.locator("#view-by-location")).to_contain_text(
                    "Location schedule"
                )

                page.get_by_role("button", name="Solve").click()
                playwright_expect(page.locator("#sfStatusText")).to_have_text(
                    "Completed",
                    timeout=70_000,
                )
                score_text = page.locator("#sfScoreDisplay").inner_text()
                assert SCORE_TEXT_RE.fullmatch(score_text), score_text
                playwright_expect(page.locator("#sf-app")).to_have_attribute(
                    "data-lifecycle-state",
                    "COMPLETED",
                )
                playwright_expect(page.locator("#sf-app")).to_have_attribute(
                    "data-snapshot-revision",
                    re.compile(r"\d+"),
                )

                page.locator('button[title="Score Analysis"]').click()
                dialog = page.get_by_role("dialog", name="Score Analysis")
                playwright_expect(dialog).to_be_visible()
                playwright_expect(dialog).to_contain_text("Assigned shift")
                assert browser_errors == []
            finally:
                browser.close()
    finally:
        server.shutdown()


def test_deliveries_browser_recommends_insertions_for_assigned_delivery() -> None:
    server, base_url = start_test_server(create_deliveries_app)
    try:
        with sync_playwright() as playwright:
            browser = launch_browser(playwright)
            try:
                page = browser.new_page()
                browser_errors = collect_browser_errors(page)
                external_urls = stub_external_map_tiles(page, base_url)

                page.goto(base_url, wait_until="networkidle")
                playwright_expect(page.locator(".sf-header-title")).to_have_text(
                    "SolverForge Deliveries"
                )
                playwright_expect(page.locator(".deliveries-kpis")).to_contain_text(
                    "Unassigned"
                )
                playwright_expect(
                    page.locator("#deliveries-map .leaflet-marker-icon").first
                ).to_be_visible()

                page.locator(".sf-nav-btn", has_text="Data").click()
                playwright_expect(
                    page.locator("h3", has_text="Draft Data")
                ).to_be_visible()
                recommend_button = page.get_by_role("button", name="Recommend").first
                playwright_expect(recommend_button).to_be_visible()
                recommend_button.click()

                dialog = page.get_by_role(
                    "dialog",
                    name="Delivery Insertion Recommendations",
                )
                playwright_expect(dialog).to_be_visible()
                playwright_expect(dialog).not_to_contain_text("No valid insertions.")
                playwright_expect(
                    dialog.get_by_role("button", name="Apply").first
                ).to_be_visible()
                assert all(
                    is_openstreetmap_tile_url(url) for url in external_urls
                ), external_urls
                assert browser_errors == []
            finally:
                browser.close()
    finally:
        server.shutdown()
