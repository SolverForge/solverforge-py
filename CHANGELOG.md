# Changelog

All notable changes to this project will be documented in this file. See [commit-and-tag-version](https://github.com/absolute-version/commit-and-tag-version) for commit guidelines.

## [0.6.0](///compare/v0.5.0...v0.6.0) (2026-07-13)


### Features

* **manager:** retain qualified solve diagnostics c6445ec
* **model:** declare explicit runtime metadata ad41f94

## [0.5.0](https://github.com/SolverForge/solverforge-py/compare/v0.4.1...v0.5.0) (2026-07-03)


### Features

* **examples:** add deliveries Python app 0179084
* **model:** carry dynamic route and shadow hooks 83a8d41
* **runtime:** bind Python list route hooks fd51788
* **runtime:** broaden dynamic Python planning surface f04eefa
* **ui:** embed shared frontend assets ab30697


### Bug Fixes

* **ci:** bootstrap Forgejo toolchains in workflow 8e5f791
* **ci:** install Playwright system dependencies b249fd1
* **ci:** retry toolchain bootstrap downloads cd60f76
* **ci:** run workflow from checkout root 8733fc3
* **ci:** select Forgejo runner labels 56ff4fc
* **deliveries:** recommend assigned-route insertions d59daea
* **examples:** report snapshot score in payloads 0bfcaa0
* **examples:** report solution scores in events bdcddd8
* **model:** validate scalar group names cfa2070
* **runtime:** align dynamic construction with core semantics 7cacda0
* **runtime:** cap first-fit required construction candidates 3666c20
* **runtime:** resolve dynamic slots against descriptors a2544be
* **runtime:** reuse required assignment construction streams 0caf06d
* **runtime:** use direct cursor for first-fit assignments 769a081
* **scoring:** return true shadow update deltas 88dc500
* **state:** preserve callback root fields 0aacef5


### Tests

* **examples:** stabilize hospital browser smoke f11efd6
* **examples:** stub map tiles in browser smoke d69ca3e


### Documentation and release

* **docs:** remove standalone docs surface b1c6e1f
* **release:** cut 0.5.0 metadata 4728014

## [0.4.1](https://github.com/SolverForge/solverforge-py/compare/v0.4.0...v0.4.1) (2026-06-02)


### Bug Fixes

* **deps:** resolve SolverForge crates from crates.io 73bfe4a
