import importlib.util
import io
import pathlib
import sys
import unittest

MODULE_PATH = (
    pathlib.Path(__file__).parents[1] / "maintainability" / "check_lizard.py"
)
SPEC = importlib.util.spec_from_file_location("check_lizard", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
check_lizard = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check_lizard
SPEC.loader.exec_module(check_lizard)


def metric_csv(
    *,
    path="src/example.rs",
    name="work",
    signature="work value : usize",
    nloc=81,
    ccn=16,
    start=10,
    end=90,
):
    return (
        f'{nloc},{ccn},200,1,{end - start + 1},'
        f'"{name}@{start}-{end}@{path}","{path}","{name}",'
        f'"{signature}",{start},{end}\n'
    )


def parse(text):
    return check_lizard.parse_lizard_csv(io.StringIO(text))


class ComplexityRatchetTests(unittest.TestCase):
    def test_synthetic_hotspot_regression_is_rejected(self):
        baseline_metric = parse(metric_csv(nloc=81, ccn=16))[0]
        current_metric = parse(metric_csv(nloc=82, ccn=17, end=91))[0]

        problems = check_lizard.compare_metrics(
            [current_metric], {baseline_metric.key: baseline_metric}
        )

        self.assertEqual(len(problems), 1)
        self.assertIn("NLOC 81->82", problems[0])
        self.assertIn("CCN 16->17", problems[0])

    def test_new_function_may_not_cross_either_threshold(self):
        problems = check_lizard.compare_metrics(parse(metric_csv()), {})

        self.assertEqual(len(problems), 1)
        self.assertIn("new maintainability debt", problems[0])

    def test_existing_debt_may_decrease(self):
        baseline_metric = parse(metric_csv(nloc=100, ccn=20, end=109))[0]
        current_metric = parse(metric_csv(nloc=79, ccn=15, end=88))[0]

        problems = check_lizard.compare_metrics(
            [current_metric], {baseline_metric.key: baseline_metric}
        )

        self.assertEqual(problems, [])

    def test_duplicate_signatures_receive_deterministic_occurrences(self):
        later = metric_csv(start=30, end=110)
        earlier = metric_csv(start=1, end=81)

        metrics = parse(later + earlier)

        self.assertEqual(
            [(metric.start_line, metric.key.occurrence) for metric in metrics],
            [(1, 1), (30, 2)],
        )

    def test_malformed_csv_is_rejected(self):
        with self.assertRaises(check_lizard.RatchetError):
            parse("not,a,lizard,row\n")


if __name__ == "__main__":
    unittest.main()
