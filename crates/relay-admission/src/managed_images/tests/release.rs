use super::*;
use crate::managed_images::faults::{fail, Point};

#[test]
fn failed_prepare_can_reconcile_and_release_the_same_writer() -> TestResult {
    for point in [Point::Write, Point::FileSync, Point::DirectorySync] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create(&home.0, deployment())?;
        let before = writer.reader().checkpoint()?;
        fail(point);
        assert_eq!(
            writer.prepare(&effect(), &before, &before, true),
            Err(AnchorError::Unavailable)
        );
        assert_eq!(
            writer.reader().generation_guard(&before).err(),
            Some(AnchorError::Busy)
        );
        let prepared = writer.pending()?.is_some();
        assert_eq!(prepared, point == Point::DirectorySync);
        assert_eq!(
            writer.reconcile(&before, &before)?,
            if prepared {
                Recovery::BothBefore
            } else {
                Recovery::Clean
            }
        );
        assert_eq!(writer.pending()?, None);
        assert_eq!(writer.reader().checkpoint()?, before);
        let live = writer.reader().generation_guard(&before)?;
        assert_eq!(
            writer.prepare(&effect(), &before, &before, true),
            Err(AnchorError::Busy)
        );
        drop(live);
        writer.prepare(&effect(), &before, &before, true)?;
    }
    Ok(())
}

#[test]
fn retirement_failure_keeps_fence_until_same_writer_durable_recovery() -> TestResult {
    for point in [
        Point::Write,
        Point::FileSync,
        Point::DirectorySync,
        Point::Unlock,
    ] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create(&home.0, deployment())?;
        let before = writer.reader().checkpoint()?;
        let retirement = writer.prepare(&effect(), &before, &before, true)?;
        fail(point);
        assert_eq!(
            writer.reconcile(&retirement.next, &retirement.next),
            Err(AnchorError::Unavailable),
            "fault {point:?} must not acknowledge retirement"
        );
        assert_eq!(
            writer.reader().generation_guard(&before).err(),
            Some(AnchorError::Busy),
            "fault {point:?} must retain the old-generation fence"
        );
        assert_eq!(
            writer.reader().generation_guard(&retirement.next).err(),
            Some(AnchorError::Busy)
        );
        let committed = matches!(point, Point::DirectorySync | Point::Unlock);
        assert_eq!(writer.pending()?.is_none(), committed);
        if committed {
            assert_eq!(writer.reader().checkpoint()?, retirement.next);
            // A rename followed by an fsync failure is visible but not yet a
            // durable acknowledgement. Clean recovery must retry that boundary.
            fail(Point::DirectorySync);
            assert_eq!(
                writer.reconcile(&retirement.next, &retirement.next),
                Err(AnchorError::Unavailable)
            );
            assert_eq!(
                writer.reader().generation_guard(&retirement.next).err(),
                Some(AnchorError::Busy)
            );
        }
        assert_eq!(
            writer.reconcile(&retirement.next, &retirement.next)?,
            if committed {
                Recovery::Clean
            } else {
                Recovery::BothAfter
            }
        );
        assert_eq!(writer.reader().checkpoint()?, retirement.next);
        assert_eq!(
            writer.reader().generation_guard(&before).err(),
            Some(AnchorError::Unreconciled)
        );
        let current = writer.reader().generation_guard(&retirement.next)?;
        assert_eq!(
            writer.prepare(&effect(), &retirement.next, &retirement.next, true),
            Err(AnchorError::Busy),
            "the new live effect still excludes another retirement"
        );
        drop(current);
        writer.prepare(&effect(), &retirement.next, &retirement.next, true)?;
    }
    Ok(())
}
