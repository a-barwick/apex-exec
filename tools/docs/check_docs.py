#!/usr/bin/env python3
"""Validate repository Markdown formatting and local links without network I/O."""

from __future__ import annotations

import argparse
import dataclasses
import pathlib
import re
import sys
import urllib.parse
from collections.abc import Iterable, Sequence

EXCLUDED_DIRECTORIES = {
    ".git",
    ".next",
    ".wrangler",
    "dist",
    "node_modules",
    "target",
}
INLINE_LINK = re.compile(
    r"!?\[[^\]]*\]\((?P<target><[^>]+>|[^)\s]+)"
    r"(?:\s+(?:\"[^\"]*\"|'[^']*'))?\)"
)
REFERENCE_LINK = re.compile(
    r"^\s*\[[^\]]+\]:\s*(?P<target><[^>]+>|\S+)", re.MULTILINE
)
INLINE_CODE = re.compile(r"`[^`\n]*`")
HEADING = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
HTML_TAG = re.compile(r"<[^>]+>")
PUNCTUATION = re.compile(r"[^\w\s-]", re.UNICODE)


@dataclasses.dataclass(frozen=True)
class CheckResult:
    documents: int
    local_links: int
    problems: list[str]


def discover_markdown(root: pathlib.Path) -> list[pathlib.Path]:
    return sorted(
        path
        for path in root.rglob("*.md")
        if not any(part in EXCLUDED_DIRECTORIES for part in path.relative_to(root).parts)
        and not _inside_nested_repository(path, root)
    )


def _inside_nested_repository(path: pathlib.Path, root: pathlib.Path) -> bool:
    for parent in path.parents:
        if parent == root:
            return False
        if (parent / ".git").exists():
            return True
    return False


def _github_anchors(text: str) -> set[str]:
    anchors: set[str] = set()
    duplicates: dict[str, int] = {}
    in_fence: str | None = None
    for line in text.splitlines():
        stripped = line.lstrip()
        fence = stripped[:3]
        if fence in {"```", "~~~"}:
            if in_fence is None:
                in_fence = fence
            elif fence == in_fence:
                in_fence = None
            continue
        if in_fence is not None:
            continue

        match = HEADING.match(line)
        if not match:
            continue
        title = INLINE_CODE.sub(lambda item: item.group(0).strip("`"), match.group(2))
        title = HTML_TAG.sub("", title).strip().lower()
        anchor = PUNCTUATION.sub("", title)
        anchor = re.sub(r"\s+", "-", anchor)
        anchor = re.sub(r"-+", "-", anchor).strip("-")
        count = duplicates.get(anchor, 0)
        duplicates[anchor] = count + 1
        anchors.add(anchor if count == 0 else f"{anchor}-{count}")
    return anchors


def _visible_markdown(text: str) -> str:
    result: list[str] = []
    in_fence: str | None = None
    for line in text.splitlines():
        stripped = line.lstrip()
        fence = stripped[:3]
        if fence in {"```", "~~~"}:
            if in_fence is None:
                in_fence = fence
            elif fence == in_fence:
                in_fence = None
            result.append("")
            continue
        result.append("" if in_fence is not None else INLINE_CODE.sub("", line))
    return "\n".join(result)


def _formatting_problems(path: pathlib.Path, text: str) -> list[str]:
    problems: list[str] = []
    relative = path.as_posix()
    if not text:
        return [f"{relative}: document is empty"]
    if not text.endswith("\n"):
        problems.append(f"{relative}: missing final newline")
    if "\r" in text:
        problems.append(f"{relative}: carriage-return line endings are not allowed")

    lines = text.splitlines()
    for number, line in enumerate(lines, start=1):
        trailing = line[len(line.rstrip(" \t")) :]
        if trailing and trailing != "  ":
            problems.append(f"{relative}:{number}: trailing whitespace")
        if "\t" in line:
            problems.append(f"{relative}:{number}: tab character; use spaces")

    visible_lines = _visible_markdown(text).splitlines()
    first_content = next((line for line in visible_lines if line.strip()), "")
    if not first_content.startswith("# "):
        problems.append(f"{relative}: first content must be one level-1 heading")
    if sum(line.startswith("# ") for line in visible_lines) != 1:
        problems.append(f"{relative}: expected exactly one level-1 heading")

    open_fence: tuple[str, int] | None = None
    for number, line in enumerate(lines, start=1):
        fence = line.lstrip()[:3]
        if fence not in {"```", "~~~"}:
            continue
        if open_fence is None:
            open_fence = (fence, number)
        elif fence == open_fence[0]:
            open_fence = None
    if open_fence is not None:
        problems.append(
            f"{relative}:{open_fence[1]}: unclosed {open_fence[0]} code fence"
        )
    return problems


def _targets(text: str) -> Iterable[tuple[str, int]]:
    visible = _visible_markdown(text)
    for pattern in (INLINE_LINK, REFERENCE_LINK):
        for match in pattern.finditer(visible):
            line = visible.count("\n", 0, match.start()) + 1
            yield match.group("target"), line


def _link_problem(
    root: pathlib.Path,
    document: pathlib.Path,
    target: str,
    line: int,
    anchor_cache: dict[pathlib.Path, set[str]],
) -> str | None:
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    target = target.replace("\\", "")
    parsed = urllib.parse.urlsplit(target)
    if parsed.scheme or parsed.netloc or target.startswith("//"):
        return None

    decoded_path = urllib.parse.unquote(parsed.path)
    if decoded_path:
        candidate = (
            root / decoded_path.lstrip("/")
            if decoded_path.startswith("/")
            else document.parent / decoded_path
        )
        resolved = candidate.resolve()
    else:
        resolved = document.resolve()

    relative_document = document.relative_to(root).as_posix()
    try:
        resolved.relative_to(root.resolve())
    except ValueError:
        return f"{relative_document}:{line}: local link escapes repository: {target}"
    if not resolved.exists():
        return f"{relative_document}:{line}: missing local link target: {target}"

    fragment = urllib.parse.unquote(parsed.fragment)
    if fragment and resolved.is_file() and resolved.suffix.lower() == ".md":
        anchors = anchor_cache.get(resolved)
        if anchors is None:
            anchors = _github_anchors(resolved.read_text(encoding="utf-8"))
            anchor_cache[resolved] = anchors
        if fragment not in anchors:
            return (
                f"{relative_document}:{line}: missing Markdown anchor "
                f"#{fragment} in {resolved.relative_to(root).as_posix()}"
            )
    return None


def validate_documents(root: pathlib.Path) -> CheckResult:
    root = root.resolve()
    documents = discover_markdown(root)
    problems: list[str] = []
    local_links = 0
    anchor_cache: dict[pathlib.Path, set[str]] = {}

    for document in documents:
        relative = document.relative_to(root)
        try:
            text = document.read_text(encoding="utf-8")
        except UnicodeDecodeError as error:
            problems.append(f"{relative.as_posix()}: invalid UTF-8: {error}")
            continue
        problems.extend(_formatting_problems(relative, text))
        for target, line in _targets(text):
            parsed = urllib.parse.urlsplit(target.strip("<>"))
            if not parsed.scheme and not parsed.netloc and not target.startswith("//"):
                local_links += 1
            problem = _link_problem(root, document, target, line, anchor_cache)
            if problem:
                problems.append(problem)

    return CheckResult(len(documents), local_links, sorted(problems))


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=pathlib.Path(__file__).parents[2],
        help="repository root (defaults to the script's repository)",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    result = validate_documents(arguments.root)
    if result.problems:
        print("Documentation validation failed:", file=sys.stderr)
        for problem in result.problems:
            print(f"- {problem}", file=sys.stderr)
        return 1
    print(
        "Documentation validation passed: "
        f"{result.documents} Markdown files, "
        f"{result.local_links} local links."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
