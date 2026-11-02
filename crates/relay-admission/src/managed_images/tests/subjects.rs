use super::*;

fn configured() -> RuntimeDeployment {
    RuntimeDeployment {
        namespace: "subjects".into(),
        witness: "a".repeat(64),
        source: "cell".into(),
    }
}
fn update(subject: u8, previous: Option<&str>, next: u8) -> SubjectUpdate {
    SubjectUpdate {
        subject: format!("{subject:064x}"),
        previous: previous.map(str::to_owned),
        next: format!("{next:064x}"),
    }
}

#[test]
fn atomic_subject_commitments_survive_restart_and_bind_every_later_checkpoint() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create_runtime(&home.0, configured())?;
    let before = writer.reader().checkpoint()?;
    let one = update(1, None, 2);
    writer.reader().check_subject(&one.subject, None)?;
    let prepared = writer.prepare_subjects(
        &effect(),
        &before,
        &before,
        false,
        std::slice::from_ref(&one),
        Some("opaque-retained-ciphertext"),
    )?;
    assert!(writer.reader().check_subject(&one.subject, None).is_err());
    assert_eq!(
        writer
            .reader()
            .pending_subject_commitments(&prepared)?
            .get(&one.subject),
        Some(&one.next)
    );
    assert_eq!(writer.reconcile(&before, &before)?, Recovery::BothBefore);
    assert_eq!(
        writer.pending()?,
        Some(prepared.clone()),
        "absence cannot erase the original subject intent"
    );
    writer.reconcile(&prepared.next, &prepared.next)?;
    writer
        .reader()
        .check_subject(&one.subject, Some(&one.next))?;
    assert!(writer.reader().check_subject(&one.subject, None).is_err());
    let second = update(3, None, 4);
    let next = writer.prepare_subjects(
        &effect(),
        &prepared.next,
        &prepared.next,
        false,
        std::slice::from_ref(&second),
        None,
    )?;
    writer.reconcile(&next.next, &next.next)?;
    let plain = writer.prepare(&effect(), &next.next, &next.next, false)?;
    assert_eq!(plain.subjects_after, next.subjects_after);
    writer.reconcile(&plain.next, &plain.next)?;
    drop(writer);
    let mut cold = AnchorWriter::open_runtime(&home.0, configured())?;
    cold.reader().check_subject(&one.subject, Some(&one.next))?;
    cold.reader()
        .check_subject(&second.subject, Some(&second.next))?;
    assert_eq!(cold.reader().subject_commitments()?.len(), 2);
    assert_eq!(
        cold.reader().retained_checkpoint(&prepared.next)?.intent,
        prepared.intent
    );
    assert_eq!(
        cold.reconcile(&before, &before),
        Err(AnchorError::Unreconciled)
    );
    Ok(())
}

#[test]
fn stale_updates_duplicate_subjects_and_capacity_errors_do_not_prepare_state() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create_runtime(&home.0, configured())?;
    let before = writer.reader().checkpoint()?;
    let one = update(1, None, 2);
    for updates in [
        vec![update(1, Some(&"a".repeat(64)), 2)],
        vec![one.clone(), one.clone()],
        (0..9).map(|id| update(id, None, id + 20)).collect(),
    ] {
        assert_eq!(
            writer.prepare_subjects(&effect(), &before, &before, false, &updates, None),
            Err(AnchorError::Invalid)
        );
        assert_eq!(writer.pending()?, None);
    }
    assert_eq!(
        writer.prepare_subjects(
            &effect(),
            &before,
            &before,
            false,
            &[one],
            Some(&"x".repeat(16_385))
        ),
        Err(AnchorError::Invalid)
    );
    let map: std::collections::BTreeMap<_, _> = (0..512)
        .map(|id| (format!("{id:064x}"), "b".repeat(64)))
        .collect();
    assert!(super::super::subjects::apply(
        &map,
        &[SubjectUpdate {
            subject: format!("{:064x}", 512),
            previous: None,
            next: "c".repeat(64)
        }]
    )
    .is_none());
    let legacy = Home::new()?;
    let mut old = AnchorWriter::create(&legacy.0, deployment())?;
    let cp = old.reader().checkpoint()?;
    assert_eq!(
        old.prepare_subjects(&effect(), &cp, &cp, false, &[update(1, None, 2)], None),
        Err(AnchorError::Invalid)
    );
    Ok(())
}

#[test]
fn partial_commit_and_corrupted_latest_subject_cannot_be_treated_as_clean() -> TestResult {
    for sql_first in [false, true] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create_runtime(&home.0, configured())?;
        let before = writer.reader().checkpoint()?;
        let one = update(1, None, 2);
        let pending = writer.prepare_subjects(
            &effect(),
            &before,
            &before,
            false,
            std::slice::from_ref(&one),
            None,
        )?;
        drop(writer);
        let mut cold = AnchorWriter::open_runtime(&home.0, configured())?;
        let (sql, fdb, expected) = if sql_first {
            (&pending.next, &before, Recovery::FdbBefore)
        } else {
            (&before, &pending.next, Recovery::SqlBefore)
        };
        assert_eq!(cold.reconcile(sql, fdb)?, expected);
        assert!(cold.reader().subject_commitments().is_err());
        cold.reconcile(&pending.next, &pending.next)?;
        let path = home.0.join("checkpoint.json");
        let mut altered: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        altered["subjects"][&one.subject] = serde_json::Value::String("c".repeat(64));
        std::fs::write(path, serde_json::to_vec(&altered)?)?;
        assert_eq!(
            cold.reader()
                .check_subject(&one.subject, Some(&"c".repeat(64))),
            Err(AnchorError::Invalid)
        );
    }
    Ok(())
}
