import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest

MODULE_PATH = (
    pathlib.Path(__file__).parents[1] / "ci" / "check_committed_whitespace.py"
)
SPEC = importlib.util.spec_from_file_location(
    "check_committed_whitespace", MODULE_PATH
)
assert SPEC is not None and SPEC.loader is not None
check_whitespace = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check_whitespace
SPEC.loader.exec_module(check_whitespace)


class SyntheticRepository:
    def __init__(self, root):
        self.root = root
        self.git("init", "--quiet", "--initial-branch=main")
        self.git("config", "user.email", "ci@example.invalid")
        self.git("config", "user.name", "CI Test")

    def git(self, *arguments):
        return subprocess.run(
            ["git", *arguments],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def commit(self, message):
        self.git("add", "--all")
        self.git("commit", "--quiet", "-m", message)
        return self.git("rev-parse", "HEAD")

    def write(self, relative_path, text):
        path = self.root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


class CommittedWhitespaceTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.repository = SyntheticRepository(
            pathlib.Path(self.temporary_directory.name)
        )
        self.repository.write("source.txt", "baseline\n")
        self.base = self.repository.commit("baseline")

    def check(self, event_range):
        return check_whitespace.check_committed_whitespace(
            self.repository.root, event_range
        )

    def test_clean_pull_request_range_passes(self):
        self.repository.write("source.txt", "baseline\nclean change\n")
        head = self.repository.commit("clean change")
        event_range = check_whitespace.select_event_range(
            "pull_request",
            {
                "pull_request": {
                    "base": {"sha": self.base},
                    "head": {"sha": head},
                }
            },
            fallback_head=None,
        )

        result = self.check(event_range)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.output, "")

    def test_push_checks_the_complete_committed_range(self):
        self.repository.write("source.txt", "baseline\ncommitted defect  \n")
        self.repository.commit("introduce whitespace defect")
        self.repository.write("later.txt", "latest commit is clean\n")
        head = self.repository.commit("clean follow-up")
        event_range = check_whitespace.select_event_range(
            "push",
            {"before": self.base, "after": head},
            fallback_head=None,
        )

        result = self.check(event_range)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source.txt:2: trailing whitespace", result.output)

    def test_merge_group_range_uses_payload_base_and_head(self):
        self.repository.write("source.txt", "baseline\nmerge queue change\n")
        head = self.repository.commit("merge queue change")
        event_range = check_whitespace.select_event_range(
            "merge_group",
            {"merge_group": {"base_sha": self.base, "head_sha": head}},
            fallback_head=None,
        )

        result = self.check(event_range)

        self.assertFalse(event_range.use_merge_base)
        self.assertEqual(result.returncode, 0)

    def test_no_base_fallback_preserves_unchanged_historical_debt(self):
        self.repository.write("source.txt", "historical committed debt  \n")
        self.repository.commit("historical whitespace debt")
        self.repository.write("later.txt", "latest commit is clean\n")
        head = self.repository.commit("clean latest commit")

        schedule_range = check_whitespace.select_event_range(
            "schedule", {}, fallback_head=head
        )
        new_branch_range = check_whitespace.select_event_range(
            "push",
            {"before": "0" * 40, "after": head},
            fallback_head=None,
        )

        self.assertEqual(self.check(schedule_range).returncode, 0)
        self.assertEqual(self.check(new_branch_range).returncode, 0)

    def test_no_base_fallback_rejects_whitespace_introduced_by_head(self):
        self.repository.write("source.txt", "committed defect  \n")
        head = self.repository.commit("introduce whitespace at head")
        schedule_range = check_whitespace.select_event_range(
            "workflow_dispatch", {}, fallback_head=head
        )
        new_branch_range = check_whitespace.select_event_range(
            "push",
            {"before": "0" * 40, "after": head},
            fallback_head=None,
        )

        schedule_result = self.check(schedule_range)
        new_branch_result = self.check(new_branch_range)

        self.assertNotEqual(schedule_result.returncode, 0)
        self.assertIn("source.txt:1: trailing whitespace", schedule_result.output)
        self.assertNotEqual(new_branch_result.returncode, 0)
        self.assertIn("source.txt:1: trailing whitespace", new_branch_result.output)

    def test_event_object_ids_are_validated_before_git_use(self):
        with self.assertRaises(check_whitespace.WhitespaceCheckError):
            check_whitespace.select_event_range(
                "push",
                {"before": self.base, "after": "$(touch unsafe)"},
                fallback_head=None,
            )


if __name__ == "__main__":
    unittest.main()
