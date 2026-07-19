import importlib.util
import pathlib
import sys
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).parents[1] / "docs" / "check_docs.py"
SPEC = importlib.util.spec_from_file_location("check_docs", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
check_docs = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check_docs
SPEC.loader.exec_module(check_docs)


class DocumentationCheckTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, relative_path, text):
        path = self.root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_valid_relative_file_and_anchor_links_pass(self):
        self.write("README.md", "# Home\n\nSee [details](docs/guide.md#usage).\n")
        self.write("docs/guide.md", "# Guide\n\n## Usage\n\nInstructions.\n")

        result = check_docs.validate_documents(self.root)

        self.assertEqual(result.problems, [])
        self.assertEqual(result.local_links, 1)

    def test_missing_local_target_is_rejected(self):
        self.write("README.md", "# Home\n\nSee [missing](docs/missing.md).\n")

        result = check_docs.validate_documents(self.root)

        self.assertEqual(len(result.problems), 1)
        self.assertIn("missing local link target", result.problems[0])

    def test_links_inside_code_fences_are_not_interpreted(self):
        self.write(
            "README.md",
            "# Home\n\n```markdown\n[example](not-a-real-file.md)\n```\n",
        )

        result = check_docs.validate_documents(self.root)

        self.assertEqual(result.problems, [])
        self.assertEqual(result.local_links, 0)

    def test_formatting_hygiene_is_enforced(self):
        self.write("README.md", "# Home \t\n\nText without final newline")

        result = check_docs.validate_documents(self.root)

        self.assertTrue(any("trailing whitespace" in item for item in result.problems))
        self.assertTrue(any("tab character" in item for item in result.problems))
        self.assertTrue(any("missing final newline" in item for item in result.problems))


if __name__ == "__main__":
    unittest.main()
