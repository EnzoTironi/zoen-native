use super::{LiveViewDemoVm, LiveViewInput, LiveViewKey};

#[test]
fn liveview_round_trip_and_replay_preserve_direction_and_sequences() {
    let key = LiveViewKey::generate().unwrap();
    let restored = LiveViewKey::restore(key.secret()).unwrap();
    assert_eq!(restored.public_key(), key.public_key());
    let vm = LiveViewDemoVm::start(key.public_key()).unwrap();
    let session = restored.accept(vm.vm_pub(), vm.session()).unwrap();
    assert_eq!(session.last_frame().unwrap(), 0);
    let frame = vm.seal_frame(b"jpeg bytes".to_vec()).unwrap();
    assert_eq!(session.open_frame(frame.clone()).unwrap(), b"jpeg bytes");
    assert_eq!(session.last_frame().unwrap(), 1);
    assert!(session.open_frame(frame.clone()).is_err());
    assert!(vm.open_input(frame).is_err());
    let text = session.seal_text("Olá 🌥".into()).unwrap();
    assert_eq!(
        vm.open_input(text.clone()).unwrap(),
        LiveViewInput::Text { chars: 5 }
    );
    assert!(vm.open_input(text).is_err());
    let click = session.seal_click(1.5, 2.5).unwrap();
    assert_eq!(
        vm.open_input(click).unwrap(),
        LiveViewInput::Click { x: 1.5, y: 2.5 }
    );
    let key = session.seal_key("Enter".into()).unwrap();
    assert_eq!(
        vm.open_input(key).unwrap(),
        LiveViewInput::Key {
            key: "Enter".into()
        }
    );
    assert_eq!(
        vm.open_input(session.seal_done().unwrap()).unwrap(),
        LiveViewInput::Done
    );
}

#[test]
fn invalid_keys_and_coordinates_refuse_before_publishing_input() {
    let key = LiveViewKey::generate().unwrap();
    assert!(key.accept(vec![0; 32], vec![8; 16]).is_err());
    assert!(LiveViewDemoVm::start(vec![0; 32]).is_err());
    assert!(LiveViewKey::restore(vec![0; 31]).is_err());
    let vm = LiveViewDemoVm::start(key.public_key()).unwrap();
    let session = key.accept(vm.vm_pub(), vm.session()).unwrap();
    assert!(session.seal_click(f64::NAN, 1.0).is_err());
    assert!(session.seal_click(1.0, f64::INFINITY).is_err());
    let done = session.seal_done().unwrap();
    assert_eq!(&done[..8], &1_u64.to_be_bytes());
    assert_eq!(vm.open_input(done).unwrap(), LiveViewInput::Done);
}

#[test]
fn poisoned_phone_ciphers_refuse_instead_of_reusing_untrusted_state() {
    let key = LiveViewKey::generate().unwrap();
    let vm = LiveViewDemoVm::start(key.public_key()).unwrap();
    let session = key.accept(vm.vm_pub(), vm.session()).unwrap();
    let poisoned = std::panic::catch_unwind(|| {
        let _guard = session.input.lock().unwrap();
        panic!("actual unwind while owning input counter");
    });
    assert!(poisoned.is_err());
    assert!(session.seal_done().is_err());
    let poisoned = std::panic::catch_unwind(|| {
        let _guard = session.frames.lock().unwrap();
        panic!("actual unwind while owning replay cursor");
    });
    assert!(poisoned.is_err());
    assert!(session.last_frame().is_err());
    assert!(session.open_frame(vm.seal_frame(vec![1]).unwrap()).is_err());
}

#[test]
fn poisoned_vm_ciphers_refuse_without_resetting_counters() {
    let key = LiveViewKey::generate().unwrap();
    let vm = LiveViewDemoVm::start(key.public_key()).unwrap();
    let session = key.accept(vm.vm_pub(), vm.session()).unwrap();
    assert!(std::panic::catch_unwind(|| {
        let _guard = vm.frames.lock().unwrap();
        panic!("actual unwind while owning frame counter");
    })
    .is_err());
    assert!(vm.seal_frame(vec![1]).is_err());
    assert!(std::panic::catch_unwind(|| {
        let _guard = vm.input.lock().unwrap();
        panic!("actual unwind while owning input cursor");
    })
    .is_err());
    assert!(vm.open_input(session.seal_done().unwrap()).is_err());
}
