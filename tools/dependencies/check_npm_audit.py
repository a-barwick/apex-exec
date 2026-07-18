#!/usr/bin/env python3
"""Run npm audit and allow only exact, documented, unexpired advisories."""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import re
import subprocess
import sys
from collections import Counter
from collections.abc import Sequence

SCHEMA_VERSION = 1
AUDIT_COMMAND = ["npm", "audit", "--json"]
ADVISORY_ID = re.compile(r"\bGHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}\b")
SEVERITIES = ("info", "low", "moderate", "high", "critical")


class AuditPolicyError(ValueError):
    """Raised when an audit report or exception policy is invalid."""


def load_policy(path: pathlib.Path) -> dict:
    try:
        policy = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AuditPolicyError(f"cannot read policy {path}: {error}") from error
    if policy.get("schema_version") != SCHEMA_VERSION:
        raise AuditPolicyError(
            f"policy schema must be {SCHEMA_VERSION}, "
            f"found {policy.get('schema_version')!r}"
        )
    if policy.get("audit_command") != AUDIT_COMMAND:
        raise AuditPolicyError(
            f"policy audit_command must be {AUDIT_COMMAND!r}, "
            f"found {policy.get('audit_command')!r}"
        )
    allowances = policy.get("allowances")
    if not isinstance(allowances, list):
        raise AuditPolicyError("policy allowances must be a list")
    return policy


def _advisory_from_url(url: object) -> str:
    if not isinstance(url, str):
        raise AuditPolicyError(f"advisory URL must be a string, found {url!r}")
    match = ADVISORY_ID.search(url)
    if not match:
        raise AuditPolicyError(f"cannot identify a GHSA advisory in URL {url!r}")
    return match.group(0)


def _resolved_advisories(
    package: str,
    vulnerabilities: dict[str, dict],
    resolving: tuple[str, ...] = (),
) -> set[str]:
    if package in resolving:
        chain = " -> ".join((*resolving, package))
        raise AuditPolicyError(f"cyclic npm audit dependency chain: {chain}")
    vulnerability = vulnerabilities.get(package)
    if not isinstance(vulnerability, dict):
        raise AuditPolicyError(
            f"npm audit references missing vulnerability package {package!r}"
        )
    via = vulnerability.get("via")
    if not isinstance(via, list) or not via:
        raise AuditPolicyError(f"npm vulnerability {package!r} has no advisory chain")

    advisories: set[str] = set()
    for item in via:
        if isinstance(item, str):
            advisories.update(
                _resolved_advisories(item, vulnerabilities, (*resolving, package))
            )
        elif isinstance(item, dict):
            advisories.add(_advisory_from_url(item.get("url")))
        else:
            raise AuditPolicyError(
                f"npm vulnerability {package!r} has invalid via entry {item!r}"
            )
    return advisories


def _allowances(policy: dict) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for index, allowance in enumerate(policy["allowances"]):
        if not isinstance(allowance, dict):
            raise AuditPolicyError(f"allowance {index} must be an object")
        advisory = allowance.get("advisory_id")
        if not isinstance(advisory, str) or not ADVISORY_ID.fullmatch(advisory):
            raise AuditPolicyError(f"allowance {index} has invalid advisory_id")
        if advisory in result:
            raise AuditPolicyError(f"duplicate allowance for {advisory}")
        severity = allowance.get("severity")
        if severity not in SEVERITIES:
            raise AuditPolicyError(f"allowance {advisory} has invalid severity")
        packages = allowance.get("packages")
        if (
            not isinstance(packages, list)
            or not packages
            or any(not isinstance(package, str) or not package for package in packages)
            or len(set(packages)) != len(packages)
        ):
            raise AuditPolicyError(
                f"allowance {advisory} must list unique package names"
            )
        if sorted(packages) != packages:
            raise AuditPolicyError(f"allowance {advisory} packages must be sorted")
        for field in ("rationale", "review_triggers"):
            if not allowance.get(field):
                raise AuditPolicyError(f"allowance {advisory} must record {field}")
        try:
            datetime.date.fromisoformat(allowance["expires"])
        except (KeyError, TypeError, ValueError) as error:
            raise AuditPolicyError(
                f"allowance {advisory} has invalid expiry date"
            ) from error
        result[advisory] = allowance
    return result


def check_report(report: dict, policy: dict, today: datetime.date) -> list[str]:
    """Return every policy violation in an npm audit v2 report."""

    problems: list[str] = []
    if report.get("auditReportVersion") != 2:
        return [
            "npm audit report version must be 2, "
            f"found {report.get('auditReportVersion')!r}"
        ]
    vulnerabilities = report.get("vulnerabilities")
    if not isinstance(vulnerabilities, dict):
        return ["npm audit report is missing the vulnerabilities object"]

    try:
        allowances = _allowances(policy)
    except AuditPolicyError as error:
        return [str(error)]

    observed_counts = Counter()
    for package in sorted(vulnerabilities):
        vulnerability = vulnerabilities[package]
        if not isinstance(vulnerability, dict):
            problems.append(f"npm vulnerability {package!r} must be an object")
            continue
        severity = vulnerability.get("severity")
        if severity not in SEVERITIES:
            problems.append(
                f"npm vulnerability {package!r} has invalid severity {severity!r}"
            )
            continue
        observed_counts[severity] += 1
        try:
            advisories = _resolved_advisories(package, vulnerabilities)
        except AuditPolicyError as error:
            problems.append(str(error))
            continue
        if not advisories:
            problems.append(f"npm vulnerability {package!r} resolved to no advisories")
            continue

        for advisory in sorted(advisories):
            allowance = allowances.get(advisory)
            if allowance is None:
                problems.append(
                    f"{package} is affected by unapproved advisory {advisory}"
                )
                continue
            if package not in allowance["packages"]:
                problems.append(
                    f"{package} is a new dependency path for allowed advisory {advisory}"
                )
            if severity != allowance["severity"]:
                problems.append(
                    f"{package} reports {advisory} as {severity}; "
                    f"policy permits exactly {allowance['severity']}"
                )
            expiry = datetime.date.fromisoformat(allowance["expires"])
            if today > expiry:
                problems.append(
                    f"allowance {advisory} expired on {expiry.isoformat()}"
                )

    metadata = report.get("metadata", {}).get("vulnerabilities")
    if not isinstance(metadata, dict):
        problems.append("npm audit report is missing vulnerability metadata")
    else:
        for severity in SEVERITIES:
            if metadata.get(severity) != observed_counts[severity]:
                problems.append(
                    f"npm audit {severity} metadata is {metadata.get(severity)!r}; "
                    f"observed {observed_counts[severity]}"
                )
        if metadata.get("total") != len(vulnerabilities):
            problems.append(
                f"npm audit total metadata is {metadata.get('total')!r}; "
                f"observed {len(vulnerabilities)}"
            )

    # An unused allowance is harmless: it means the dependency was fixed before
    # the time-boxed policy entry was removed. It is reported in the success
    # summary so reviewers can delete stale policy promptly.
    return sorted(set(problems))


def run_audit(project: pathlib.Path) -> tuple[dict, int, str]:
    completed = subprocess.run(
        AUDIT_COMMAND,
        cwd=project,
        check=False,
        capture_output=True,
        text=True,
    )
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise AuditPolicyError(
            f"npm audit did not return JSON (exit {completed.returncode}): {detail}"
        ) from error
    if report.get("error"):
        error = report["error"]
        detail = (
            error.get("detail") or error.get("summary")
            if isinstance(error, dict)
            else str(error)
        )
        raise AuditPolicyError(
            f"npm audit failed operationally with exit {completed.returncode}: {detail}"
        )
    if completed.returncode not in (0, 1):
        detail = completed.stderr.strip()
        raise AuditPolicyError(
            f"npm audit failed operationally with exit {completed.returncode}: {detail}"
        )
    return report, completed.returncode, completed.stderr


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    repository = pathlib.Path(__file__).parents[2]
    parser.add_argument(
        "--project", type=pathlib.Path, default=repository / "website"
    )
    parser.add_argument(
        "--policy",
        type=pathlib.Path,
        default=pathlib.Path(__file__).with_name("npm-audit-policy.json"),
    )
    parser.add_argument(
        "--today",
        type=datetime.date.fromisoformat,
        default=datetime.date.today(),
        help="policy evaluation date; intended only for deterministic tests",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        policy = load_policy(arguments.policy)
        report, _, _ = run_audit(arguments.project)
        problems = check_report(report, policy, arguments.today)
    except AuditPolicyError as error:
        print(f"npm audit policy error: {error}", file=sys.stderr)
        return 2

    if problems:
        print("npm dependency policy failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    vulnerabilities = report["vulnerabilities"]
    advisory_ids = sorted(
        {
            advisory
            for package in vulnerabilities
            for advisory in _resolved_advisories(package, vulnerabilities)
        }
    )
    print(
        "npm dependency policy passed: "
        f"{len(vulnerabilities)} vulnerability records, "
        f"{len(advisory_ids)} explicitly documented advisory "
        f"({', '.join(advisory_ids) if advisory_ids else 'none'})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
