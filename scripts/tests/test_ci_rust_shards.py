import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "ci-rust-shards.py"
SPEC = importlib.util.spec_from_file_location("rust_shards", SOURCE)
SHARDS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SHARDS)


def target(name, kind="test", test=True, doctest=False):
    return {"name": name, "kind": [kind], "test": test, "doctest": doctest}


def metadata(targets):
    return {"workspace_members": ["cli"], "packages": [
        {"id": "cli", "name": "zoen-cli", "targets": targets},
    ]}


class RustShardTests(unittest.TestCase):
    def test_new_journeys_join_exactly_one_shard_independent_of_metadata_order(self):
        targets = [target("zoen", "bin"), target("new_future_journey"),
                   target("journey_complete_reads"), target("journey_files"),
                   target("journey_m2"), target("zoen-image-host", "bin")]
        plans = [SHARDS.commands(metadata(targets), i)[0] for i in range(SHARDS.COUNT)]
        selected = [{command[i + 1] for i, arg in enumerate(command)
                     if arg in ("--bin", "--test")} for command in plans]
        self.assertEqual(sum(len(s) for s in selected), len(set.union(*selected)))
        self.assertEqual(set.union(*selected), {t["name"] for t in targets})
        for i in range(SHARDS.COUNT):
            self.assertEqual(SHARDS.commands(metadata(list(reversed(targets))), i)[0], plans[i])
            self.assertIn("--no-fail-fast", plans[i])

    def test_default_examples_compile_without_execution_and_doc_tests_run_once(self):
        targets = [target("zoen", "bin"), target("new_library", "lib", doctest=True),
                   target("canonical_check", "example", test=False)]
        left, right = [SHARDS.commands(metadata(targets), i) for i in (0, 1)]
        self.assertEqual([c[-2:] for c in left[1:]], [["--examples", "--no-run"],
                                                                  ["--no-fail-fast", "--doc"]])
        self.assertEqual(len(right), 1)

    def test_failed_test_command_does_not_hide_later_compile_or_doc_failures(self):
        plan = [["cargo", "test", "--test", "journey"],
                ["cargo", "test", "--examples", "--no-run"]]
        with patch.object(SHARDS.subprocess, "run", side_effect=[
                subprocess.CompletedProcess(plan[0], 101),
                subprocess.CompletedProcess(plan[1], 0)]) as run:
            self.assertEqual(SHARDS.execute(plan), 1)
            self.assertEqual([call.args[0] for call in run.call_args_list], plan)

    def test_invalid_or_unreviewed_target_selection_cannot_silently_omit_tests(self):
        valid = metadata([target("zoen", "bin"), target("journey")])
        cases = [(valid, 3), (metadata([]), 0),
                 (metadata([target("zoen", "bin"), target("unknown", "future")]), 0)]
        gated = target("optional")
        gated["required-features"] = ["new-profile"]
        cases.append((metadata([target("zoen", "bin"), gated]), 0))
        foreign = metadata(valid["packages"][0]["targets"])
        foreign["workspace_members"] = []
        cases.append((foreign, 0))
        for data, shard in cases:
            with self.subTest(data=data, shard=shard), self.assertRaises(ValueError):
                SHARDS.commands(data, shard)


if __name__ == "__main__":
    unittest.main()
