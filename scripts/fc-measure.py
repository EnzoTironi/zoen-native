#!/usr/bin/env python3
"""Measure Firecracker on this host: cold boot to init, and snapshot restore to first reply.

Usage: fc-measure.py <tools-dir> [runs]. Called by `scripts/firecracker.sh measure`, which wraps
it in `timeout -s KILL`. Every VM here is also killed after 10 s, so nothing outlives the run.
The guest is alpine's minirootfs with a shell script as init: `/zinit` prints a marker and
reboots (cold boot), `/zecho` answers each line on the serial console (restore).
"""
import http.client, json, os, shutil, signal, socket, statistics, subprocess, sys, tarfile, time

TOOLS = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 10
WORK = os.path.join(TOOLS, "measure")
FC = os.path.join(TOOLS, "firecracker")


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=10)
        self.unix_path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(10)
        self.sock.connect(self.unix_path)


def api(sock, method, path, body=None):
    c = UnixHTTP(sock)
    c.request(method, path, json.dumps(body) if body is not None else None,
              {"Content-Type": "application/json"})
    r = c.getresponse()
    data = r.read()
    c.close()
    if r.status >= 300:
        raise RuntimeError(f"{method} {path}: {r.status} {data!r}")


def build_rootfs():
    shutil.rmtree(WORK, ignore_errors=True)
    root = os.path.join(WORK, "root")
    os.makedirs(root)
    tarfile.open(os.path.join(TOOLS, "alpine.tar.gz")).extractall(root, filter="tar")
    scripts = {
        "zinit": "#!/bin/sh\nmount -t proc proc /proc\necho ZOEN_BOOTED\nexec /bin/busybox reboot -f\n",
        "zecho": "#!/bin/sh\nmount -t proc proc /proc\necho ZOEN_READY\nwhile read l; do echo \"PONG $l\"; done\n",
    }
    for name, body in scripts.items():
        p = os.path.join(root, name)
        with open(p, "w") as f:
            f.write(body)
        os.chmod(p, 0o755)
    img = os.path.join(WORK, "rootfs.ext4")
    with open(img, "wb") as f:
        f.truncate(64 << 20)
    mkfs = shutil.which("mkfs.ext4") or "/sbin/mkfs.ext4"
    subprocess.run([mkfs, "-q", "-F", "-d", root, img], check=True)
    shutil.rmtree(root)
    return img


def config(img, init):
    return {
        "boot-source": {"kernel_image_path": os.path.join(TOOLS, "vmlinux"),
                        "boot_args": f"console=ttyS0 reboot=k panic=1 pci=off quiet init=/{init}"},
        "drives": [{"drive_id": "rootfs", "path_on_host": img, "is_root_device": True, "is_read_only": True}],
        "machine-config": {"vcpu_count": 2, "mem_size_mib": 128},
    }


def spawn(args):
    return subprocess.Popen([FC, "--level", "Error", *args], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)


def kill(p):
    if p.poll() is None:
        os.killpg(p.pid, signal.SIGKILL)
    p.wait()


def wait_line(p, marker, limit=10.0):
    t0 = time.perf_counter()
    for line in p.stdout:
        if marker in line:
            return True
        if time.perf_counter() - t0 > limit:
            return False
    return False


def wait_socket(path):
    for _ in range(4000):
        if os.path.exists(path):
            return
        time.sleep(0.00025)
    raise RuntimeError("no API socket")


def main():
    img = build_rootfs()
    cold_cfg = os.path.join(WORK, "cold.json")
    json.dump(config(img, "zinit"), open(cold_cfg, "w"))
    boots = []
    for _ in range(N):
        t0 = time.perf_counter()
        p = spawn(["--no-api", "--config-file", cold_cfg])
        try:
            ok = wait_line(p, b"ZOEN_BOOTED")
        finally:
            kill(p)
        if ok:
            boots.append((time.perf_counter() - t0) * 1000)

    # One template: boot, wait until init answers, pause, full snapshot.
    echo_cfg = os.path.join(WORK, "echo.json")
    json.dump(config(img, "zecho"), open(echo_cfg, "w"))
    src = os.path.join(WORK, "src.sock")
    p = spawn(["--api-sock", src, "--config-file", echo_cfg])
    try:
        assert wait_line(p, b"ZOEN_READY"), "template VM never became ready"
        api(src, "PATCH", "/vm", {"state": "Paused"})
        t0 = time.perf_counter()
        api(src, "PUT", "/snapshot/create", {"snapshot_type": "Full",
            "snapshot_path": os.path.join(WORK, "vm.state"), "mem_file_path": os.path.join(WORK, "vm.mem")})
        snap_ms = (time.perf_counter() - t0) * 1000
    finally:
        kill(p)

    loads, replies = [], []
    for i in range(N):
        sock = os.path.join(WORK, f"r{i}.sock")
        t0 = time.perf_counter()
        p = spawn(["--api-sock", sock])
        try:
            wait_socket(sock)
            api(sock, "PUT", "/snapshot/load", {"snapshot_path": os.path.join(WORK, "vm.state"),
                "mem_backend": {"backend_type": "File", "backend_path": os.path.join(WORK, "vm.mem")},
                "resume_vm": True})
            t_load = time.perf_counter() - t0
            p.stdin.write(b"ping\n")
            p.stdin.flush()
            ok = wait_line(p, b"PONG ping", 5)
            t_reply = time.perf_counter() - t0
        finally:
            kill(p)
            if os.path.exists(sock):
                os.unlink(sock)
        if ok:
            loads.append(t_load * 1000)
            replies.append(t_reply * 1000)

    host = subprocess.run(["sh", "-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2; "
                           "grep -qw hypervisor /proc/cpuinfo && echo nested || echo bare-metal"],
                          capture_output=True, text=True).stdout.split("\n")
    print(json.dumps({
        "host_cpu": host[0].strip(), "virtualization": host[1].strip(), "runs": N,
        "vm": {"vcpu": 2, "mem_mib": 128, "kernel": "6.1 (Firecracker CI)"},
        "cold_boot_ms": {"ok": len(boots), "p50": round(statistics.median(boots)), "min": round(min(boots)), "max": round(max(boots))},
        "snapshot_create_ms": round(snap_ms),
        "mem_file_mib": os.path.getsize(os.path.join(WORK, "vm.mem")) >> 20,
        "restore_load_ms": {"p50": round(statistics.median(loads), 1)},
        "restore_to_reply_ms": {"ok": len(replies), "p50": round(statistics.median(replies), 1),
                                "min": round(min(replies), 1), "max": round(max(replies), 1)},
    }, indent=2))
    shutil.rmtree(WORK, ignore_errors=True)


if __name__ == "__main__":
    main()
