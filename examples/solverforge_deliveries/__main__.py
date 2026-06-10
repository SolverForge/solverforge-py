from __future__ import annotations

import argparse

from solverforge import console

from .src.lib import route_summary, solve_demo
from .src.main import serve


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=7861)
    parser.add_argument(
        "--solve",
        action="store_true",
        help="run the delivery model once in the terminal instead of starting the web app",
    )
    args = parser.parse_args(argv)
    if args.solve:
        console.init()
        solved = solve_demo()
        print(f"score={solved.score}")
        for vehicle_name, delivery_count, demand in route_summary(solved):
            print(f"{vehicle_name}: {delivery_count} stops, {demand} units")
        return
    serve(args.host, args.port)


if __name__ == "__main__":
    main()
