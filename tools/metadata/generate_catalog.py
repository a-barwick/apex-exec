#!/usr/bin/env python3
"""Generate the checked-in M26 catalog from pinned Salesforce CLI data."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument(
        "--profile-describe",
        action="append",
        default=[],
        metavar="VERSION=PATH",
    )
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def load_profile(value: str) -> tuple[str, list[dict[str, object]]]:
    version, separator, filename = value.partition("=")
    if not separator or not version or not filename:
        raise ValueError(f"invalid profile describe input: {value}")
    document = json.loads(Path(filename).read_text())
    if document.get("status") != 0:
        raise ValueError(f"profile {version} describeMetadata did not succeed")
    objects = document["result"]["metadataObjects"]
    names = {item["xmlName"] for item in objects}
    if len(names) != len(objects):
        raise ValueError(f"profile {version} contains duplicate metadata types")
    return version, objects


def normalize_type(value: dict[str, object]) -> dict[str, object]:
    children = value.get("children", {})
    child_types = children.get("types", {}) if isinstance(children, dict) else {}
    return {
        "name": value["name"],
        "directory": value["directoryName"],
        "suffix": value.get("suffix"),
        "folderType": value.get("folderType"),
        "inFolder": bool(value.get("inFolder", False)),
        "metaFile": bool(value.get("metaFile", False)),
        "bundle": value.get("strategies", {}).get("adapter") == "bundle",
        "mixedContent": value.get("strategies", {}).get("adapter")
        in {"mixedContent", "matchingContentFile"},
        "strictDirectory": bool(value.get("strictDirectoryName", False)),
        "children": [
            {
                "name": child["name"],
                "directory": child["directoryName"],
                "suffix": child.get("suffix"),
                "ignoreParentName": bool(child.get("ignoreParentName", False)),
            }
            for child in sorted(
                child_types.values(), key=lambda item: item["name"].casefold()
            )
        ],
        "sourceSupported": True,
    }


def main() -> None:
    args = parse_args()
    registry = json.loads(args.registry.read_text())
    registry_type_count = len(registry["types"])
    catalog = {
        item["name"]: item
        for item in (normalize_type(value) for value in registry["types"].values())
    }
    described = dict(sorted(load_profile(value) for value in args.profile_describe))
    for objects in described.values():
        for item in objects:
            catalog.setdefault(
                item["xmlName"],
                {
                    "name": item["xmlName"],
                    "directory": item["directoryName"],
                    "suffix": item.get("suffix"),
                    "folderType": None,
                    "inFolder": bool(item.get("inFolder", False)),
                    "metaFile": bool(item.get("metaFile", False)),
                    "bundle": not item.get("suffix") and not item.get("inFolder", False),
                    "mixedContent": False,
                    "strictDirectory": False,
                    "children": [],
                    "sourceSupported": False,
                },
            )
    types = sorted(catalog.values(), key=lambda item: item["name"].casefold())
    profiles = {
        version: sorted(
            (item["xmlName"] for item in objects), key=str.casefold
        )
        for version, objects in described.items()
    }
    output = {
        "schemaVersion": 1,
        "source": {
            "name": "@salesforce/source-deploy-retrieve",
            "version": "12.34.5",
            "registryTypes": registry_type_count,
            "catalogTypes": len(types),
        },
        "types": types,
        "orgProfiles": profiles,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2, sort_keys=False) + "\n")


if __name__ == "__main__":
    main()
