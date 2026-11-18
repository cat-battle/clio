"""Command line entry point: ``clio-viewer --file tasks.json``."""

from __future__ import annotations

import argparse
import sys

from .adapters import FileLoader, JiraLoader, Loader
from .keys import parse_keys
from .model import Graph, ValidationError
from .state import ViewerState


def _parse_size(spec: str) -> tuple[int, int]:
    w, sep, h = spec.lower().partition("x")
    if not sep:
        raise argparse.ArgumentTypeError(f"expected WIDTHxHEIGHT, got {spec!r}")
    return int(w), int(h)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="clio-viewer",
        description="Schematic-style TUI for Clio task hierarchies (Milestone → Epic → Story → Subtask).",
    )
    src = p.add_mutually_exclusive_group(required=True)
    src.add_argument("--file", metavar="PATH", help="load a Clio task document (JSON, see viewer/schema)")
    src.add_argument("--jira", action="store_true", help="load from Jira Cloud (JIRA_URL, JIRA_EMAIL, JIRA_API_TOKEN)")
    p.add_argument("--jql", metavar="QUERY", help="JQL query selecting the issues to show (with --jira)")
    p.add_argument("--snapshot", metavar="WxH", type=_parse_size, help="print one WIDTHxHEIGHT frame instead of running interactively")
    p.add_argument("--keys", metavar="SCRIPT", help='keys to replay before the snapshot, e.g. "<space>j<enter>"')
    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.jira and not args.jql:
        parser.error("--jira requires --jql")
    if args.keys and not args.snapshot:
        parser.error("--keys requires --snapshot")
    try:
        loader: Loader = JiraLoader.from_env(args.jql) if args.jira else FileLoader(args.file)
        graph = Graph(loader.load())
    except ValidationError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    state = ViewerState(graph, loader)

    if args.snapshot:
        from .views import snapshot

        try:
            keys = parse_keys(args.keys or "")
            sys.stdout.write(snapshot(state, *args.snapshot, keys))
        except ValueError as e:
            print(f"error: {e}", file=sys.stderr)
            return 1
        return 0

    from .app import ClioViewer

    ClioViewer(state).run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
