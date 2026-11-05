use super::*;
use crate::inbox_folders::{InboxFoldersInput as Input, InboxFoldersScope as Scope};
use types::{InboxFolder, InboxFolderKind, InboxFolders};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn scope(f: &Fixture) -> Scope {
    Scope {
        expected_identity_id: f.identity.clone(),
        expected_device_id: f.device.clone(),
    }
}
fn configuration(f: &Fixture) -> InboxFolders {
    InboxFolders {
        folders: vec![InboxFolder {
            id: "folder:work".into(),
            name: "Work 🪁".into(),
            kinds: vec![InboxFolderKind::Group],
            include: vec![f.space.clone()],
            exclude: vec![],
            exclude_read: false,
            exclude_muted: false,
            exclude_archived: true,
        }],
        default_folder: "folder:work".into(),
    }
}
fn put(
    f: &Fixture,
    id: &str,
    revision: &str,
    configuration: InboxFolders,
) -> Result<crate::inbox_folders::InboxFoldersResult, CoreError> {
    f.client.lock().inbox_folders(&Input::Put {
        scope: scope(f),
        operation_id: id.into(),
        observed_revision: revision.into(),
        configuration,
    })
}
#[test]
fn folders_exact_retry_survives_later_edit_without_reverting_it() -> TestResult {
    let f = Fixture::new();
    let initial = f
        .client
        .lock()
        .inbox_folders(&Input::Read { scope: scope(&f) })?;
    assert_eq!(initial.revision, "0");
    let config = configuration(&f);
    let first = put(&f, "first", "0", config.clone())?;
    let second = put(&f, "second", &first.revision, InboxFolders::default())?;
    let before = f.client.lock().store.event_count()?;
    let retry = put(&f, "first", "0", config.clone())?;
    assert_eq!(retry.applied_revision, Some(first.revision));
    assert_eq!(retry.revision, second.revision);
    assert!(retry.configuration.folders.is_empty());
    assert_eq!(f.client.lock().store.event_count()?, before);
    assert!(matches!(
        put(&f, "third", "0", config),
        Err(CoreError::Stale { .. })
    ));
    assert!(put(&f, "first", "0", InboxFolders::default()).is_err());
    assert_eq!(f.client.lock().store.event_count()?, before);
    Ok(())
}
#[test]
fn folders_reject_invalid_bounds_duplicates_defaults_and_foreign_owner_without_effects(
) -> TestResult {
    let f = Fixture::new();
    let valid = configuration(&f);
    let mut variants = Vec::new();
    let mut c = valid.clone();
    c.folders[0].name = "x".repeat(33);
    variants.push(c);
    let mut c = valid.clone();
    c.folders[0].name = "bad\nname".into();
    variants.push(c);
    let mut c = valid.clone();
    c.folders[0].id = "all".into();
    variants.push(c);
    let mut c = valid.clone();
    c.folders.push(c.folders[0].clone());
    variants.push(c);
    let mut c = valid.clone();
    c.folders[0].kinds.push(InboxFolderKind::Group);
    variants.push(c);
    let mut c = valid.clone();
    c.default_folder = "folder:missing".into();
    variants.push(c);
    let mut c = valid.clone();
    c.folders[0].include = vec![f.space.clone(); 201];
    variants.push(c);
    let before = f.client.lock().store.event_count()?;
    for c in variants {
        assert!(put(&f, "invalid", "0", c).is_err());
    }
    let mut foreign = scope(&f);
    foreign.expected_identity_id = "other".into();
    assert!(f
        .client
        .lock()
        .inbox_folders(&Input::Read { scope: foreign })
        .is_err());
    let mut device = scope(&f);
    device.expected_device_id = "other".into();
    assert!(f
        .client
        .lock()
        .inbox_folders(&Input::Put {
            scope: device,
            operation_id: "foreign".into(),
            observed_revision: "0".into(),
            configuration: valid
        })
        .is_err());
    assert_eq!(f.client.lock().store.event_count()?, before);
    Ok(())
}
#[test]
fn folders_cold_reopen_restores_order_default_and_receipt_from_signed_log() -> TestResult {
    let directory =
        std::env::temp_dir().join(types::new_id("folders").map_err(|e| format!("id: {e:?}"))?);
    std::fs::create_dir(&directory)?;
    let path = directory
        .join("owned.sqlite")
        .to_string_lossy()
        .into_owned();
    let f = Fixture::at(path.clone());
    let config = configuration(&f);
    let receipt = put(&f, "saved", "0", config.clone())?;
    let owner = scope(&f);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    let recovered = client.lock().inbox_folders(&Input::Read {
        scope: owner.clone(),
    })?;
    assert_eq!(recovered.configuration, config);
    assert_eq!(recovered.revision, receipt.revision);
    let count = client.lock().store.event_count()?;
    let retry = client.lock().inbox_folders(&Input::Put {
        scope: owner,
        operation_id: "saved".into(),
        observed_revision: "0".into(),
        configuration: config,
    })?;
    assert_eq!(retry.applied_revision, Some(receipt.revision));
    assert_eq!(client.lock().store.event_count()?, count);
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn failed_folder_append_preserves_prior_state_and_corrupt_history_cannot_be_defaulted() -> TestResult
{
    let f = Fixture::new();
    let first = put(&f, "first", "0", configuration(&f))?;
    f.client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER fail_folders BEFORE INSERT ON events WHEN NEW.json LIKE '%InboxFoldersSet%' BEGIN SELECT RAISE(ABORT,'injected folder append failure'); END;")?;
    assert!(put(&f, "failed", &first.revision, InboxFolders::default()).is_err());
    let unchanged = f
        .client
        .lock()
        .inbox_folders(&Input::Read { scope: scope(&f) })?;
    assert_eq!(unchanged.configuration, first.configuration);
    assert_eq!(unchanged.revision, first.revision);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_folders;")?;
    {
        let mut engine = f.client.lock();
        let personal = engine.ensure_personal_space()?;
        engine.append(
            &personal,
            &f.identity,
            types::EventBody::InboxFoldersSet {
                configuration: InboxFolders::default(),
                operation_id: "bad-chain".into(),
                previous_revision: "0".into(),
            },
        )?;
    }
    assert!(
        f.client
            .lock()
            .inbox_folders(&Input::Read { scope: scope(&f) })
            .is_err(),
        "A changed log head must invalidate the cached state and expose a broken preference chain"
    );
    Ok(())
}

#[test]
fn folder_limits_accept_the_boundary_and_unknown_chat_references_grant_no_access() -> TestResult {
    let f = Fixture::new();
    let mut config = configuration(&f);
    let template = config.folders[0].clone();
    config.folders = (0..20)
        .map(|i| types::InboxFolder {
            id: format!("folder:{i}"),
            name: "x".repeat(32),
            include: (0..200).map(|n| format!("unknown:{i}:{n}")).collect(),
            ..template.clone()
        })
        .collect();
    config.default_folder = "folder:19".into();
    let result = put(&f, "boundary", "0", config.clone())?;
    assert_eq!(result.configuration, config);
    assert!(!f.client.lock().state.spaces.contains_key("unknown:0:0"));
    let mut too_many = config.clone();
    too_many.folders.push(types::InboxFolder {
        id: "folder:21".into(),
        ..template
    });
    assert!(put(&f, "overflow", &result.revision, too_many).is_err());
    {
        let mut engine = f.client.lock();
        engine.append(
            &f.space,
            &f.identity,
            types::EventBody::InboxFoldersSet {
                configuration: InboxFolders::default(),
                operation_id: "public-room".into(),
                previous_revision: result.revision.clone(),
            },
        )?;
    }
    assert_eq!(
        f.client
            .lock()
            .inbox_folders(&Input::Read { scope: scope(&f) })?
            .configuration,
        config,
        "An event outside the owner's Personal log cannot replace folder preferences"
    );
    Ok(())
}
