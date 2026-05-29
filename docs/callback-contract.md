# Callback Contract

Python callbacks are the only constraint authoring surface.

- A filter callback returns `bool`.
- A weight callback returns a score object or an integer.
- A grouped scalar callback returns scalar edit candidates for the declared
  `@scalar_group`.
- A conflict repair callback returns scalar edit candidates for constraints
  declared with `@conflict_repair`.
- Value range providers return ordered finite collections.
- Distance callbacks return numeric distances.
- Owner hooks used by list selectors return an owner index or `None`.
- Callback exceptions are surfaced as SolverForge Python exceptions with the
  original Python traceback.

Callbacks must be deterministic for a given solution state. The solver may call
callbacks many times and from multiple worker threads on free-threaded Python.
