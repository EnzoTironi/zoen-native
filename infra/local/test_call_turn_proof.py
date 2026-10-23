"""Fault the real proof harness through a stateful Docker adapter; no Docker runs."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/call-turn-proof.py"
SPEC = importlib.util.spec_from_file_location("call_turn_proof", SCRIPT)
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


class Docker:
    def __init__(self):
        self.containers = {}
        self.networks = {}
        self.timeout_create = False
        self.timeout_network = False
        self.fail_logs = False
        self.fail_inspect = None
        self.removed = []

    def __call__(self, *args, **kwargs):
        def response(value="", code=0, error=""):
            return subprocess.CompletedProcess(args, code, value, error)

        if args[:2] == ("network", "ls"):
            return response()
        if args[:2] == ("network", "create"):
            name = args[-1]
            label, owner = args[args.index("--label") + 1].split("=", 1)
            self.networks[name] = {"Id": "network-id", "Labels": {label: owner}}
            if self.timeout_network:
                raise subprocess.TimeoutExpired("network create committed", 20)
            return response("network-id")
        if args[:2] == ("network", "inspect"):
            info = self.networks.get(args[-1])
            return (response(json.dumps([info])) if info else
                    response("[]", 1, "Error: No such network"))
        if args[:2] == ("network", "rm"):
            name = next(name for name, info in self.networks.items() if info["Id"] == args[-1])
            del self.networks[name]
            self.removed.append(args[-1])
            return response()
        if args[0] == "create":
            name = args[args.index("--name") + 1]
            label, owner = args[args.index("--label") + 1].split("=", 1)
            self.containers[name] = {"Id": "container-" + str(len(self.containers)),
                                     "Config": {"Labels": {label: owner}},
                                     "HostConfig": {"PortBindings": {}, "PublishAllPorts": False}}
            if self.timeout_create:
                raise subprocess.TimeoutExpired("container create committed", 20)
            return response(self.containers[name]["Id"])
        if args[0] == "inspect":
            if args[-1] == self.fail_inspect:
                raise subprocess.TimeoutExpired("inspect unavailable", 20)
            info = next((info for name, info in self.containers.items()
                         if args[-1] in (name, info["Id"])), None)
            return (response(json.dumps([info])) if info else
                    response("[]", 1, "Error: No such object"))
        if args[0] == "logs":
            if self.fail_logs:
                raise subprocess.TimeoutExpired("logs unavailable", 20)
            return response("safe log")
        if args[0] == "rm":
            name = next(name for name, info in self.containers.items() if info["Id"] == args[-1])
            del self.containers[name]
            self.removed.append(args[-1])
            return response()
        raise AssertionError("unexpected Docker operation: " + repr(args))


class CleanupTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.proof = HARNESS.Proof(Path(self.directory.name), {
            "secret": "a" * 64,
            "valid": {"username": "valid-user", "credential": "valid-password"},
            "expired": {"username": "expired-user", "credential": "expired-password"},
        })
        self.proof.private = Path(self.directory.name)
        self.docker = Docker()
        self.proof.docker = self.docker

    def test_create_commit_then_cli_timeout_still_removes_exact_owned_container(self):
        self.docker.timeout_create = True
        with self.assertRaises(subprocess.TimeoutExpired):
            self.proof.container("client", ["turnutils_uclient"])
        self.assertTrue(self.docker.containers)
        self.assertTrue(self.proof.cleanup())
        self.assertFalse(self.docker.containers)
        self.assertEqual(self.docker.removed, ["container-0"])
        self.assertEqual(len(self.proof.report["cleanup"]["containerIds"]), 1)

    def test_network_commit_then_cli_timeout_is_tracked_before_create_returns(self):
        self.docker.timeout_network = True
        with self.assertRaises(subprocess.TimeoutExpired):
            self.proof.allocate_network()
        self.assertTrue(self.docker.networks)
        self.assertTrue(self.proof.cleanup())
        self.assertFalse(self.docker.networks)
        self.assertEqual(self.docker.removed, ["network-id"])

    def test_log_failure_does_not_prevent_any_container_or_network_cleanup(self):
        self.proof.allocate_network()
        self.proof.server = self.proof.container("server", ["turnserver"])
        self.proof.container("peer", ["turnutils_peer"])
        self.docker.fail_logs = True
        self.assertFalse(self.proof.cleanup(), "missing observation must stay a failure")
        self.assertFalse(self.docker.containers)
        self.assertFalse(self.docker.networks)
        self.assertEqual(len(self.docker.removed), 3)
        self.assertIn("server log unavailable", self.proof.report["cleanup"]["failures"][0])

    def test_foreign_ownership_never_deletes_and_does_not_block_other_resources(self):
        self.proof.container("foreign", ["turnserver"])
        self.proof.container("owned", ["turnutils_peer"])
        name = self.proof.containers[0]
        self.docker.containers[name]["Config"]["Labels"][HARNESS.OWNER_LABEL] = "another-owner"
        self.assertFalse(self.proof.cleanup())
        self.assertEqual(list(self.docker.containers), [name])
        self.assertEqual(self.docker.removed, ["container-1"])

    def test_one_inspection_timeout_cannot_abort_later_cleanup(self):
        self.proof.container("first", ["turnserver"])
        self.proof.container("second", ["turnutils_peer"])
        self.docker.fail_inspect = self.proof.containers[1]
        self.assertFalse(self.proof.cleanup())
        self.assertEqual(list(self.docker.containers), [self.proof.containers[1]])
        self.assertEqual(self.docker.removed, ["container-0"])


if __name__ == "__main__":
    unittest.main()
