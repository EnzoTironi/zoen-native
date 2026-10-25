use super::*;

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("renewal-journal")?);
        std::fs::create_dir(&path).unwrap();
        Ok(Self(path))
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("steps.json")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn absent_or_legacy_data_never_proves_the_held_call_did_not_execute(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = Directory::new()?;
    let path = dir.path();
    let hash = "ab".repeat(32);
    assert!(!JournalState::renewal_safe(&path, "root", &hash, "source").unwrap());
    JournalState::begin_resume(&path, "root", &hash, Some("source")).unwrap();
    let legacy = std::fs::read(&path).unwrap();
    JournalState::register_hold(&path, "root", &hash, "source").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), legacy);
    assert!(!JournalState::renewal_safe(&path, "root", &hash, "source").unwrap());
    assert!(!String::from_utf8(legacy).unwrap().contains("registered"));
    Ok(())
}

#[test]
fn before_executor_witness_and_operational_budget_survive_cold_journal_reads(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = Directory::new()?;
    let path = dir.path();
    let hash = "ab".repeat(32);
    JournalState::register_hold(&path, "root", &hash, "source").unwrap();
    assert_eq!(
        JournalState::load(&path).unwrap().continuations["root"].attempts,
        0
    );
    assert!(JournalState::renewal_safe(&path, "root", &hash, "source").unwrap());
    assert!(JournalState::begin_resume(&path, "root", &hash, Some("source")).unwrap());
    JournalState::entering_executor(&path, "root", &hash, "source").unwrap();
    let held = JournalState::load(&path).unwrap();
    assert!(held.continuations["root"].outcome_unknown);
    assert_eq!(held.continuations["root"].attempts, 1);
    assert!(!JournalState::renewal_safe(&path, "root", &hash, "source").unwrap());
    JournalState::register_hold(&path, "root", &hash, "source").unwrap();
    assert!(!JournalState::renewal_safe(&path, "root", &hash, "source").unwrap());
    for _ in 1..MAX_TRIES {
        assert!(JournalState::begin_resume(&path, "root", &hash, Some("source")).unwrap());
    }
    assert!(!JournalState::begin_resume(&path, "root", &hash, Some("source")).unwrap());
    assert_eq!(JournalState::load(&path).unwrap().continuations.len(), 1);
    Ok(())
}

#[test]
fn wrong_binding_interrupted_writer_and_corrupt_optional_witness_refuse_without_overwriting(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = Directory::new()?;
    let path = dir.path();
    let hash = "ab".repeat(32);
    JournalState::register_hold(&path, "root", &hash, "source").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(JournalState::register_hold(&path, "root", &"cd".repeat(32), "source").is_err());
    assert!(!JournalState::renewal_safe(&path, "root", &hash, "other-source").unwrap());
    std::fs::write(path.with_extension("tmp"), b"owned interrupted update").unwrap();
    assert!(JournalState::renewal_safe(&path, "root", &hash, "source").is_err());
    assert!(JournalState::entering_executor(&path, "root", &hash, "source").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(path.with_extension("tmp")).unwrap();
    for field in ["registered", "outcome_unknown"] {
        let mut corrupt: serde_json::Value = serde_json::from_slice(&before).unwrap();
        corrupt["continuations"]["root"][field] = "true".into();
        std::fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
        assert!(JournalState::load(&path).is_err());
        assert!(JournalState::renewal_safe(&path, "root", &hash, "source").is_err());
    }
    Ok(())
}
