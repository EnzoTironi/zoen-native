"""Readiness and owned cleanup for the guard's blocked Cargo test fixture."""

import json
import subprocess
import time


def wait_for_owned_cargo(child, marker, root, target, deadline, *, clock=time.monotonic, pause=time.sleep):
    # The build script can run before Popen returns to the guard and its launched
    # callback publishes cargoPid. A marker alone is not the complete barrier.
    while child.poll() is None:
        if clock() >= deadline:
            break
        paths = list(root.glob(".cargo-source-guard-*/state.json"))
        if marker.exists() and len(paths) == 1:
            try:
                state = json.loads(paths[0].read_text())
            except (FileNotFoundError, ValueError):
                state = None
            if isinstance(state, dict):
                pid = state.get("cargoPid")
                if (state.get("status") == "in_progress"
                        and type(state.get("pid")) is int and state["pid"] == child.pid
                        and state.get("target") == str(target.resolve())
                        and type(pid) is int and pid > 0):
                    if clock() >= deadline:
                        break
                    return pid
        pause(0.02)
    raise AssertionError("Guard startup did not publish a live owner's Cargo PID before its deadline")


def retire_blocked_guard(child, stream, gate):
    # Releasing the fixture gate also lets an unpublished Cargo child finish.
    # Always retire/reap the exact guard we spawned and close its stream, even
    # when readiness failed before the caller received a Cargo PID.
    try:
        gate.unlink(missing_ok=True)
    finally:
        try:
            if child.poll() is None:
                try:
                    child.terminate()
                except ProcessLookupError:
                    pass
            try:
                child.wait(timeout=8)
            except subprocess.TimeoutExpired:
                try:
                    child.kill()
                except ProcessLookupError:
                    pass
                child.wait(timeout=8)
        finally:
            stream.close()
