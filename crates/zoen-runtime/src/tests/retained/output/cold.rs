//! Cold native custody and independent managed-anchor reopen.
use super::*;

pub(in crate::tests::retained) async fn cold(fixture: &mut Fixture) {
    let mut reopened = fixture.world.reopen().await;
    let custody = fixture.restore_custody();
    let capability = Arc::get_mut(&mut fixture.world.runtime)
        .unwrap()
        .output_fixture
        .take()
        .unwrap();
    let runtime = Arc::get_mut(&mut reopened).unwrap();
    runtime.native = Some(custody);
    // Move the original fixture capability. No store read reconstructs it.
    runtime.output_fixture = Some(capability);
    if let Some(directory) = &fixture.managed_directory {
        // Close the former host's actual writer lock before cold-opening the
        // retained file. Do not transfer an in-memory continuity capability.
        Arc::get_mut(&mut fixture.world.runtime)
            .unwrap()
            .managed_images = None;
        let actor = &fixture.world.agents[0];
        let runtime = match Arc::try_unwrap(reopened) {
            Ok(runtime) => runtime,
            Err(_) => panic!("cold fixture runtime unexpectedly shared"),
        };
        reopened = Arc::new(
            runtime
                .open_managed_image_continuity(
                    &directory.0,
                    &actor.identity,
                    actor.device.as_deref().unwrap(),
                )
                .await
                .unwrap(),
        );
    }
    fixture.world.runtime.finance.pool.close().await;
    fixture.world.runtime = reopened;
}
