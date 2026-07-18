#!/usr/bin/env python3
"""Compare Lizard CSV metrics with Apex Exec's committed debt baseline."""

from __future__ import annotations

import argparse
import csv
import dataclasses
import io
import json
import pathlib
import sys
from collections import defaultdict
from collections.abc import Iterable, Sequence
from typing import TextIO

SCHEMA_VERSION = 1
TOOL_NAME = "lizard"
TOOL_VERSION = "1.21.2"
LANGUAGE = "rust"
ROOTS = ["src"]
MAX_NLOC = 80
MAX_CCN = 15
THRESHOLD_COMPARISON = "strictly-greater-than"
LIZARD_OPTIONS = [
    "src",
    "--languages",
    "rust",
    "--CCN",
    "15",
    "--Threshold",
    "nloc=80",
    "--csv",
    "--ignore_warnings",
    "-1",
]


@dataclasses.dataclass(frozen=True, order=True)
class FunctionKey:
    path: str
    name: str
    signature: str
    occurrence: int


@dataclasses.dataclass(frozen=True)
class FunctionMetric:
    key: FunctionKey
    start_line: int
    end_line: int
    nloc: int
    cyclomatic_complexity: int
    token_count: int


class RatchetError(ValueError):
    """Raised when Lizard output or baseline data is invalid."""


def _integer(value: str, field: str, row_number: int) -> int:
    try:
        result = int(value)
    except ValueError as error:
        raise RatchetError(
            f"Lizard CSV row {row_number} has invalid {field}: {value!r}"
        ) from error
    if result < 0:
        raise RatchetError(
            f"Lizard CSV row {row_number} has negative {field}: {result}"
        )
    return result


def _normalized_path(value: str, row_number: int) -> str:
    normalized = pathlib.PurePosixPath(value.replace("\\", "/")).as_posix()
    if normalized.startswith("/") or normalized == ".." or normalized.startswith("../"):
        raise RatchetError(
            f"Lizard CSV row {row_number} has non-repository path: {value!r}"
        )
    if not any(normalized == root or normalized.startswith(f"{root}/") for root in ROOTS):
        raise RatchetError(
            f"Lizard CSV row {row_number} is outside configured roots: {value!r}"
        )
    return normalized


def parse_lizard_csv(stream: TextIO) -> list[FunctionMetric]:
    """Parse Lizard's headerless 11-column CSV and assign stable occurrences."""

    grouped: dict[
        tuple[str, str, str], list[tuple[int, int, int, int, int]]
    ] = defaultdict(list)
    reader = csv.reader(stream)
    for row_number, row in enumerate(reader, start=1):
        if len(row) != 11:
            raise RatchetError(
                f"Lizard CSV row {row_number} has {len(row)} columns; expected 11"
            )

        nloc = _integer(row[0], "NLOC", row_number)
        ccn = _integer(row[1], "CCN", row_number)
        token_count = _integer(row[2], "token count", row_number)
        path = _normalized_path(row[6], row_number)
        name = row[7].strip()
        signature = " ".join(row[8].split())
        start_line = _integer(row[9], "start line", row_number)
        end_line = _integer(row[10], "end line", row_number)
        if not name:
            raise RatchetError(f"Lizard CSV row {row_number} has an empty function name")
        if start_line == 0 or end_line < start_line:
            raise RatchetError(
                f"Lizard CSV row {row_number} has invalid line range "
                f"{start_line}-{end_line}"
            )
        grouped[(path, name, signature)].append(
            (start_line, end_line, nloc, ccn, token_count)
        )

    if not grouped:
        raise RatchetError("Lizard CSV contained no Rust functions")

    metrics: list[FunctionMetric] = []
    for (path, name, signature), instances in sorted(grouped.items()):
        for occurrence, values in enumerate(sorted(instances), start=1):
            start_line, end_line, nloc, ccn, token_count = values
            metrics.append(
                FunctionMetric(
                    key=FunctionKey(path, name, signature, occurrence),
                    start_line=start_line,
                    end_line=end_line,
                    nloc=nloc,
                    cyclomatic_complexity=ccn,
                    token_count=token_count,
                )
            )
    return metrics


def exceeds_threshold(metric: FunctionMetric) -> bool:
    return metric.nloc > MAX_NLOC or metric.cyclomatic_complexity > MAX_CCN


def _expected_metadata() -> dict[str, object]:
    return {
        "schema_version": SCHEMA_VERSION,
        "tool": {"name": TOOL_NAME, "version": TOOL_VERSION},
        "language": LANGUAGE,
        "roots": ROOTS,
        "thresholds": {
            "nloc": MAX_NLOC,
            "cyclomatic_complexity": MAX_CCN,
            "comparison": THRESHOLD_COMPARISON,
        },
        "lizard_options": LIZARD_OPTIONS,
        "identity": "path + function name + normalized signature + occurrence",
    }


def render_baseline(
    metrics: Iterable[FunctionMetric], source_revision: str, captured_on: str
) -> str:
    records = []
    for metric in sorted(filter(exceeds_threshold, metrics), key=lambda item: item.key):
        records.append(
            {
                "path": metric.key.path,
                "name": metric.key.name,
                "signature": metric.key.signature,
                "occurrence": metric.key.occurrence,
                "start_line": metric.start_line,
                "end_line": metric.end_line,
                "nloc": metric.nloc,
                "cyclomatic_complexity": metric.cyclomatic_complexity,
                "token_count": metric.token_count,
            }
        )

    baseline = _expected_metadata()
    baseline.update(
        {
            "source_revision": source_revision,
            "captured_on": captured_on,
            "comparison_policy": (
                "Existing debt may decrease but neither NLOC nor cyclomatic "
                "complexity may exceed its recorded cap. A function not in this "
                "baseline may not cross either threshold."
            ),
            "functions": records,
        }
    )
    return json.dumps(baseline, indent=2, sort_keys=False) + "\n"


def load_baseline(path: pathlib.Path) -> tuple[dict[FunctionKey, FunctionMetric], dict]:
    try:
        baseline = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RatchetError(f"cannot read baseline {path}: {error}") from error

    for field, expected in _expected_metadata().items():
        if baseline.get(field) != expected:
            raise RatchetError(
                f"baseline field {field!r} is {baseline.get(field)!r}; "
                f"expected {expected!r}"
            )

    if not baseline.get("source_revision") or not baseline.get("captured_on"):
        raise RatchetError("baseline must record source_revision and captured_on")

    records = baseline.get("functions")
    if not isinstance(records, list):
        raise RatchetError("baseline functions must be a list")

    result: dict[FunctionKey, FunctionMetric] = {}
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            raise RatchetError(f"baseline function {index} must be an object")
        try:
            key = FunctionKey(
                path=record["path"],
                name=record["name"],
                signature=record["signature"],
                occurrence=record["occurrence"],
            )
            metric = FunctionMetric(
                key=key,
                start_line=record["start_line"],
                end_line=record["end_line"],
                nloc=record["nloc"],
                cyclomatic_complexity=record["cyclomatic_complexity"],
                token_count=record["token_count"],
            )
        except (KeyError, TypeError) as error:
            raise RatchetError(f"invalid baseline function {index}: {error}") from error
        if key in result:
            raise RatchetError(f"duplicate baseline function identity: {key}")
        if not exceeds_threshold(metric):
            raise RatchetError(
                f"baseline function is not over either threshold: {key.path}:{key.name}"
            )
        result[key] = metric

    if len(result) != len(records):
        raise RatchetError("baseline contains duplicate function identities")
    return result, baseline


def compare_metrics(
    current: Iterable[FunctionMetric],
    baseline: dict[FunctionKey, FunctionMetric],
) -> list[str]:
    problems: list[str] = []
    for metric in sorted(current, key=lambda item: item.key):
        cap = baseline.get(metric.key)
        location = f"{metric.key.path}:{metric.start_line} {metric.key.name}"
        if cap is None:
            if exceeds_threshold(metric):
                problems.append(
                    f"{location} is new maintainability debt "
                    f"(NLOC {metric.nloc}>{MAX_NLOC} or "
                    f"CCN {metric.cyclomatic_complexity}>{MAX_CCN})"
                )
            continue

        increases = []
        if metric.nloc > cap.nloc:
            increases.append(f"NLOC {cap.nloc}->{metric.nloc}")
        if metric.cyclomatic_complexity > cap.cyclomatic_complexity:
            increases.append(
                "CCN "
                f"{cap.cyclomatic_complexity}->{metric.cyclomatic_complexity}"
            )
        if increases:
            problems.append(f"{location} exceeds its debt cap ({', '.join(increases)})")
    return problems


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)

    check = subcommands.add_parser("check", help="reject complexity regressions")
    check.add_argument("--baseline", type=pathlib.Path, required=True)

    render = subcommands.add_parser(
        "render-baseline", help="render a reviewed baseline to standard output"
    )
    render.add_argument("--source-revision", required=True)
    render.add_argument("--captured-on", required=True)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        metrics = parse_lizard_csv(sys.stdin)
        if arguments.command == "render-baseline":
            sys.stdout.write(
                render_baseline(
                    metrics, arguments.source_revision, arguments.captured_on
                )
            )
            return 0

        baseline, metadata = load_baseline(arguments.baseline)
        problems = compare_metrics(metrics, baseline)
    except RatchetError as error:
        print(f"complexity ratchet configuration error: {error}", file=sys.stderr)
        return 2

    if problems:
        print("Lizard complexity ratchet failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    current_debt = sum(exceeds_threshold(metric) for metric in metrics)
    print(
        "Lizard complexity ratchet passed: "
        f"{current_debt} current threshold violations, "
        f"{len(baseline)} recorded debt caps, "
        f"baseline {metadata['source_revision']}."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
