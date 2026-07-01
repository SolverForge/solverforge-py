# SolverForge Python Makefile
# Mixed Python/Rust binding workflow aligned with the SolverForge ecosystem.

# ============== Colors & Symbols ==============
GREEN := \033[92m
EMERALD := \033[38;2;16;185;129m
CYAN := \033[96m
YELLOW := \033[93m
RED := \033[91m
GRAY := \033[90m
BOLD := \033[1m
RESET := \033[0m

CHECK := OK
CROSS := FAIL
ARROW := =>
PROGRESS := ..

# ============== Project Metadata ==============
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' pyproject.toml | head -1)
CRATE_VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
RUST_VERSION := 1.95.0
HOST_PYTHON ?= python3.14
VENV ?= $(CURDIR)/.venv
PYTHON ?= $(VENV)/bin/python
PIP ?= $(VENV)/bin/pip
MATURIN ?= $(VENV)/bin/maturin
PYTEST ?= $(PYTHON) -m pytest
MYPY ?= $(PYTHON) -m mypy
RUFF ?= $(PYTHON) -m ruff
BLACK ?= $(PYTHON) -m black --line-length 100 --target-version py314
PLAYWRIGHT ?= $(PYTHON) -m playwright
MATURIN_VERSION := 1.13.3
BLACK_VERSION := 26.5.1
PRE_COMMIT ?= $(PYTHON) -m pre_commit
TWINE ?= $(PYTHON) -m twine
PYTEST_ARGS ?=
TEST ?=
APP_HOST ?= 127.0.0.1
PORT ?= 7860
DIST_DIR ?= $(CURDIR)/dist
SOLVERFORGE_RELEASE_VERSION := $(shell $(HOST_PYTHON) scripts/verify_solverforge_release_base.py --print-version 2>/dev/null || printf unknown)
MYPY_PATHS := python/solverforge examples/solverforge_hospital examples/solverforge_deliveries
PY_STYLE_PATHS := python tests examples scripts
DOC_CHECK_PATHS := README.md AGENTS.md WIREFRAME.md docs examples/solverforge_hospital/README.md examples/solverforge_deliveries/README.md vendor/solverforge-ui/README.md
PY_DEPS_STAMP := $(VENV)/.solverforge-py-deps
PLAYWRIGHT_BROWSERS_STAMP := $(VENV)/.solverforge-py-playwright-chromium
PY_LIBDIR := $(shell $(HOST_PYTHON) -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR") or "")')
PY_LDLIBRARY := $(shell $(HOST_PYTHON) -c 'import sysconfig; print(sysconfig.get_config_var("LDLIBRARY") or "")')
PY_INSTSONAME := $(shell $(HOST_PYTHON) -c 'import sysconfig; print(sysconfig.get_config_var("INSTSONAME") or sysconfig.get_config_var("LDLIBRARY") or "")')
PY_LINK_DIR := $(CURDIR)/target/libpython-link

# ============== Phony Targets ==============
.PHONY: banner help doctor venv install-python-deps install-playwright-browsers python-link develop develop-debug build build-wheel \
		        build-sdist build-dist build-release check test test-quick rust-test py-test test-hospital test-deliveries test-one \
	        test-examples-browser rust-test-one typecheck ruff lint fmt fmt-check py-format py-format-check clippy pre-commit docs-check \
        ci-local audit pre-release release-base-check release-upstream-check dist-check smoke-wheel \
	        hospital-run hospital-solve deliveries-run deliveries-solve version release-info clean clean-dist clean-py clean-venv

.DEFAULT_GOAL := help

# ============== Banner ==============
banner:
	@printf -- "$(EMERALD)$(BOLD)  ____        _                _____\n"
	@printf -- " / ___|  ___ | |_   _____ _ __|  ___|__  _ __ __ _  ___\n"
	@printf -- " \\___ \\\\ / _ \\\\| \\\\ \\\\ / / _ \\\\ '__| |_ / _ \\\\| '__/ _\` |/ _ \\\\\n"
	@printf -- "  ___) | (_) | |\\\\ V /  __/ |  |  _| (_) | | | (_| |  __/\n"
	@printf -- " |____/ \\\\___/|_| \\_/ \\___|_|  |_|  \\___/|_|  \\__, |\\___|\n"
	@printf -- "                                             |___/$(RESET)\n"
	@printf -- "  $(GRAY)v$(VERSION)$(RESET) $(EMERALD)SolverForge Python Build System$(RESET)\n\n"

# ============== Environment ==============
doctor: banner
	@printf -- "$(CYAN)$(BOLD)==== Toolchain ======================================$(RESET)\n\n"
	@$(HOST_PYTHON) -c 'import sys; assert sys.version_info[:2] == (3, 14), sys.version; print("$(CHECK) host python", sys.version.split()[0])'
	@rustc --version
	@cargo --version
	@printf -- "$(GREEN)$(CHECK) Toolchain commands are available$(RESET)\n\n"

venv:
	@if [ -x "$(PYTHON)" ]; then \
		current="$$("$(PYTHON)" -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')"; \
		expected="$$("$(HOST_PYTHON)" -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')"; \
		if [ "$$current" != "$$expected" ]; then rm -rf "$(VENV)"; fi; \
	fi
	@if [ ! -x "$(PYTHON)" ]; then "$(HOST_PYTHON)" -m venv "$(VENV)"; fi

install-python-deps: $(PY_DEPS_STAMP)

$(PY_DEPS_STAMP): pyproject.toml Makefile | venv
	@printf -- "$(PROGRESS) Installing Python runtime and developer tools...\n"
	@PIP_DISABLE_PIP_VERSION_CHECK=1 "$(PIP)" install \
		"maturin==$(MATURIN_VERSION)" "fastapi>=0.115,<1" "uvicorn>=0.32,<1" \
		"playwright>=1.55,<2" pytest mypy ruff "black==$(BLACK_VERSION)" pre-commit twine
	@touch "$(PY_DEPS_STAMP)"
	@printf -- "$(GREEN)$(CHECK) Python dependencies installed$(RESET)\n\n"

install-playwright-browsers: $(PLAYWRIGHT_BROWSERS_STAMP)

$(PLAYWRIGHT_BROWSERS_STAMP): $(PY_DEPS_STAMP)
	@printf -- "$(PROGRESS) Installing Playwright Chromium browser...\n"
	@$(PLAYWRIGHT) install chromium
	@touch "$(PLAYWRIGHT_BROWSERS_STAMP)"
	@printf -- "$(GREEN)$(CHECK) Playwright Chromium browser installed$(RESET)\n\n"

python-link: $(PY_LINK_DIR)/$(PY_LDLIBRARY)

$(PY_LINK_DIR)/$(PY_LDLIBRARY):
	@mkdir -p "$(PY_LINK_DIR)"
	@test -n "$(PY_LIBDIR)" || (printf -- "$(RED)$(CROSS) Python LIBDIR is empty$(RESET)\n" && exit 1)
	@test -n "$(PY_INSTSONAME)" || (printf -- "$(RED)$(CROSS) Python INSTSONAME is empty$(RESET)\n" && exit 1)
	@test -e "$(PY_LIBDIR)/$(PY_INSTSONAME)" || (printf -- "$(RED)$(CROSS) Missing $(PY_LIBDIR)/$(PY_INSTSONAME)$(RESET)\n" && exit 1)
	@ln -sf "$(PY_LIBDIR)/$(PY_INSTSONAME)" "$@"

# ============== Build Targets ==============
develop: install-python-deps
	@printf -- "$(PROGRESS) Installing release native extension into $(VENV)...\n"
	@$(MATURIN) develop --release --locked --pip-path "$(PIP)"
	@printf -- "$(GREEN)$(CHECK) Development install complete$(RESET)\n\n"

develop-debug: install-python-deps
	@printf -- "$(PROGRESS) Installing debug native extension into $(VENV)...\n"
	@$(MATURIN) develop --locked --pip-path "$(PIP)"
	@printf -- "$(GREEN)$(CHECK) Debug development install complete$(RESET)\n\n"

build: banner
	@printf -- "$(CYAN)$(BOLD)==== Rust Build =====================================$(RESET)\n\n"
	@cargo build --locked && printf -- "$(GREEN)$(CHECK) Rust build passed$(RESET)\n\n"

build-wheel: install-python-deps
	@printf -- "$(PROGRESS) Building debug Python wheel...\n"
	@$(MATURIN) build --locked -i "$(PYTHON)"
	@printf -- "$(GREEN)$(CHECK) Wheel build passed$(RESET)\n\n"

build-sdist: release-base-check install-python-deps
	@printf -- "$(PROGRESS) Building release source distribution...\n"
	@mkdir -p "$(DIST_DIR)"
	@$(MATURIN) sdist -o "$(DIST_DIR)"
	@printf -- "$(GREEN)$(CHECK) Source distribution build passed$(RESET)\n\n"

build-release: install-python-deps
	@printf -- "$(CYAN)$(BOLD)==== Release Wheel ==================================$(RESET)\n\n"
	@mkdir -p "$(DIST_DIR)"
	@$(MATURIN) build --release --locked --strip --compatibility pypi -i "$(PYTHON)" --out "$(DIST_DIR)"
	@printf -- "$(GREEN)$(CHECK) Release wheel build passed$(RESET)\n\n"

build-dist: clean-dist build-sdist build-release
	@printf -- "$(GREEN)$(CHECK) Release distributions written to $(DIST_DIR)$(RESET)\n\n"

check: banner
	@printf -- "$(PROGRESS) Running cargo check...\n"
	@cargo check --locked && printf -- "$(GREEN)$(CHECK) Cargo check passed$(RESET)\n\n"

# ============== Test Targets ==============
test: rust-test py-test

test-quick: develop
	@printf -- "$(PROGRESS) Running quick Python regression suite...\n"
	@$(PYTEST) -q tests/python/test_config.py tests/python/test_decorators.py \
		tests/python/test_constraints.py tests/python/test_scalar_solving.py \
		tests/python/test_list_solving.py $(PYTEST_ARGS)

rust-test: python-link banner
	@printf -- "$(CYAN)$(BOLD)==== Rust Tests =====================================$(RESET)\n\n"
	@LIBRARY_PATH="$(PY_LINK_DIR):$${LIBRARY_PATH:-}" cargo test --locked && printf -- "\n$(GREEN)$(CHECK) Rust tests passed$(RESET)\n\n"

py-test: develop install-playwright-browsers
	@printf -- "$(CYAN)$(BOLD)==== Python Tests ===================================$(RESET)\n\n"
	@$(PYTEST) $(PYTEST_ARGS) && printf -- "\n$(GREEN)$(CHECK) Python tests passed$(RESET)\n\n"

test-hospital: develop install-playwright-browsers
	@printf -- "$(PROGRESS) Running hospital example tests...\n"
	@$(PYTEST) tests/python/test_hospital_example.py tests/python/test_hospital_frontend_app.py \
		tests/python/test_examples_playwright.py::test_hospital_browser_solves_and_opens_analysis_modal \
		$(PYTEST_ARGS)

test-deliveries: develop install-playwright-browsers
	@printf -- "$(PROGRESS) Running deliveries example tests...\n"
	@$(PYTEST) tests/python/test_deliveries_example.py tests/python/test_deliveries_frontend_app.py \
		tests/python/test_examples_playwright.py::test_deliveries_browser_recommends_insertions_for_assigned_delivery \
		$(PYTEST_ARGS)

test-examples-browser: develop install-playwright-browsers
	@printf -- "$(PROGRESS) Running example browser tests...\n"
	@$(PYTEST) tests/python/test_examples_playwright.py $(PYTEST_ARGS)

test-one: develop
	@test -n "$(TEST)" || (printf -- "$(RED)$(CROSS) Set TEST=pattern$(RESET)\n" && exit 1)
	@$(PYTEST) -k "$(TEST)" -vv $(PYTEST_ARGS)

rust-test-one: python-link
	@test -n "$(TEST)" || (printf -- "$(RED)$(CROSS) Set TEST=pattern$(RESET)\n" && exit 1)
	@LIBRARY_PATH="$(PY_LINK_DIR):$${LIBRARY_PATH:-}" cargo test --locked "$(TEST)" -- --nocapture

# ============== Lint & Format ==============
typecheck: install-python-deps
	@printf -- "$(PROGRESS) Running mypy...\n"
	@$(MYPY) $(MYPY_PATHS) && printf -- "$(GREEN)$(CHECK) Typecheck passed$(RESET)\n"

ruff: install-python-deps
	@printf -- "$(PROGRESS) Running ruff...\n"
	@$(RUFF) check $(PY_STYLE_PATHS) && printf -- "$(GREEN)$(CHECK) Ruff passed$(RESET)\n"

lint: banner fmt-check ruff typecheck clippy
	@printf -- "\n$(GREEN)$(BOLD)$(CHECK) All lint checks passed$(RESET)\n\n"

fmt:
	@printf -- "$(PROGRESS) Formatting Rust code...\n"
	@cargo fmt --all
	@printf -- "$(GREEN)$(CHECK) Formatting complete$(RESET)\n"

fmt-check:
	@printf -- "$(PROGRESS) Checking Rust formatting...\n"
	@cargo fmt --all -- --check
	@printf -- "$(GREEN)$(CHECK) Formatting valid$(RESET)\n"

py-format: install-python-deps
	@printf -- "$(PROGRESS) Formatting Python code with Black...\n"
	@$(BLACK) $(PY_STYLE_PATHS)

py-format-check: install-python-deps
	@printf -- "$(PROGRESS) Checking Python formatting with Black...\n"
	@$(BLACK) --check $(PY_STYLE_PATHS)

clippy: banner
	@printf -- "$(PROGRESS) Running clippy...\n"
	@cargo clippy --locked --all-targets -- -D warnings && printf -- "$(GREEN)$(CHECK) Clippy passed$(RESET)\n"

pre-commit: install-python-deps
	@printf -- "$(PROGRESS) Running pre-commit hooks...\n"
	@$(PRE_COMMIT) run --all-files

docs-check:
	@printf -- "$(PROGRESS) Checking documentation surface...\n"
	@test -f README.md
	@test -f AGENTS.md
	@test -f WIREFRAME.md
	@test -f docs/release.md
	@test -f examples/solverforge_hospital/README.md
	@test -f examples/solverforge_deliveries/README.md
	@test -f vendor/solverforge-ui/README.md
	@! rg -n "move_count_limit|bounded scalar assignment fallback|currently contains scaffolding|owner-aware, nearby, swap, sublist, reverse, k-opt, and ruin selectors .*not yet|no committed history yet|/home/pvd/dev/solverforge" $(DOC_CHECK_PATHS)
	@printf -- "$(GREEN)$(CHECK) Documentation surface looks current$(RESET)\n"

release-base-check:
	@printf -- "$(PROGRESS) Checking SolverForge release dependency base...\n"
	@$(HOST_PYTHON) scripts/verify_solverforge_release_base.py
	@printf -- "$(GREEN)$(CHECK) SolverForge dependency base is locked$(RESET)\n"

release-upstream-check: release-base-check

dist-check: install-python-deps
	@printf -- "$(PROGRESS) Checking release distributions...\n"
	@test -d "$(DIST_DIR)" || (printf -- "$(RED)$(CROSS) Missing $(DIST_DIR); run make build-dist$(RESET)\n" && exit 1)
	@$(TWINE) check "$(DIST_DIR)"/*
	@$(PYTHON) scripts/verify_release_artifacts.py --dist "$(DIST_DIR)"
	@printf -- "$(GREEN)$(CHECK) Release distributions passed metadata checks$(RESET)\n\n"

smoke-wheel:
	@printf -- "$(PROGRESS) Smoke testing built wheel in a clean venv...\n"
	@test -d "$(DIST_DIR)" || (printf -- "$(RED)$(CROSS) Missing $(DIST_DIR); run make build-dist$(RESET)\n" && exit 1)
	@set -e; \
		tmpdir="$$(mktemp -d)"; \
		trap 'rm -rf "$$tmpdir"' EXIT; \
		"$(HOST_PYTHON)" -m venv "$$tmpdir/venv"; \
		"$$tmpdir/venv/bin/python" -m pip install --disable-pip-version-check --no-index --find-links "$(DIST_DIR)" "solverforge==$(VERSION)"; \
		"$$tmpdir/venv/bin/python" -c 'import solverforge; print(solverforge.__version__); from solverforge import Solver, ConstraintFactory, SolverForgeError'
	@printf -- "$(GREEN)$(CHECK) Wheel smoke test passed$(RESET)\n\n"

# ============== CI Simulation ==============
ci-local: banner
	@printf -- "$(CYAN)$(BOLD)==== Local CI Simulation ============================$(RESET)\n\n"
	@printf -- "$(PROGRESS) Step 1/8: format check...\n"
	@$(MAKE) fmt-check --no-print-directory
	@printf -- "$(PROGRESS) Step 2/8: cargo check...\n"
	@$(MAKE) check --no-print-directory
	@printf -- "$(PROGRESS) Step 3/8: clippy...\n"
	@$(MAKE) clippy --no-print-directory
	@printf -- "$(PROGRESS) Step 4/8: Rust tests...\n"
	@$(MAKE) rust-test --no-print-directory
	@printf -- "$(PROGRESS) Step 5/8: Python tests...\n"
	@$(MAKE) py-test --no-print-directory
	@printf -- "$(PROGRESS) Step 6/8: typecheck...\n"
	@$(MAKE) typecheck --no-print-directory
	@printf -- "$(PROGRESS) Step 7/8: ruff...\n"
	@$(MAKE) ruff --no-print-directory
	@printf -- "$(PROGRESS) Step 8/8: docs-check...\n"
	@$(MAKE) docs-check --no-print-directory
	@printf -- "\n$(GREEN)$(BOLD)$(CHECK) Local CI simulation passed$(RESET)\n\n"

audit: ci-local

pre-release: banner
	@printf -- "$(CYAN)$(BOLD)==== Pre-Release Validation ========================$(RESET)\n\n"
	@$(MAKE) release-base-check --no-print-directory
	@$(MAKE) ci-local --no-print-directory
	@$(MAKE) build-dist --no-print-directory
	@$(MAKE) dist-check --no-print-directory
	@$(MAKE) smoke-wheel --no-print-directory
	@printf -- "\n$(GREEN)$(BOLD)$(CHECK) Ready for release v$(VERSION)$(RESET)\n\n"

# ============== Example App ==============
hospital-run: develop
	@printf -- "$(PROGRESS) Serving hospital example at http://$(APP_HOST):$(PORT) ...\n"
	@$(PYTHON) -m examples.solverforge_hospital --host "$(APP_HOST)" --port "$(PORT)"

hospital-solve: develop
	@printf -- "$(PROGRESS) Running hospital terminal solve...\n"
	@$(PYTHON) -m examples.solverforge_hospital --solve

deliveries-run: develop
	@printf -- "$(PROGRESS) Serving deliveries example at http://$(APP_HOST):$(PORT) ...\n"
	@$(PYTHON) -m examples.solverforge_deliveries --host "$(APP_HOST)" --port "$(PORT)"

deliveries-solve: develop
	@printf -- "$(PROGRESS) Running deliveries terminal solve...\n"
	@$(PYTHON) -m examples.solverforge_deliveries --solve

# ============== Utilities ==============
version:
	@printf -- "$(CYAN)Python package:$(RESET) $(YELLOW)$(BOLD)$(VERSION)$(RESET)\n"
	@printf -- "$(CYAN)Rust crate:$(RESET)    $(YELLOW)$(BOLD)$(CRATE_VERSION)$(RESET)\n"
	@printf -- "$(CYAN)Rust toolchain:$(RESET) $(YELLOW)$(BOLD)$(RUST_VERSION)$(RESET)\n"

release-info: version
	@printf -- "$(CYAN)SolverForge crates:$(RESET) $(SOLVERFORGE_RELEASE_VERSION)\n"
	@printf -- "$(GRAY)Release distributions are written under $(DIST_DIR) by make build-dist.$(RESET)\n"

clean: clean-py
	@printf -- "$(ARROW) Cleaning Cargo artifacts...\n"
	@cargo clean
	@printf -- "$(GREEN)$(CHECK) Clean complete$(RESET)\n"

clean-dist:
	@printf -- "$(ARROW) Cleaning release distributions...\n"
	@rm -rf "$(DIST_DIR)"

clean-py:
	@printf -- "$(ARROW) Cleaning Python caches...\n"
	@rm -rf .mypy_cache .pytest_cache .ruff_cache .coverage htmlcov

clean-venv:
	@printf -- "$(ARROW) Removing virtualenv $(VENV)...\n"
	@rm -rf "$(VENV)"

# ============== Help ==============
help: banner
	@printf -- "$(BOLD)Environment$(RESET)\n"
	@printf -- "  $(GREEN)make doctor$(RESET)              Check Python, Rust, and Cargo availability\n"
	@printf -- "  $(GREEN)make venv$(RESET)                Create the Python 3.14 virtualenv\n"
	@printf -- "  $(GREEN)make install-python-deps$(RESET) Install runtime and developer Python tools\n\n"
	@printf -- "$(BOLD)Build$(RESET)\n"
	@printf -- "  $(GREEN)make develop$(RESET)             Install the release native extension locally\n"
	@printf -- "  $(GREEN)make develop-debug$(RESET)       Install the debug native extension locally\n"
	@printf -- "  $(GREEN)make build$(RESET)               Run cargo build --locked\n"
	@printf -- "  $(GREEN)make build-wheel$(RESET)         Build a debug Python wheel\n"
	@printf -- "  $(GREEN)make build-sdist$(RESET)         Build a release source distribution\n"
	@printf -- "  $(GREEN)make build-dist$(RESET)          Build release source distribution and local wheel\n"
	@printf -- "  $(GREEN)make build-release$(RESET)       Build a release Python wheel\n"
	@printf -- "  $(GREEN)make check$(RESET)               Run cargo check --locked\n\n"
	@printf -- "$(BOLD)Test$(RESET)\n"
	@printf -- "  $(GREEN)make test$(RESET)                Run Rust and Python tests\n"
	@printf -- "  $(GREEN)make test-quick$(RESET)          Run fast Python regressions without example app tests\n"
	@printf -- "  $(GREEN)make rust-test$(RESET)           Run cargo test --locked\n"
	@printf -- "  $(GREEN)make py-test$(RESET)             Run pytest after release develop install\n"
	@printf -- "  $(GREEN)make test-hospital$(RESET)       Run hospital model and frontend tests\n"
	@printf -- "  $(GREEN)make test-deliveries$(RESET)     Run deliveries model and frontend tests\n"
	@printf -- "  $(GREEN)make test-examples-browser$(RESET) Run both example browser tests\n"
	@printf -- "  $(GREEN)make test-one TEST=pattern$(RESET) Run one selected Python test\n"
	@printf -- "  $(GREEN)make rust-test-one TEST=pattern$(RESET) Run one selected Rust test\n\n"
	@printf -- "$(BOLD)Quality$(RESET)\n"
	@printf -- "  $(GREEN)make fmt$(RESET)                 Format Rust code with rustfmt\n"
	@printf -- "  $(GREEN)make fmt-check$(RESET)           Check Rust formatting with rustfmt\n"
	@printf -- "  $(GREEN)make py-format$(RESET)           Format Python code with Black\n"
	@printf -- "  $(GREEN)make py-format-check$(RESET)     Check Python formatting with Black\n"
	@printf -- "  $(GREEN)make ruff$(RESET)                Run ruff over python, tests, and examples\n"
	@printf -- "  $(GREEN)make typecheck$(RESET)           Run mypy over package and examples\n"
	@printf -- "  $(GREEN)make clippy$(RESET)              Run clippy with warnings denied\n"
	@printf -- "  $(GREEN)make lint$(RESET)                Run rustfmt check, ruff, mypy, and clippy\n"
	@printf -- "  $(GREEN)make pre-commit$(RESET)          Run all pre-commit hooks\n"
	@printf -- "  $(GREEN)make docs-check$(RESET)          Verify tracked documentation surfaces\n"
	@printf -- "  $(GREEN)make ci-local$(RESET)            Simulate the primary local CI gate\n"
	@printf -- "  $(GREEN)make audit$(RESET)               Alias for ci-local\n\n"
	@printf -- "$(BOLD)Release$(RESET)\n"
	@printf -- "  $(GREEN)make release-base-check$(RESET)  Verify exact SolverForge release crate base\n"
	@printf -- "  $(GREEN)make dist-check$(RESET)          Run twine and artifact content checks\n"
	@printf -- "  $(GREEN)make smoke-wheel$(RESET)         Install the local wheel in a clean venv\n"
	@printf -- "  $(GREEN)make pre-release$(RESET)         Run ci-local and release artifact checks\n"
	@printf -- "  $(GREEN)make version$(RESET)             Print package and crate versions\n"
	@printf -- "  $(GREEN)make release-info$(RESET)        Show release artifact information\n\n"
	@printf -- "$(BOLD)Examples$(RESET)\n"
	@printf -- "  $(GREEN)make hospital-run$(RESET)        Serve the hospital app on APP_HOST=$(APP_HOST) PORT=$(PORT)\n"
	@printf -- "  $(GREEN)make hospital-solve$(RESET)      Run the hospital model once in the terminal\n\n"
	@printf -- "  $(GREEN)make deliveries-run$(RESET)      Serve the deliveries app on APP_HOST=$(APP_HOST) PORT=$(PORT)\n"
	@printf -- "  $(GREEN)make deliveries-solve$(RESET)    Run the deliveries model once in the terminal\n\n"
	@printf -- "$(BOLD)Cleanup$(RESET)\n"
	@printf -- "  $(GREEN)make clean$(RESET)               Remove Cargo artifacts and Python caches\n"
	@printf -- "  $(GREEN)make clean-dist$(RESET)          Remove release distributions\n"
	@printf -- "  $(GREEN)make clean-py$(RESET)            Remove Python caches only\n"
	@printf -- "  $(GREEN)make clean-venv$(RESET)          Remove the local virtualenv\n\n"
	@printf -- "$(GRAY)Python 3.14 and Rust $(RUST_VERSION) are required.$(RESET)\n"
