use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Default)]
struct State {
    objects: Mutex<HashMap<String, Vec<u8>>>,
    stall: Mutex<Option<&'static str>>,
    calls: Mutex<Vec<(String, String)>>,
    completed: Mutex<Vec<(String, String)>>,
    stop: AtomicBool,
}

/// A local S3 endpoint whose selected requests wait for explicit release. A PUT can
/// finish after the relay has timed out, matching an unknown cloud write outcome.
pub struct MockS3 {
    pub endpoint: String,
    state: Arc<State>,
    server: Option<JoinHandle<()>>,
}

impl MockS3 {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(State::default());
        let worker_state = state.clone();
        let server = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !worker_state.stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let state = worker_state.clone();
                        workers.push(std::thread::spawn(move || serve(socket, state)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("mock S3 listener: {e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            endpoint,
            state,
            server: Some(server),
        }
    }

    pub fn stall(&self, method: Option<&'static str>) {
        *self.state.stall.lock().unwrap() = method;
    }

    pub fn bytes(&self, key: &str) -> Vec<u8> {
        self.state.objects.lock().unwrap()[key].clone()
    }

    pub fn call_count(&self) -> usize {
        self.state.calls.lock().unwrap().len()
    }

    pub async fn wait_for_call(&self, method: &str, after: usize) -> String {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some((_, key)) = self.state.calls.lock().unwrap()[after..]
                    .iter()
                    .find(|(m, _)| m == method)
                {
                    return key.clone();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("mock S3 did not receive the expected request")
    }

    pub async fn wait_for_completion(&self, method: &str, key: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if self
                    .state
                    .completed
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(m, k)| m == method && k == key)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("mock S3 did not finish the released request");
    }
}

fn wait(state: &State, method: &str) -> bool {
    while state.stall.lock().unwrap().as_deref() == Some(method) {
        if state.stop.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    !state.stop.load(Ordering::SeqCst)
}

fn serve(mut socket: TcpStream, state: Arc<State>) {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];
    let split = loop {
        let Ok(n) = socket.read(&mut chunk) else {
            return;
        };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&chunk[..n]);
        if let Some(split) = request.windows(4).position(|b| b == b"\r\n\r\n") {
            break split;
        }
        assert!(request.len() < 64 * 1024, "oversized mock S3 headers");
    };
    let headers = std::str::from_utf8(&request[..split]).unwrap();
    let mut first = headers.lines().next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_string();
    let key = first
        .next()
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .strip_prefix("/backup-test/")
        .expect("mock S3 bucket")
        .to_string();
    let length: usize = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse().unwrap())
        .unwrap_or(0);
    while request.len() < split + 4 + length {
        let Ok(n) = socket.read(&mut chunk) else {
            return;
        };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&chunk[..n]);
    }
    state
        .calls
        .lock()
        .unwrap()
        .push((method.clone(), key.clone()));
    if !wait(&state, &method) {
        return;
    }
    let (status, body) = match method.as_str() {
        "PUT" => {
            state
                .objects
                .lock()
                .unwrap()
                .insert(key.clone(), request[split + 4..split + 4 + length].to_vec());
            (200, Vec::new())
        }
        "GET" | "HEAD" => match state.objects.lock().unwrap().get(&key) {
            Some(body) => (200, body.clone()),
            None => (404, b"<Error><Code>NoSuchKey</Code></Error>".to_vec()),
        },
        "DELETE" => {
            state.objects.lock().unwrap().remove(&key);
            (204, Vec::new())
        }
        _ => panic!("unexpected mock S3 method {method}"),
    };
    let headers = format!(
        "HTTP/1.1 {status} mock\r\nContent-Length: {}\r\nETag: \"mock\"\r\nLast-Modified: Wed, 01 Jan 2025 00:00:00 GMT\r\nConnection: close\r\n\r\n",
        body.len(),
    );
    let _ = socket.write_all(headers.as_bytes());
    if method == "GET" && !wait(&state, "GET_BODY") {
        return;
    }
    if method != "HEAD" {
        let _ = socket.write_all(&body);
    }
    state.completed.lock().unwrap().push((method, key));
}

impl Drop for MockS3 {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        if let Some(server) = self.server.take() {
            let result = server.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}
