#[test]
fn embedded_migration_versions_are_unique() {
    let migrations = sqlx::migrate!("./migrations");
    let mut descriptions = std::collections::BTreeMap::new();
    for migration in migrations.iter() {
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
