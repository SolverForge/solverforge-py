from __future__ import annotations

import os
import shutil
import socket
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

import uvicorn
from playwright.sync_api import Browser, Page, Playwright, sync_playwright
from playwright.sync_api import expect as playwright_expect

from examples.solverforge_deliveries import create_app as create_deliveries_app
from examples.solverforge_hospital import create_app as create_hospital_app


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
                    "/sf/sf.0.6.5.css",
                    "/sf/sf.0.6.5.js",
                    "/sf/sf.0.6.5.mjs",
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
                playwright_expect(page.locator("#sfScoreDisplay")).to_contain_text(
                    "0hard/"
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
                assert browser_errors == []
            finally:
                browser.close()
    finally:
        server.shutdown()
