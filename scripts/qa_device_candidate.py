"""Select only the source-qualified private iOS QA driver; never install it."""

import json
import os
from pathlib import Path
import shutil
import subprocess


# External MIT candidate: UIKit fixture696 and normal CLI fixture699 passed.
# Published0.21.31 alone is insufficient: it still contains the unsafe clear.
QUALIFIED_COMMIT = "ca3075064f7ba38b2cf452996556aa4b8b9db5cc"
PRIVATE_PATHS = (
    "AGENT_DEVICE_HOME",
    "AGENT_DEVICE_STATE_DIR",
    "AGENT_DEVICE_IOS_RUNNER_DERIVED_PATH",
)


class CandidateError(ValueError):
    pass


def candidate_command(arguments, environment=None):
    env = os.environ if environment is None else environment
    configured = env.get("ZOEN_QA_DRIVER_ROOT")
    if not configured:
        return None
    root = Path(configured)
    if not root.is_absolute():
        raise CandidateError("The qualified QA driver root must be absolute.")
    root = root.resolve()
    paths = []
    for name in PRIVATE_PATHS:
        path = Path(env.get(name, ""))
        if not path.is_absolute():
            raise CandidateError(f"The private driver requires an absolute {name}.")
        paths.append(path.resolve())
    if len(set(paths)) != len(paths):
        raise CandidateError("Driver config, daemon state and build paths must be distinct.")
    bundle = env.get("AGENT_DEVICE_IOS_BUNDLE_ID", "")
    if not bundle.startswith("dev.agentdevice.zoenqa.") or (
        env.get("AGENT_DEVICE_IOS_RUNNER_TEST_BUNDLE_ID") != bundle + ".uitests"
    ):
        raise CandidateError("The qualified driver requires its own QA runner bundle IDs.")
    if any(env.get(key) for key in (
        "AGENT_DEVICE_CONFIG", "AGENT_DEVICE_DAEMON_BASE_URL", "AGENT_DEVICE_DAEMON_AUTH_TOKEN"
    )) or any(arg == "--config" or arg.startswith("--config=") for arg in arguments):
        raise CandidateError("The qualified private driver cannot use another connection config.")
    try:
        def git(*args):
            return subprocess.run(
                ["git", "-C", str(root), *args], env=env, check=True,
                capture_output=True, text=True, timeout=5,
            ).stdout.strip()

        if Path(git("rev-parse", "--show-toplevel")).resolve() != root:
            raise CandidateError("The driver root must be the qualified checkout itself.")
        if git("rev-parse", "HEAD") != QUALIFIED_COMMIT or git("status", "--porcelain"):
            raise CandidateError("The private QA driver must match the exact clean qualified source.")
    except (OSError, subprocess.SubprocessError) as failure:
        raise CandidateError("Could not verify the private driver; no fallback was attempted.") from failure
    entry = root / "bin/agent-device.mjs"
    node = env.get("ZOEN_QA_NODE_BIN") or shutil.which("node", path=env.get("PATH"))
    if not entry.is_file() or not node or not Path(node).is_absolute() or not Path(node).is_file():
        raise CandidateError("The private Node executable and driver entrypoint are required.")
    # Never attach the corrected client to an unknown pre-existing daemon from
    # an unpatched package with the same upstream version. Own a fresh profile.
    state = paths[1]
    marker = state / ".zoen-qualified-source.json"
    expected = {"source": str(root), "commit": QUALIFIED_COMMIT}
    try:
        state.mkdir(parents=True, exist_ok=True, mode=0o700)
        if marker.exists():
            if json.loads(marker.read_text()) != expected:
                raise CandidateError("The private daemon profile belongs to another source.")
        else:
            if any(state.iterdir()):
                raise CandidateError("Use a fresh empty daemon directory; its existing source is unknown.")
            with marker.open("x") as output:
                json.dump(expected, output)
            marker.chmod(0o600)
    except (OSError, ValueError) as failure:
        if isinstance(failure, CandidateError):
            raise
        raise CandidateError("Could not verify the private daemon profile; no fallback was attempted.") from failure
    return [str(node), str(entry)]
