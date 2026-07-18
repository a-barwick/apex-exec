#!/usr/bin/env python3
"""Reject committed whitespace errors across event-aware GitHub ranges."""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import pathlib
import re
import subprocess
import sys
from collections.abc import Sequence

SHA_PATTERN = re.compile(r"^(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})$")
FALLBACK_EVENTS = {"schedule", "workflow_dispatch"}


class WhitespaceCheckError(ValueError):
    """Raised when the event or repository cannot define a safe comparison."""


@dataclasses.dataclass(frozen=True)
class EventRange:
    event_name: str
    base_sha: str | None
    head_sha: str
    use_merge_base: bool


@dataclasses.dataclass(frozen=True)
class DiffCheckResult:
    returncode: int
    effective_base: str
    head: str
    output: str


def _validated_sha(value: object, label: str, *, allow_zero: bool = False) -> str:
    if not isinstance(value, str) or not SHA_PATTERN.fullmatch(value):
        raise WhitespaceCheckError(f"{label} must be a 40- or 64-digit hex object ID")
    normalized = value.lower()
    if not allow_zero and set(normalized) == {"0"}:
        raise WhitespaceCheckError(f"{label} may not be the all-zero object ID")
    return normalized


def _mapping(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        raise WhitespaceCheckError(f"event payload is missing object {label}")
    return value


def select_event_range(
    event_name: str, payload: dict, fallback_head: str | None
) -> EventRange:
    """Select an event's committed comparison without shell expansion."""

    if event_name == "pull_request":
        pull_request = _mapping(payload.get("pull_request"), "pull_request")
        base = _mapping(pull_request.get("base"), "pull_request.base")
        head = _mapping(pull_request.get("head"), "pull_request.head")
        return EventRange(
            event_name=event_name,
            base_sha=_validated_sha(base.get("sha"), "pull_request.base.sha"),
            head_sha=_validated_sha(head.get("sha"), "pull_request.head.sha"),
            use_merge_base=True,
        )

    if event_name == "merge_group":
        merge_group = _mapping(payload.get("merge_group"), "merge_group")
        return EventRange(
            event_name=event_name,
            base_sha=_validated_sha(
                merge_group.get("base_sha"), "merge_group.base_sha"
            ),
            head_sha=_validated_sha(
                merge_group.get("head_sha"), "merge_group.head_sha"
            ),
            use_merge_base=False,
        )

    if event_name == "push":
        return EventRange(
            event_name=event_name,
            base_sha=_validated_sha(
                payload.get("before"), "push.before", allow_zero=True
            ),
            head_sha=_validated_sha(payload.get("after"), "push.after"),
            use_merge_base=False,
        )

    if event_name in FALLBACK_EVENTS:
        return EventRange(
            event_name=event_name,
            base_sha=None,
            head_sha=_validated_sha(fallback_head, "GITHUB_SHA"),
            use_merge_base=False,
        )

    raise WhitespaceCheckError(f"unsupported GitHub event: {event_name!r}")


def _git(
    repository: pathlib.Path,
    *arguments: str,
    check: bool = True,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        ["git", *arguments],
        cwd=repository,
        check=False,
        capture_output=True,
        text=True,
        input=input_text,
    )
    if check and completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise WhitespaceCheckError(
            f"git {' '.join(arguments)} failed with exit "
            f"{completed.returncode}: {detail}"
        )
    return completed


def _resolved_commit(repository: pathlib.Path, sha: str, label: str) -> str:
    completed = _git(repository, "rev-parse", "--verify", f"{sha}^{{commit}}")
    resolved = completed.stdout.strip().lower()
    if not SHA_PATTERN.fullmatch(resolved):
        raise WhitespaceCheckError(f"git returned an invalid object ID for {label}")
    return resolved


def _empty_tree(repository: pathlib.Path) -> str:
    completed = _git(repository, "mktree", input_text="")
    tree = completed.stdout.strip().lower()
    if not SHA_PATTERN.fullmatch(tree):
        raise WhitespaceCheckError("git returned an invalid empty-tree object ID")
    return tree


def _first_parent_or_empty(repository: pathlib.Path, head: str) -> str:
    """Return the ratchet base without re-litigating historical whitespace."""

    completed = _git(repository, "rev-list", "--parents", "-n", "1", head)
    objects = completed.stdout.split()
    if not objects or objects[0].lower() != head:
        raise WhitespaceCheckError("git returned an invalid first-parent record")
    if len(objects) == 1:
        return _empty_tree(repository)
    parent = objects[1].lower()
    if not SHA_PATTERN.fullmatch(parent):
        raise WhitespaceCheckError("git returned an invalid first-parent object ID")
    return parent


def check_committed_whitespace(
    repository: pathlib.Path, event_range: EventRange
) -> DiffCheckResult:
    """Run Git's whitespace checker against the selected committed tree range."""

    repository = repository.resolve()
    head = _resolved_commit(repository, event_range.head_sha, "event head")

    if event_range.base_sha is None or set(event_range.base_sha) == {"0"}:
        effective_base = _first_parent_or_empty(repository, head)
    else:
        base = _resolved_commit(repository, event_range.base_sha, "event base")
        if event_range.use_merge_base:
            completed = _git(repository, "merge-base", base, head)
            effective_base = completed.stdout.strip().lower()
            if not SHA_PATTERN.fullmatch(effective_base):
                raise WhitespaceCheckError("git returned an invalid merge-base object ID")
        else:
            effective_base = base

    completed = _git(
        repository,
        "diff",
        "--check",
        "--no-ext-diff",
        effective_base,
        head,
        "--",
        check=False,
    )
    output = "\n".join(
        part.strip() for part in (completed.stdout, completed.stderr) if part.strip()
    )
    return DiffCheckResult(
        returncode=completed.returncode,
        effective_base=effective_base,
        head=head,
        output=output,
    )


def _load_event(path: pathlib.Path) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise WhitespaceCheckError(f"cannot read GitHub event payload {path}: {error}")
    if not isinstance(payload, dict):
        raise WhitespaceCheckError("GitHub event payload must be a JSON object")
    return payload


def _parser() -> argparse.ArgumentParser:
    repository = pathlib.Path(__file__).parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=pathlib.Path, default=repository)
    parser.add_argument("--event-name", default=os.environ.get("GITHUB_EVENT_NAME"))
    parser.add_argument(
        "--event-path",
        type=pathlib.Path,
        default=os.environ.get("GITHUB_EVENT_PATH"),
    )
    parser.add_argument("--fallback-head", default=os.environ.get("GITHUB_SHA"))
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    if not arguments.event_name:
        print(
            "committed whitespace check error: missing GITHUB_EVENT_NAME",
            file=sys.stderr,
        )
        return 2
    if arguments.event_path is None:
        print("committed whitespace check error: missing GITHUB_EVENT_PATH", file=sys.stderr)
        return 2

    try:
        payload = _load_event(arguments.event_path)
        event_range = select_event_range(
            arguments.event_name, payload, arguments.fallback_head
        )
        result = check_committed_whitespace(arguments.repository, event_range)
    except WhitespaceCheckError as error:
        print(f"committed whitespace check error: {error}", file=sys.stderr)
        return 2

    comparison = f"{result.effective_base}..{result.head}"
    if result.returncode != 0:
        print(
            f"committed whitespace check failed for "
            f"{event_range.event_name} range {comparison}:",
            file=sys.stderr,
        )
        if result.output:
            print(result.output, file=sys.stderr)
        return 1

    print(
        f"Committed whitespace check passed for "
        f"{event_range.event_name} range {comparison}."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
