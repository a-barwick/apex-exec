import datetime
import importlib.util
import pathlib
import sys
import unittest

MODULE_PATH = (
    pathlib.Path(__file__).parents[1] / "dependencies" / "check_npm_audit.py"
)
SPEC = importlib.util.spec_from_file_location("check_npm_audit", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
check_npm_audit = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check_npm_audit
SPEC.loader.exec_module(check_npm_audit)


def report():
    return {
        "auditReportVersion": 2,
        "vulnerabilities": {
            "framework": {
                "severity": "moderate",
                "via": ["processor"],
            },
            "processor": {
                "severity": "moderate",
                "via": [
                    {
                        "url": (
                            "https://github.com/advisories/"
                            "GHSA-aaaa-bbbb-cccc"
                        )
                    }
                ],
            },
        },
        "metadata": {
            "vulnerabilities": {
                "info": 0,
                "low": 0,
                "moderate": 2,
                "high": 0,
                "critical": 0,
                "total": 2,
            }
        },
    }


def policy(expires="2026-08-18"):
    return {
        "schema_version": 1,
        "audit_command": ["npm", "audit", "--json"],
        "allowances": [
            {
                "advisory_id": "GHSA-aaaa-bbbb-cccc",
                "severity": "moderate",
                "packages": ["framework", "processor"],
                "expires": expires,
                "rationale": "No compatible fix.",
                "review_triggers": ["Dependency update"],
            }
        ],
    }


class NpmAuditPolicyTests(unittest.TestCase):
    def test_exact_unexpired_advisory_chain_is_allowed(self):
        problems = check_npm_audit.check_report(
            report(), policy(), datetime.date(2026, 7, 18)
        )

        self.assertEqual(problems, [])

    def test_new_advisory_is_rejected(self):
        changed = report()
        changed["vulnerabilities"]["processor"]["via"][0]["url"] = (
            "https://github.com/advisories/GHSA-dddd-eeee-ffff"
        )

        problems = check_npm_audit.check_report(
            changed, policy(), datetime.date(2026, 7, 18)
        )

        self.assertTrue(any("unapproved advisory" in item for item in problems))

    def test_new_dependency_path_is_rejected(self):
        changed = report()
        changed["vulnerabilities"]["wrapper"] = {
            "severity": "moderate",
            "via": ["framework"],
        }
        changed["metadata"]["vulnerabilities"]["moderate"] = 3
        changed["metadata"]["vulnerabilities"]["total"] = 3

        problems = check_npm_audit.check_report(
            changed, policy(), datetime.date(2026, 7, 18)
        )

        self.assertTrue(any("new dependency path" in item for item in problems))

    def test_expired_allowance_is_rejected(self):
        problems = check_npm_audit.check_report(
            report(), policy(expires="2026-07-17"), datetime.date(2026, 7, 18)
        )

        self.assertTrue(any("expired" in item for item in problems))


if __name__ == "__main__":
    unittest.main()
