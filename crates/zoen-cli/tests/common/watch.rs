//! Owned background CLI watcher; readiness belongs to this process, never a relay total.
use super::World;
use std::{path::PathBuf, process::Child, time::Duration};

pub struct Watch {
    pub child: Child,
    pub log: PathBuf,
    pub status_log: PathBuf,
}

impl Watch {
    pub fn start(world: &World, who: &str, name: &str) -> Self {
        let log = world.dir.join(format!("{name}.stdout.log"));
        let status_log = world.dir.join(format!("{name}.stderr.log"));
        let child = world
            .cmd(who, &["watch", "--for", "90"])
            .env("ZOEN_NET_DEBUG", "1")
            .stdout(std::fs::File::create(&log).expect("watch stdout"))
            .stderr(std::fs::File::create(&status_log).expect("watch stderr"))
            .spawn()
            .expect("spawn watcher");
        Self {
            child,
            log,
            status_log,
        }
    }

    pub async fn ready(&mut self, who: &str) {
        self.until(&format!("@{who} online and synced"), |stdout, stderr| {
            ready(stdout, stderr, who)
        })
        .await;
    }

    pub async fn until(&mut self, expected: &str, observed: impl Fn(&str, &str) -> bool) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let stdout = std::fs::read_to_string(&self.log).expect("watch stdout");
            let stderr = std::fs::read_to_string(&self.status_log).expect("watch stderr");
            let log = format!("stdout:\n{stdout}\nstderr:\n{stderr}");
            let status = self.child.try_wait().expect("watch status");
            assert!(
                status.is_none(),
                "watch exited while waiting for {expected}: {status:?}\n{log}"
            );
            if observed(&stdout, &stderr) {
                return log;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "watch never observed {expected}:\n{log}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn ready(stdout: &str, stderr: &str, who: &str) -> bool {
    let expected = format!("watching as @{who} (Ctrl-C to stop)");
    stdout.lines().any(|line| line == expected)
        && stderr
            .lines()
            .rev()
            .find(|line| line.starts_with("[zoen-net] connection="))
            .is_some_and(|line| line.contains("connection=online synced=true"))
}

#[cfg(test)]
mod tests {
    use super::ready;

    #[test]
    fn readiness_requires_this_watcher_and_its_latest_connection() {
        let header = "watching as @ana (Ctrl-C to stop)\n";
        let online = "[zoen-net] connection=online synced=true pending=0\n";
        assert!(ready(header, online, "ana"));
        assert!(!ready(header, online, "bruno"));
        assert!(!ready(header, "", "ana"));
        assert!(!ready("", online, "ana"));
        assert!(!ready(
            header,
            &format!("{online}[zoen-net] connection=offline synced=false\n"),
            "ana"
        ));
        assert!(!ready(
            header,
            "[zoen-net] connection=online synced=false\n",
            "ana"
        ));
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
