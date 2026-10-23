#!/usr/bin/env python3
"""Opt-in real coturn proof. Called by the ignored production-issuer Rust test.

Only UUID-named resources created here may be removed. No host ports, existing
Docker services, product accounts, stored credentials or shared Cargo targets.
"""
import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import uuid

IMAGE = "coturn/coturn@sha256:bbefd3e1fdfdc0d58770fe01b581fd8b00d9f3a5580d00acb77cf719a6bc78e3"
HOST = "turn.zoen-proof.invalid"
OWNER_LABEL = "io.zoen.turn-proof"
ROOT = Path(__file__).resolve().parents[1]


class Proof:
    def __init__(self, evidence, data):
        self.evidence = evidence
        self.data = data
        self.owner = "zoen-turn-proof-" + uuid.uuid4().hex
        self.network = None
        self.containers = []
        self.server = None
        self.private = None
        self.redactions = [data["secret"]]
        for case in ("valid", "expired"):
            self.redactions.extend(data[case].values())
        self.report = {"status": "FAIL", "image": IMAGE, "owner": self.owner,
                       "hostPorts": [], "cases": {}, "cleanup": {"complete": False}}

    def redact(self, value):
        for secret in sorted(set(self.redactions), key=len, reverse=True):
            value = value.replace(secret, "<redacted>")
        return re.sub(r"\x1b\[[0-9;]*m", "", value)

    def command(self, args, timeout=20, check=True):
        # Never print a command: upstream clients accept passwords in arguments
        # inside their own container, obtained there from our private mount.
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        if check and result.returncode:
            raise RuntimeError(self.redact(result.stdout + result.stderr)[-3000:])
        return result

    def docker(self, *args, **kwargs):
        return self.command(["docker", *args], **kwargs)

    def container(self, suffix, command, ip=None, server=False, writable=False):
        name = self.owner + "-" + suffix
        args = ["create", "--name", name, "--label", OWNER_LABEL + "=" + self.owner,
                "--network", self.network or "none", "--user", "0:0", "--read-only",
                "--cap-drop", "ALL", "--pids-limit", "64", "--memory", "128m",
                "--cpus", "0.5", "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=16m",
                "--tmpfs", "/var/lib/coturn:rw,nosuid,nodev,noexec,size=16m"]
        if ip:
            args.extend(["--ip", ip])
        if server:
            # Upstream's turnserver has a file capability. Its exec requires this
            # bounding capability even though this proof listens above port 1024.
            args.extend(["--cap-add", "NET_BIND_SERVICE", "--network-alias", HOST])
        mount = f"type=bind,src={self.private},dst=/proof"
        if not writable:
            mount += ",readonly"
        args.extend(["--mount", mount, "--entrypoint", command[0], IMAGE, *command[1:]])
        cid = self.docker(*args).stdout.strip()
        self.containers.append(cid)
        info = json.loads(self.docker("inspect", cid).stdout)[0]
        if info["HostConfig"].get("PortBindings") or info["HostConfig"].get("PublishAllPorts"):
            raise RuntimeError("proof container unexpectedly publishes host ports")
        return cid

    def run(self, suffix, command, timeout=25, check=True, writable=False):
        cid = self.container(suffix, command, writable=writable)
        try:
            return self.docker("start", "--attach", cid, timeout=timeout, check=check)
        except subprocess.TimeoutExpired:
            self.docker("kill", cid, check=False)
            raise RuntimeError(f"owned client {suffix} exceeded {timeout}s") from None

    def allocate_network(self):
        ids = self.docker("network", "ls", "-q").stdout.split()
        existing = json.loads(self.docker("network", "inspect", *ids).stdout) if ids else []
        ranges = [ipaddress.ip_network(config["Subnet"])
                  for network in existing for config in (network.get("IPAM", {}).get("Config") or [])
                  if config.get("Subnet")]
        # Documentation-only addresses let the actual production private-peer
        # deny rules stay enabled. --internal prevents egress; no ports publish.
        for candidate in ("192.0.2.0/24", "198.51.100.0/24", "203.0.113.0/24"):
            subnet = ipaddress.ip_network(candidate)
            if any(subnet.version == old.version and subnet.overlaps(old) for old in ranges):
                continue
            name = self.owner + "-net"
            self.docker("network", "create", "--internal", "--subnet", candidate,
                        "--label", OWNER_LABEL + "=" + self.owner, name)
            self.network = name
            self.report["subnet"] = candidate
            return str(subnet.network_address + 2), str(subnet.network_address + 3)
        raise RuntimeError("all isolated documentation subnets overlap existing networks")

    def setup(self):
        self.report["imageInspection"] = json.loads(self.docker("image", "inspect", IMAGE).stdout)[0]["Id"]
        server_ip, peer_ip = self.allocate_network()
        cert = self.run("cert", ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                                "-keyout", "/proof/tls.key", "-out", "/proof/tls.crt", "-days", "1",
                                "-subj", "/CN=" + HOST, "-addext", "subjectAltName=DNS:" + HOST],
                        writable=True)
        self.report["certificateGenerated"] = cert.returncode == 0
        template = (ROOT / "infra/local/coturn.conf.example").read_text()
        self.report["templateSha256"] = hashlib.sha256(template.encode()).hexdigest()
        config = template.replace("realm=turn.zoen.invalid", "realm=" + HOST)
        config = config.replace("/run/zoen-turn/tls.crt", "/proof/tls.crt")
        config = config.replace("/run/zoen-turn/tls.key", "/proof/tls.key")
        config += (f"\nstatic-auth-secret={self.data['secret']}\nlistening-ip={server_ip}\n"
                   f"relay-ip={server_ip}\nrelay-threads=1\npidfile=/tmp/turn.pid\n")
        (self.private / "server.conf").write_text(config)
        (self.evidence / "server.redacted.conf").write_text(self.redact(config))
        self.server = self.container("server", ["turnserver", "-c", "/proof/server.conf",
                                                "--log-file", "stdout", "--simple-log"],
                                     ip=server_ip, server=True)
        self.docker("start", self.server)
        peer = self.container("peer", ["turnutils_peer", "-L", peer_ip, "-p", "3480"], ip=peer_ip)
        self.docker("start", peer)
        self.peer_ip = peer_ip
        self.tls_ready()

    def tls_ready(self):
        command = ["/bin/sh", "-c", "exec openssl s_client -brief -verify_return_error "
                   f"-verify_hostname {HOST} -servername {HOST} -CAfile /proof/tls.crt "
                   f"-connect {HOST}:5349 </dev/null"]
        deadline = time.monotonic() + 12
        for attempt in range(12):
            result = self.run("tls-" + str(attempt), command, timeout=5, check=False)
            log = self.redact(result.stdout + result.stderr)
            if result.returncode == 0 and "Verification: OK" in log:
                (self.evidence / "tls.log").write_text(log)
                self.report["tlsCertificateAndHostnameVerified"] = True
                return
            if time.monotonic() >= deadline:
                break
            time.sleep(0.25)
        raise RuntimeError("TLS readiness/certificate/hostname verification failed: " + log[-1500:])

    def client(self, name, username, credential, peer=None):
        self.redactions.extend([username, credential])
        (self.private / (name + ".credentials")).write_text(username + "\n" + credential + "\n")
        # No -W: the upstream client must use our production-issued credential,
        # not generate a separate REST password itself.
        shell = ("exec turnutils_uclient -t -S -v -c -m 1 -n 5 -l 100 -z 20 "
                 "-E /proof/tls.crt -p 5349 "
                 f'-u "$(sed -n 1p /proof/{name}.credentials)" '
                 f'-w "$(sed -n 2p /proof/{name}.credentials)" '
                 f"-e {peer or self.peer_ip} -r 3480 {HOST}")
        result = self.run(name, ["/bin/sh", "-c", shell], check=False)
        log = self.redact(result.stdout + result.stderr)
        (self.evidence / (name + ".log")).write_text(log)
        return result.returncode, log

    def cases(self):
        valid = self.data["valid"]
        code, log = self.client("valid", **valid)
        counts = re.findall(r"tot_send_msgs=(\d+), tot_recv_msgs=(\d+)", log)
        sent, received = (int(value) for value in counts[-1]) if counts else (None, None)
        loss = re.search(r"total lost packets (\d+)", log, re.I)
        ok = code == 0 and sent is not None and sent >= 5 and sent == received
        ok = bool(ok and loss and int(loss[1]) == 0)
        self.report["cases"]["valid"] = {"pass": ok, "exit": code,
                                           "sent": sent, "received": received}
        if not ok:
            raise RuntimeError("valid production credentials did not prove a complete UDP echo relay")
        wrong = ("A" if valid["credential"][0] != "A" else "B") + valid["credential"][1:]
        for name, credential in (("invalid-mac", {**valid, "credential": wrong}),
                                 ("expired", self.data["expired"])):
            reason = ("message integrity does not match any auth secret" if name == "invalid-mac"
                      else "the time-limited username timestamp is in the past")
            before = self.docker("logs", self.server)
            count_before = (before.stdout + before.stderr).count(reason)
            code, log = self.client(name, **credential)
            # A timeout, TLS failure, empty output or unrelated failure is NOT an
            # authentication rejection proof. Require a fresh, explicit auth
            # cause from the pinned server, not the initial nonce challenge.
            after = self.docker("logs", self.server)
            count_after = (after.stdout + after.stderr).count(reason)
            refused = code != 0 and "Cannot complete Allocation" in log and count_after > count_before
            relayed = re.search(r"tot_recv_msgs=([1-9]\d*)", log)
            self.report["cases"][name] = {"pass": refused and not relayed, "exit": code,
                                          "serverAuthRejections": count_after - count_before,
                                          "reason": "integrity_mismatch" if name == "invalid-mac" else "expired"}
            if not refused or relayed:
                raise RuntimeError(name + " was not proven refused by TURN authentication")
        code, log = self.client("private-peer", **valid, peer="10.0.0.1")
        refused = code != 0 and bool(re.search(r"channel bind: error 403\b", log, re.I))
        self.report["cases"]["private-peer"] = {"pass": refused, "exit": code,
                                                 "forbidden403": refused}
        if not refused:
            raise RuntimeError("private-peer ChannelBind was not proven forbidden")

    def cleanup(self):
        failures = []
        if self.server:
            result = self.docker("logs", self.server, check=False)
            (self.evidence / "server.log").write_text(self.redact(result.stdout + result.stderr))
        for cid in reversed(self.containers):
            inspected = self.docker("inspect", cid, check=False)
            if inspected.returncode:
                failures.append("cannot inspect owned container " + cid)
                continue
            info = json.loads(inspected.stdout)[0]
            if info.get("Config", {}).get("Labels", {}).get(OWNER_LABEL) != self.owner:
                failures.append("ownership mismatch for " + cid)
                continue
            if self.docker("rm", "--force", "--volumes", cid, check=False).returncode:
                failures.append("cannot remove owned container " + cid)
        if self.network:
            info = json.loads(self.docker("network", "inspect", self.network).stdout)[0]
            if info.get("Labels", {}).get(OWNER_LABEL) != self.owner:
                failures.append("network ownership mismatch")
            elif self.docker("network", "rm", self.network, check=False).returncode:
                failures.append("cannot remove owned network")
        self.report["cleanup"] = {"complete": not failures, "failures": failures,
                                   "containerIds": self.containers, "network": self.network}
        return not failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    if os.environ.get("ZOEN_TURN_DOCKER_PROOF") != "1":
        parser.error("explicit ZOEN_TURN_DOCKER_PROOF=1 is required")
    if not args.evidence.is_absolute() or (args.evidence / "report.json").exists():
        parser.error("absolute new evidence directory is required")
    data = json.loads(args.input.read_text())
    args.input.unlink()
    if not re.fullmatch(r"[0-9a-f]{64}", data["secret"]):
        parser.error("synthetic 32-byte hex secret required")
    proof = Proof(args.evidence, data)
    try:
        with tempfile.TemporaryDirectory(prefix=".turn-private-", dir=args.evidence) as private:
            proof.private = Path(private)
            try:
                proof.setup()
                proof.cases()
                proof.report["status"] = "PASS"
            finally:
                if not proof.cleanup():
                    proof.report["status"] = "FAIL"
    except Exception as error:
        proof.report["status"] = "FAIL"
        proof.report["error"] = proof.redact(str(error))
    (args.evidence / "report.json").write_text(json.dumps(proof.report, indent=2) + "\n")
    print(json.dumps({"turnProof": proof.report["status"], "cleanup": proof.report["cleanup"]["complete"]}))
    return 0 if proof.report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
