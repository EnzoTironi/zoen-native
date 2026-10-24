"""Hold a real subprocess before exec while the Anchor owner reconciles.

The Rust harness uses maintained public APIs and std::process, without unsafe.
Only this Linux external observer uses ptrace to control scheduling.
"""
import ctypes
import errno
import json
import os
import pathlib
import signal
import sys
import time

libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.restype = ctypes.c_long
TRACEME, CONT, SETOPTIONS, GETEVENTMSG = 0, 7, 0x4200, 0x4201
OPTIONS = 0x2 | 0x4 | 0x8 | 0x10 | 0x100000  # fork/vfork/clone/exec/exitkill
WALL = 0x40000000


def ptrace(request, pid, data=0):
    result = libc.ptrace(request, pid, None, data)
    if result == -1:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), f'ptrace({request}, {pid})')
    return result


run = pathlib.Path(sys.argv[1])
run.mkdir()
binary = sys.argv[2]
pid = os.fork()
if pid == 0:
    ptrace(TRACEME, 0)
    os.environ['PROOF_RUN'] = str(run)
    os.execv(binary, [binary, *sys.argv[3:]])

_, initial = os.waitpid(pid, 0)
if not os.WIFSTOPPED(initial):
    raise RuntimeError(f'initial trace stop missing: {initial}')
ptrace(SETOPTIONS, pid, OPTIONS)
ptrace(CONT, pid)
held = set()
known = {pid}
pending_stops = {}
events = []
exit_code = None
released = False
deadline = time.monotonic() + 40


def record_held(child, status):
    event = status >> 16
    events.append({'held_child': child, 'event': event, 'stop_signal': os.WSTOPSIG(status)})
    descriptors = []
    for descriptor in pathlib.Path(f'/proc/{child}/fd').iterdir():
        try:
            descriptors.append({'fd': descriptor.name, 'target': os.readlink(descriptor)})
        except FileNotFoundError:
            pass
    (run / 'child-descriptors.json').write_text(json.dumps(descriptors, indent=2))
    if event == 4:
        raise RuntimeError('observer reached exec; this is not a pre-exec proof')
    required = '/writer.lock' if os.environ.get('ZOEN_ANCHOR_FORK_HELPER') == 'writer' else '/generation.lock'
    if not any(row['target'].endswith(required) for row in descriptors):
        raise RuntimeError(f'pre-exec child does not retain {required}')
    (run / 'pre-exec-held').write_text(str(child))


try:
    while known:
        if time.monotonic() > deadline:
            raise TimeoutError('bounded public-API fork proof timed out')
        if held and (run / 'observed.json').exists() and not released:
            for child in held:
                ptrace(CONT, child)
            released = True
        try:
            child, status = os.waitpid(-1, os.WNOHANG | WALL)
        except ChildProcessError:
            break
        if child == 0:
            time.sleep(0.002)
            continue
        if os.WIFEXITED(status) or os.WIFSIGNALED(status):
            known.discard(child)
            if child == pid:
                exit_code = os.waitstatus_to_exitcode(status)
                if exit_code != 0:
                    raise RuntimeError(f'public harness exit: {exit_code}')
            continue
        if not os.WIFSTOPPED(status):
            raise RuntimeError(f'unexpected wait status {status}')
        if child not in known:
            # The child stop may be delivered before its parent's clone event.
            # Do not accidentally continue it through exec before classifying it.
            pending_stops[child] = status
            continue
        event = status >> 16
        if event in (1, 2, 3):
            new_pid = ctypes.c_ulong()
            ptrace(GETEVENTMSG, child, ctypes.byref(new_pid))
            known.add(new_pid.value)
            events.append({'parent': child, 'child': new_pid.value,
                           'event': {1: 'fork', 2: 'vfork', 3: 'clone'}[event]})
            if event in (1, 2):
                held.add(new_pid.value)
            if new_pid.value in pending_stops:
                pending = pending_stops.pop(new_pid.value)
                if new_pid.value in held:
                    record_held(new_pid.value, pending)
                else:
                    ptrace(CONT, new_pid.value)
            ptrace(CONT, child)
        elif child in held and not released:
            record_held(child, status)
        else:
            sig = os.WSTOPSIG(status)
            # Trace traps and initial clone stops are observer events only.
            ptrace(CONT, child, 0 if sig in (signal.SIGTRAP, signal.SIGSTOP) else sig)
    if exit_code != 0:
        raise RuntimeError(f'public harness exit: {exit_code}')
    if not held or not released:
        raise RuntimeError('no real subprocess held before exec')
    observed = json.loads((run / 'observed.json').read_text())
    receipt = {'exit_code': exit_code, 'events': events,
               'child_released_after_observation': released, **observed}
    (run / 'trace-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
finally:
    for remaining in known:
        try:
            os.kill(remaining, signal.SIGKILL)
        except ProcessLookupError:
            pass
