# SolverForge Deliveries Python

Python port of the `uc-deliveries` example. It models deliveries as problem facts,
vehicles as route-owning planning entities, and uses SolverForge Python list
variables with CVRP route hooks and shadow-variable refresh callbacks.
The FastAPI app serves shared `/sf/*` frontend assets through the native
`solverforge-ui` bridge and keeps only app-specific browser modules under this
example's `static/` tree.

Run the web app:

```bash
python -m examples.solverforge_deliveries --host 127.0.0.1 --port 7861
```

Run one terminal solve:

```bash
python -m examples.solverforge_deliveries --solve
```
