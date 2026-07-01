# Non-Goals

- No Rust code generation.
- No symbolic expression constraint language.
- No string constraint parser.
- No fixed pre-modeled backends as the main product.
- No hidden dependency on private upstream modules. Public upstream
  SolverForge crates are consumed through exact crates.io pins; only explicitly
  vendored package-local assets may use local paths.
- No upstream SolverForge mutation in this repo.
