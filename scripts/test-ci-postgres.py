#!/usr/bin/env python3
"""Exercise CI setup without starting services or requiring PostgreSQL/sudo locally."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HELPER = Path(__file__).with_name("ci-postgres.sh")
URL = "postgres://zoen:zoen-ci@127.0.0.1:5432/postgres"
FAKE = r'''
import json
import os
from pathlib import Path
import re
import sys

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
directory = Path(os.environ["PG_CI_MOCK"])
state_file = directory / "state.json"
state = json.loads(state_file.read_text()) if state_file.exists() else {}
with (directory / "commands.jsonl").open("a") as output:
    output.write(json.dumps([tool, *args]) + "\n")
mode = os.environ.get("PG_CI_MODE", "success")

def finish(code=0):
    state_file.write_text(json.dumps(state))
    sys.exit(code)

def invoke(arguments):
    executable = directory / "bin" / arguments[0]
    assert executable.is_file(), arguments
    os.execv(str(executable), [str(executable), *arguments[1:]])

if tool == "timeout":
    assert args[0] in ("5s", "10s", "60s"), args
    if mode == "start_timeout" and "restart" in args:
        finish(124)
    invoke(args[1:])
elif tool == "sudo":
    assert args.pop(0) == "-n"
    if args[0] == "-u":
        assert args[:2] == ["-u", "postgres"]
        args = args[2:]
    invoke(args)
elif tool == "pg_lsclusters":
    version = "15" if mode == "missing_cluster" else "16"
    print(f"{version} main 5432 down postgres /var/lib/postgresql/{version}/main /var/log/postgresql/postgresql-{version}-main.log")
elif tool == "pg_conftool":
    assert args[:2] == ["16", "main"]
    action, key = args[2:4]
    if action == "set":
        value = args[4]
        # PgCommon.pm quote_conf_value: numeric-looking IPv4 addresses remain unquoted.
        # https://salsa.debian.org/postgresql/postgresql-common/-/blob/master/PgCommon.pm
        if not re.fullmatch(r"-?[\d.]+|\w+", value):
            value = "'" + value.replace("'", "''") + "'"
        state[key] = value
    else:
        assert action == "show"
        print(f"{key} = {state[key]}")
elif tool == "systemctl":
    if args[0] == "restart":
        assert args == ["restart", "postgresql@16-main.service"]
        if mode == "start_failed":
            print("cluster startup failed", file=sys.stderr)
            finish(37)
        # PostgreSQL guc-file.l accepts one value before EOL; 127.0.0.1 is two REALs.
        value = state["listen_addresses"]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*|'(?:[^']|'')*'|[-+]?\d+(?:\.\d+)?", value):
            print(f"invalid PostgreSQL config value: {value}", file=sys.stderr)
            finish(1)
        state["running"] = True
    else:
        assert "status" in args
        print("mock cluster status", file=sys.stderr)
        finish(3)
elif tool == "pg_isready":
    assert "--host=127.0.0.1" in args and "--timeout=1" in args
    assert state.get("running")
    state["polls"] = state.get("polls", 0) + 1
    if mode == "second_poll" and state["polls"] == 1:
        finish(1)
elif tool == "psql":
    assert "-X" in args and "-w" in args and "--set=ON_ERROR_STOP=1" in args
    if args[0].startswith("postgres://"):
        assert args[0] == "postgres://zoen:zoen-ci@127.0.0.1:5432/postgres"
        assert os.environ["PGCONNECT_TIMEOUT"] == "5"
        assert state.get("role") == "zoen"
        if mode == "auth_failed":
            print("password authentication failed", file=sys.stderr)
            finish(47)
        maximum = "299" if mode == "wrong_settings" else state["max_connections"]
        print(f"16|zoen|{maximum}|t")
    else:
        sql = sys.stdin.read()
        assert "IF NOT EXISTS" in sql and "CREATE ROLE zoen" in sql
        assert "LOGIN SUPERUSER PASSWORD 'zoen-ci'" in sql
        assert "scram-sha-256" in sql
        state["role"] = "zoen"
elif tool == "cat":
    assert args == ["/etc/postgresql/16/main/start.conf"]
    print("auto")
elif tool in ("journalctl", "tail"):
    print(f"mock {tool} diagnostics", file=sys.stderr)
    finish(89)  # Diagnostic failures must not mask the original startup exit status.
elif tool != "sleep":
    raise AssertionError((tool, args))
finish()
'''


class PostgresSetupTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zoen-ci-postgres-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        binaries = self.directory / "bin"
        binaries.mkdir()
        for tool in ("timeout", "sudo", "pg_lsclusters", "pg_conftool", "systemctl",
                     "pg_isready", "psql", "cat", "journalctl", "tail", "sleep"):
            executable = binaries / tool
            executable.write_text(f"#!{sys.executable}\n" + FAKE)
            executable.chmod(0o755)
        self.export = self.directory / "github-env"
        self.export.touch()
        self.environment = dict(os.environ, GITHUB_ACTIONS="true", RUNNER_OS="Linux",
                                GITHUB_ENV=str(self.export), PG_CI_MOCK=str(self.directory),
                                PATH=f"{binaries}:/usr/bin:/bin")

    def run_helper(self, mode="success"):
        return subprocess.run(["/bin/bash", str(HELPER)],
                              env=dict(self.environment, PG_CI_MODE=mode),
                              capture_output=True, text=True, timeout=10)

    def assert_failed_without_export(self, result, status):
        self.assertEqual(status, result.returncode, result.stderr)
        self.assertEqual("", self.export.read_text())

    def test_configured_server_authenticates_and_repeat_keeps_it_configured(self):
        for _ in range(2):
            result = self.run_helper()
            self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(f"ZOEN_TEST_PG={URL}\n" * 2, self.export.read_text())

    def test_readiness_can_require_another_poll(self):
        result = self.run_helper("second_poll")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(2, json.loads((self.directory / "state.json").read_text())["polls"])

    def test_missing_cluster_fails(self):
        result = self.run_helper("missing_cluster")
        self.assert_failed_without_export(result, 1)
        self.assertIn("Expected the runner's installed PostgreSQL", result.stderr)

    def test_startup_failure_retains_status_and_collects_diagnostics(self):
        result = self.run_helper("start_failed")
        self.assert_failed_without_export(result, 37)
        self.assertIn("startup diagnostics", result.stderr)
        self.assertIn("mock journalctl diagnostics", result.stderr)
        self.assertIn("mock tail diagnostics", result.stderr)
        self.assertIn("auto", result.stderr)

    def test_startup_timeout_retains_timeout_status(self):
        self.assert_failed_without_export(self.run_helper("start_timeout"), 124)

    def test_tcp_authentication_failure_does_not_publish_fixture(self):
        self.assert_failed_without_export(self.run_helper("auth_failed"), 47)

    def test_wrong_server_settings_do_not_publish_fixture(self):
        result = self.run_helper("wrong_settings")
        self.assert_failed_without_export(result, 1)
        self.assertIn("Unexpected PostgreSQL fixture configuration", result.stderr)

    def test_non_actions_environment_cannot_touch_services(self):
        self.environment["GITHUB_ACTIONS"] = "false"
        self.assert_failed_without_export(self.run_helper(), 1)
        self.assertFalse((self.directory / "commands.jsonl").exists())


if __name__ == "__main__":
    unittest.main()
