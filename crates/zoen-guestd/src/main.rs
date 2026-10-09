//! `zoen-guestd`: PID 1 inside a Zoen microVM.
//!
//! As PID 1 it mounts the basics, then forks: the parent only reaps orphans, the child serves.
//! It serves two things:
//! - control requests from the host on vsock port 52 (exec, put, get, hello), one per
//!   connection;
//! - in a browser microVM, the browser (see `browser.rs`): Chromium driven over a CDP pipe
//!   that never leaves the VM;
//! - the in-VM proxy address 127.0.0.1:3128, where each TCP connection is spliced onto a new
//!   vsock connection to the host (port 1080), which the host hands to the egress proxy. The
//!   VM has no other network: no NIC, only loopback.

#[cfg(target_os = "linux")]
fn main() {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("zoen-guestd runs inside Linux microVMs only");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
mod browser;

#[cfg(target_os = "linux")]
mod linux {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use std::ffi::CString;
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::os::fd::{FromRawFd, RawFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    use zoen_guestd::*;

    const MAX_REQUEST: u64 = 96 * 1024 * 1024;
    const BASE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    const SYSTEM_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

    pub fn main() {
        if std::process::id() == 1 {
            init_system();
            // SAFETY: fork in a single-threaded process; the child only continues normally.
            match unsafe { libc::fork() } {
                0 => serve(),
                pid if pid > 0 => reap_forever(),
                _ => serve(),
            }
        } else {
            serve()
        }
    }

    fn reap_forever() -> ! {
        loop {
            let mut status = 0;
            // SAFETY: plain waitpid on any child.
            let r = unsafe { libc::waitpid(-1, &mut status, 0) };
            if r < 0 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    fn mount(src: &str, target: &str, fstype: &str, flags: libc::c_ulong, data: &str) {
        let _ = std::fs::create_dir_all(target);
        let (s, t, f, d) = (
            CString::new(src).unwrap(),
            CString::new(target).unwrap(),
            CString::new(fstype).unwrap(),
            CString::new(data).unwrap(),
        );
        // SAFETY: valid NUL-terminated strings.
        let r = unsafe {
            libc::mount(
                s.as_ptr(),
                t.as_ptr(),
                f.as_ptr(),
                flags,
                d.as_ptr() as *const libc::c_void,
            )
        };
        if r != 0 {
            eprintln!(
                "zoen-guestd: mount {target}: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    pub(crate) fn mount_tmpfs(target: &str, data: &str) {
        let nodev = (libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong;
        mount("tmpfs", target, "tmpfs", nodev, data);
    }

    fn cmdline_value(key: &str) -> Option<String> {
        let line = std::fs::read_to_string("/proc/cmdline").ok()?;
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=').map(str::to_string))
    }

    fn init_system() {
        let nodev = (libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong;
        mount("proc", "/proc", "proc", nodev | libc::MS_NOEXEC, "");
        mount("sysfs", "/sys", "sysfs", nodev | libc::MS_NOEXEC, "");
        mount("devtmpfs", "/dev", "devtmpfs", libc::MS_NOSUID, "");
        mount("tmpfs", "/run", "tmpfs", nodev, "mode=0755,size=16m");
        mount("tmpfs", "/tmp", "tmpfs", nodev, "mode=1777,size=64m");
        let work_mib = cmdline_value("zoen.work_mib").unwrap_or_else(|| "256".into());
        mount(
            "tmpfs",
            GUEST_WORK,
            "tmpfs",
            nodev,
            &format!("mode=0755,size={work_mib}m"),
        );
        mount("tmpfs", "/root", "tmpfs", nodev, "mode=0700,size=16m");
        let _ = std::fs::create_dir_all("/run/zoen");
        let host = b"sandbox";
        // SAFETY: valid buffer.
        unsafe { libc::sethostname(host.as_ptr() as *const libc::c_char, host.len()) };
        loopback_up();
    }

    fn loopback_up() {
        // SAFETY: ioctl on a fresh datagram socket with a zeroed, then filled, ifreq.
        unsafe {
            let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
            if fd < 0 {
                return;
            }
            let mut ifr: libc::ifreq = std::mem::zeroed();
            for (i, b) in b"lo".iter().enumerate() {
                ifr.ifr_name[i] = *b as libc::c_char;
            }
            if libc::ioctl(fd, libc::SIOCGIFFLAGS as _, &mut ifr) == 0 {
                ifr.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
                libc::ioctl(fd, libc::SIOCSIFFLAGS as _, &ifr);
            }
            libc::close(fd);
        }
    }

    fn vsock_listen(port: u32) -> std::io::Result<RawFd> {
        // SAFETY: socket/bind/listen with a correctly sized sockaddr_vm.
        unsafe {
            let fd = libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut addr: libc::sockaddr_vm = std::mem::zeroed();
            addr.svm_family = libc::AF_VSOCK as libc::sa_family_t;
            addr.svm_port = port;
            addr.svm_cid = libc::VMADDR_CID_ANY;
            if libc::bind(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            ) != 0
                || libc::listen(fd, 64) != 0
            {
                let e = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(e);
            }
            Ok(fd)
        }
    }

    fn vsock_accept(fd: RawFd) -> std::io::Result<File> {
        // SAFETY: accept on a listening socket; the new fd is owned by the File.
        let c = unsafe {
            libc::accept4(
                fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };
        if c < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(c) })
    }

    pub(crate) fn vsock_connect_host(port: u32) -> std::io::Result<File> {
        // SAFETY: as above; CID 2 is the host.
        unsafe {
            let fd = libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut addr: libc::sockaddr_vm = std::mem::zeroed();
            addr.svm_family = libc::AF_VSOCK as libc::sa_family_t;
            addr.svm_port = port;
            addr.svm_cid = libc::VMADDR_CID_HOST;
            if libc::connect(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            ) != 0
            {
                let e = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(e);
            }
            Ok(File::from_raw_fd(fd))
        }
    }

    fn serve() -> ! {
        std::thread::spawn(proxy_forwarder);
        let fd = loop {
            match vsock_listen(CONTROL_PORT) {
                Ok(fd) => break fd,
                Err(e) => {
                    eprintln!("zoen-guestd: vsock listen: {e}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        println!("ZOEN_GUESTD_READY");
        loop {
            match vsock_accept(fd) {
                Ok(conn) => {
                    std::thread::spawn(move || {
                        let _ = handle(conn);
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    fn proxy_forwarder() {
        let listener = loop {
            match TcpListener::bind(GUEST_PROXY) {
                Ok(l) => break l,
                Err(_) => {
                    loopback_up();
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };
        for conn in listener.incoming().flatten() {
            std::thread::spawn(move || {
                if let Ok(up) = vsock_connect_host(EGRESS_PORT) {
                    splice(conn, up);
                }
            });
        }
    }

    fn splice(tcp: TcpStream, vs: File) {
        let (mut t1, mut v1) = match (tcp.try_clone(), vs.try_clone()) {
            (Ok(t), Ok(v)) => (t, v),
            _ => return,
        };
        let (mut t2, mut v2) = (tcp, vs);
        let a = std::thread::spawn(move || {
            let _ = std::io::copy(&mut t1, &mut v1);
            // SAFETY: half-close the vsock side so the host sees EOF.
            unsafe { libc::shutdown(std::os::fd::AsRawFd::as_raw_fd(&v1), libc::SHUT_WR) };
        });
        let _ = std::io::copy(&mut v2, &mut t2);
        let _ = t2.shutdown(std::net::Shutdown::Write);
        let _ = a.join();
    }

    fn handle(conn: File) -> std::io::Result<()> {
        let mut reader = BufReader::new(conn.try_clone()?).take(MAX_REQUEST);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(req),
            Err(e) => Response::err(format!("bad request: {e}")),
        };
        let mut out = serde_json::to_vec(&resp).unwrap_or_default();
        out.push(b'\n');
        let mut w = conn;
        w.write_all(&out)?;
        w.flush()
    }

    fn dispatch(req: Request) -> Response {
        // While the owner has the browser, nothing but the browser's own operations runs:
        // no commands, no file access that could read the page or its profile.
        if crate::browser::takeover_active()
            && matches!(
                req,
                Request::Exec { .. } | Request::Put { .. } | Request::Get { .. }
            )
        {
            return Response::err(format!(
                "{TAKEOVER_IN_PROGRESS}: the owner is using the browser"
            ));
        }
        match req {
            Request::Browser { browser } => crate::browser::dispatch(browser),
            Request::Hello {
                entropy_b64,
                now_ms,
                ca_pem,
            } => hello(&entropy_b64, now_ms, ca_pem),
            Request::Exec {
                argv,
                env,
                cwd,
                stdin_b64,
                timeout_ms,
                max_output,
            } => exec(argv, env, cwd, stdin_b64, timeout_ms, max_output),
            Request::Put { path, data_b64 } => match B64.decode(data_b64) {
                Ok(data) => {
                    let p = std::path::Path::new(&path);
                    if let Some(dir) = p.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    match std::fs::write(p, data) {
                        Ok(()) => Response {
                            ok: true,
                            ..Default::default()
                        },
                        Err(e) => Response::err(e),
                    }
                }
                Err(e) => Response::err(e),
            },
            Request::Get { path, max_bytes } => match File::open(&path) {
                Ok(f) => {
                    let mut data = Vec::new();
                    match f.take(max_bytes as u64 + 1).read_to_end(&mut data) {
                        Ok(_) if data.len() > max_bytes => Response::err("file too large"),
                        Ok(_) => Response {
                            ok: true,
                            data_b64: B64.encode(data),
                            ..Default::default()
                        },
                        Err(e) => Response::err(e),
                    }
                }
                Err(e) => Response::err(e),
            },
        }
    }

    fn hello(entropy_b64: &str, now_ms: i64, ca_pem: Option<String>) -> Response {
        // A VM restored from a shared template starts with the template's RNG state; mix in
        // host entropy before anything else runs.
        if let Ok(e) = B64.decode(entropy_b64) {
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/urandom") {
                let _ = f.write_all(&e);
            }
        }
        let ts = libc::timespec {
            tv_sec: now_ms / 1000,
            tv_nsec: (now_ms % 1000) * 1_000_000,
        };
        // SAFETY: valid timespec.
        unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) };
        // The trust bundle tools are pointed at always exists: the image's roots, plus the
        // lease CA when the host sends one.
        let _ = std::fs::create_dir_all("/run/zoen");
        if !std::path::Path::new(GUEST_TRUST_BUNDLE).exists() || ca_pem.is_some() {
            let system = std::fs::read_to_string(SYSTEM_BUNDLE).unwrap_or_default();
            let bundle = match &ca_pem {
                Some(ca) => format!("{system}\n{ca}"),
                None => system,
            };
            if std::fs::write(GUEST_TRUST_BUNDLE, bundle).is_err() {
                return Response::err("could not write the trust bundle");
            }
        }
        if let Some(ca) = ca_pem {
            if std::fs::write(GUEST_CA_PATH, &ca).is_err() {
                return Response::err("could not write the lease CA");
            }
            // Tools that ignore SSL_CERT_FILE read the system bundle; the root filesystem is
            // read-only, so the bundle is bind-mounted over it (once per VM).
            let mounted = std::fs::read_to_string("/proc/self/mountinfo")
                .map(|m| m.contains(SYSTEM_BUNDLE))
                .unwrap_or(false);
            if !mounted {
                mount(
                    GUEST_TRUST_BUNDLE,
                    SYSTEM_BUNDLE,
                    "",
                    libc::MS_BIND as libc::c_ulong,
                    "",
                );
            }
        }
        Response {
            ok: true,
            ..Default::default()
        }
    }

    fn exec(
        argv: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        stdin_b64: Option<String>,
        timeout_ms: u64,
        max_output: usize,
    ) -> Response {
        let Some((prog, args)) = argv.split_first() else {
            return Response::err("empty argv");
        };
        let proxy = format!("http://{GUEST_PROXY}");
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .env_clear()
            .env("PATH", BASE_PATH)
            .env("HOME", "/root")
            .env("HTTP_PROXY", &proxy)
            .env("HTTPS_PROXY", &proxy)
            .env("http_proxy", &proxy)
            .env("https_proxy", &proxy)
            .env("NO_PROXY", "localhost,127.0.0.1")
            .env("SSL_CERT_FILE", GUEST_TRUST_BUNDLE)
            .env("CURL_CA_BUNDLE", GUEST_TRUST_BUNDLE)
            .env("REQUESTS_CA_BUNDLE", GUEST_TRUST_BUNDLE)
            .env("NODE_EXTRA_CA_CERTS", GUEST_TRUST_BUNDLE)
            .envs(env)
            .current_dir(cwd.as_deref().unwrap_or(GUEST_WORK))
            .stdin(if stdin_b64.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let started = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return Response::err(format!("spawn {prog}: {e}")),
        };
        let pgid = child.id() as i32;
        if let (Some(mut si), Some(data)) = (child.stdin.take(), stdin_b64) {
            let data = B64.decode(data).unwrap_or_default();
            std::thread::spawn(move || {
                let _ = si.write_all(&data);
            });
        }
        let out = child.stdout.take().map(|s| reader(s, max_output));
        let err = child.stderr.take().map(|s| reader(s, max_output));
        let deadline = started + Duration::from_millis(timeout_ms);
        let (code, timed_out) = loop {
            match child.try_wait() {
                Ok(Some(st)) => break (st.code(), false),
                Ok(None) if Instant::now() >= deadline => {
                    // SAFETY: signal the whole process group we created.
                    unsafe { libc::kill(-pgid, libc::SIGKILL) };
                    let _ = child.wait();
                    break (None, true);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(e) => return Response::err(e),
            }
        };
        // Leftover background processes would hold the pipes open.
        // SAFETY: as above.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
        Response {
            ok: true,
            exit_code: code,
            timed_out,
            stdout_b64: B64.encode(stdout),
            stderr_b64: B64.encode(stderr),
            ..Default::default()
        }
    }

    fn reader(mut s: impl Read + Send + 'static, max: usize) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut buf = [0u8; 16 * 1024];
            let mut cut = false;
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let room = max.saturating_sub(kept.len());
                kept.extend_from_slice(&buf[..n.min(room)]);
                cut |= n > room;
            }
            if cut {
                kept.extend_from_slice(b"\n[cut]");
            }
            kept
        })
    }
}
