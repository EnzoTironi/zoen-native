import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

from cargo_guard_process import retire_blocked_guard, wait_for_owned_cargo


class Guard:
    pid = 71

    def poll(self):
        return None


class CargoGuardReadinessTests(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory(prefix="guard-readiness-")
        self.addCleanup(temporary.cleanup)
        root = pathlib.Path(temporary.name)
        marker = root / "started"
        marker.write_text("build script is already running")
        path = root / ".cargo-source-guard-fixture/state.json"
        path.parent.mkdir()
        target = root / "target"
        state = {"status": "in_progress", "pid": Guard.pid, "target": str(target.resolve())}
        return root, marker, path, target, state

    def test_marker_before_pid_publication_waits_for_the_owned_state(self):
        root, marker, path, target, state = self.fixture()
        path.write_text(json.dumps(state))
        # This is the exact CI precondition: the script marker exists, but the
        # parent's atomic launched(pid) publication has not happened yet.
        with self.assertRaises(KeyError):
            json.loads(path.read_text())["cargoPid"]
        observed = []

        def publish_after_observation(delay):
            self.assertEqual(delay, 0.02)
            observed.append(json.loads(path.read_text()))
            path.write_text(json.dumps({**state, "cargoPid": 83}))

        actual = wait_for_owned_cargo(Guard(), marker, root, target, 20,
                                     clock=lambda: len(observed), pause=publish_after_observation)
        self.assertEqual(actual, 83)
        self.assertEqual(observed, [state])

    def test_stale_owner_target_status_and_noninteger_pids_never_satisfy_the_barrier(self):
        root, marker, path, target, state = self.fixture()
        valid = {**state, "cargoPid": 83}
        variants = [
            {**valid, "pid": 72}, {**valid, "target": str(root / "foreign")},
            {**valid, "status": "bound"}, {**valid, "cargoPid": True},
            {**valid, "cargoPid": 83.0}, {**valid, "cargoPid": 0}, state,
        ]
        path.write_text("{")
        observed = []

        def publish_next(_):
            observed.append(path.read_text())
            path.write_text(json.dumps(variants.pop(0) if variants else valid))

        self.assertEqual(wait_for_owned_cargo(Guard(), marker, root, target, 20,
                         clock=lambda: len(observed), pause=publish_next), 83)
        self.assertEqual(len(observed), 8)

    def test_unpublished_state_does_not_extend_the_existing_deadline(self):
        root, marker, path, target, state = self.fixture()
        path.write_text(json.dumps(state))
        times = iter([19.99, 20])
        pauses = []
        with self.assertRaisesRegex(AssertionError, "deadline"):
            wait_for_owned_cargo(Guard(), marker, root, target, 20,
                                 clock=lambda: next(times), pause=pauses.append)
        self.assertEqual(pauses, [0.02])

    def test_exited_guard_is_not_admitted_even_with_a_published_pid(self):
        root, marker, path, target, state = self.fixture()
        path.write_text(json.dumps({**state, "cargoPid": 83}))
        child = Guard()
        child.poll = lambda: 0
        with self.assertRaises(AssertionError):
            wait_for_owned_cargo(child, marker, root, target, 20)

    def test_published_pid_cannot_be_admitted_at_or_after_deadline(self):
        root, marker, path, target, state = self.fixture()
        path.write_text(json.dumps({**state, "cargoPid": 83}))
        for samples in ([20], [19.99, 20]):
            with self.subTest(clock=samples):
                clock = iter(samples)
                with self.assertRaisesRegex(AssertionError, "deadline"):
                    wait_for_owned_cargo(Guard(), marker, root, target, 20,
                                         clock=lambda: next(clock),
                                         pause=lambda _: self.fail("no grace after the deadline"))

    def test_readiness_failure_cleanup_releases_gate_closes_stream_and_reaps_owned_child(self):
        root, _, _, _, _ = self.fixture()
        gate = root / "gate"
        gate.write_text("hold")
        stream = (root / "blocked.log").open("wb")
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"],
                                 stdout=stream, stderr=subprocess.STDOUT)
        try:
            retire_blocked_guard(child, stream, gate)
            self.assertFalse(gate.exists())
            self.assertTrue(stream.closed)
            self.assertIsNotNone(child.poll())
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            stream.close()


if __name__ == "__main__":
    unittest.main()
