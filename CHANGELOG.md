# Changelog

All notable changes to this project will be documented in this file.
## [0.7.2](https://github.com/SolverForge/solverforge-py/compare/v0.7.1...v0.7.2) (2026-10-28)


### Bug Fixes

* **callbacks:** report callback errors once through the binding's error path ([d0ecd29](https://github.com/SolverForge/solverforge-py/commit/d0ecd29d3172e60202a86f93db4416fca683e087))


## [0.7.1](https://github.com/SolverForge/solverforge-py/compare/v0.7.0...v0.7.1) (2026-10-28)


### Bug Fixes

* **callbacks:** propagate interrupts raised while syncing callback views ([e1d1134](https://github.com/SolverForge/solverforge-py/commit/e1d11348ce5e377ed932f6b054a081fb1dfcba2e))
* **runtime:** stop swallowing errors from optional attribute lookups ([0917e6a](https://github.com/SolverForge/solverforge-py/commit/0917e6ab49e77a26c122a0c064124820ac37272b))


## [0.7.0](https://github.com/SolverForge/solverforge-py/compare/v0.6.15...v0.7.0) (2026-10-27)


### ⚠ BREAKING CHANGES

* **score:** migrate to the plain three-level DynamicScore
* **deps:** retained telemetry replaces `phase` with a `phases` list; read `phases[-1]`.

### Features

* **manager:** report speculative evaluation from parallel local search ([3930b06](https://github.com/SolverForge/solverforge-py/commit/3930b060cc48dfaf6a3d781df0768ebe79fd144a))
* **solver:** evaluate Python models on several cores ([5ff3a20](https://github.com/SolverForge/solverforge-py/commit/5ff3a20f9526b48bfda4f6b06311162cb934a651))


### Bug Fixes

* **ci:** serialize distribution build and validation ([df38c85](https://github.com/SolverForge/solverforge-py/commit/df38c853b99aeea9e276d81f0773f5b5a7cf2f39))
* **deps:** target the SolverForge 0.23.0 registry base ([afdf0fa](https://github.com/SolverForge/solverforge-py/commit/afdf0fa0ec21eeac89098689605c93a76e387db2))
* **solver:** release the GIL for the duration of a solve ([4c6dddb](https://github.com/SolverForge/solverforge-py/commit/4c6dddb2172a355897d4666ba39e5eb386a99a44))


### Tests

* pin parallel evaluation parity and worker-thread tracebacks ([056867f](https://github.com/SolverForge/solverforge-py/commit/056867fd63e9ce5f049562626f329de35d3f99ce))
* **threading:** pin parallel evaluation across score families and list models ([77a05ab](https://github.com/SolverForge/solverforge-py/commit/77a05abc550f9e12337dbefaace93f383d6f09c7))
* **threading:** pin per-phase speculative telemetry and TOML thread counts ([d021ae4](https://github.com/SolverForge/solverforge-py/commit/d021ae423171c5c8d72bcbd8d65c70c0bb4568fa))


### Documentation and release

* document parallel move evaluation ([5ff3764](https://github.com/SolverForge/solverforge-py/commit/5ff376458139fed91245892bee3bbc209dbd4f57))


## [0.6.15](https://github.com/SolverForge/solverforge-py/compare/v0.6.14...v0.6.15) (2026-10-18)


### Bug Fixes

* **deps:** target the SolverForge 0.22.0 registry base ([d441dff](https://github.com/SolverForge/solverforge-py/commit/d441dff836a2035a767306db744662e150b64f15))


## [0.6.14](https://github.com/SolverForge/solverforge-py/compare/v0.6.13...v0.6.14) (2026-10-15)


### Bug Fixes

* **constraints:** dispatch list precedence outside the generic Python path ([4061a38](https://github.com/SolverForge/solverforge-py/commit/4061a38fcc2996c29a6914fdbd04b050cc29ce2a))


## [0.6.13](https://github.com/SolverForge/solverforge-py/compare/v0.6.12...v0.6.13) (2026-10-14)


### Bug Fixes

* **runtime:** resolve assignment metadata fields once at import ([0af0ab3](https://github.com/SolverForge/solverforge-py/commit/0af0ab344702592d12ee6237557b0131952fb08f))


## [0.6.12](https://github.com/SolverForge/solverforge-py/compare/v0.6.11...v0.6.12) (2026-10-14)


### Bug Fixes

* **deps:** target the SolverForge 0.21.2 registry base ([0d50553](https://github.com/SolverForge/solverforge-py/commit/0d505534be99ba3d226def9a14653a0ae3d2cc7b))


## [0.6.11](https://github.com/SolverForge/solverforge-py/compare/v0.6.10...v0.6.11) (2026-10-14)


### Bug Fixes

* **deps:** target the SolverForge 0.21.0 registry base ([33dc856](https://github.com/SolverForge/solverforge-py/commit/33dc8565c6070bfda68d0d7d62148de5624ec95e))


## [0.6.10](https://github.com/SolverForge/solverforge-py/compare/v0.6.9...v0.6.10) (2026-09-29)

### Bug Fixes

* **ci:** enforce the Python formatter check in lint and the local CI 1bd7f8c

## [0.6.9](https://github.com/SolverForge/solverforge-py/compare/v0.6.8...v0.6.9) (2026-09-29)

### Features

* **model:** pin planning entities with planning_pin() ba34dbf

### Bug Fixes

* **marshal:** keep the cause when a pin attribute cannot be read 47d0eb0
* **schema:** reject a pin name that collides with a variable 2a38af7

### Tests

* **descriptor:** initialize the pinned row in its initializer a63e183
* **pinning:** strengthen the controls the review found weak 5330f1a

### Documentation and release

* **pinning:** correct the claims the review found wrong 860844c
* **pinning:** document the entity pin contract and add an example 19ddb53
* **pinning:** record the version the release tooling computes d98e8f4

## [0.6.8](https://github.com/SolverForge/solverforge-py/compare/v0.6.7...v0.6.8) (2026-09-29)

### Tests

* **assets:** derive the synthetic bundle guard from the package version 2a06fd5
* **release:** guard the declared release version surfaces 097fc25

### Documentation and release

* **release:** document the release commit and tag tooling 5af585b
* **release:** repair the changelog compare links 3d0e17e

## [0.6.7](https://github.com/SolverForge/solverforge-py/compare/v0.6.6...v0.6.7) (2026-09-29)


### Bug Fixes

* **deps:** consume the SolverForge 0.19.7 registry release 35b1483
* **schema:** reject the unsupported pinning flag d201a51
* **ui:** serve the solverforge-ui 0.9.0 asset bundle b3d52d7

## [0.6.6](https://github.com/SolverForge/solverforge-py/compare/v0.6.5...v0.6.6) (2026-08-11)

### Bug Fixes

* **scalar:** enforce native assignment hard domains 128a864
## [0.6.5](https://github.com/SolverForge/solverforge-py/compare/v0.6.4...v0.6.5) (2026-07-29)


### Bug Fixes

* **ci:** pin Ruff to the lint contract a639112
* **deps:** consume SolverForge 0.19.3 rotation repair 544c31b

## [0.6.4](https://github.com/SolverForge/solverforge-py/compare/v0.6.3...v0.6.4) (2026-07-26)


### Bug Fixes

* **deps:** consume SolverForge 0.19.2 construction repair 52bbcdd

## [0.6.3](https://github.com/SolverForge/solverforge-py/compare/v0.6.2...v0.6.3) (2026-07-18)


### Bug Fixes

* **runtime:** enforce mandatory completion at limits cc1262b

## [0.6.2](https://github.com/SolverForge/solverforge-py/compare/v0.6.1...v0.6.2) (2026-07-17)

## [0.6.1](https://github.com/SolverForge/solverforge-py/compare/v0.6.0...v0.6.1) (2026-07-13)


### Bug Fixes

* **package:** restrict source archives to build inputs 615f291

## [0.6.0](https://github.com/SolverForge/solverforge-py/compare/v0.5.0...v0.6.0) (2026-07-13)


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
