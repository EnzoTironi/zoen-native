#[test]
fn embedded_migration_versions_are_unique() {
    let mut descriptions = std::collections::BTreeMap::new();
    for migration in zoen_relay::MIGRATIONS.iter() {
        let previous = descriptions.insert(migration.version, &migration.description);
        assert!(
            previous.is_none(),
            "migration version {} is shared by {:?} and {}; assign the new migration an unused version",
            migration.version,
            previous,
            migration.description
        );
    }
}
