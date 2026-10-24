//! Readiness of this watcher from its own startup and latest connection observation.
pub(crate) fn ready(stdout: &str, stderr: &str, who: &str) -> bool {
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
