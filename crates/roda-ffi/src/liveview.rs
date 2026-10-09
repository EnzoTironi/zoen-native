//! The phone's half of an agent's browser live view and takeover (ADR 0028 §7).
//!
//! Frames come down sealed by the browser microVM to this device's live-view key; the
//! phone opens them here, so nothing in between (sandbox node, relay, model) sees the
//! screen. Takeover input goes up sealed the same way, and `Done` is the only thing that
//! hands the browser back to the agent.
//!
//! `LiveViewDemoVm` plays the VM's half with the same crypto for the showcase build, until
//! the relay carries real sessions to the phone.

use std::sync::Mutex;

use zoen_liveview::{DeviceKey, Direction, InputEvent, Offer, Opener, Sealer, SESSION_ID_LEN};

use crate::CoreError;

fn bad(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.to_string(),
    }
}

fn arr32(b: &[u8], what: &str) -> Result<[u8; 32], CoreError> {
    b.try_into().map_err(|_| bad(what))
}

/// This device's long-lived live-view key. The secret half stays in the keychain.
#[derive(uniffi::Object)]
pub struct LiveViewKey {
    key: DeviceKey,
}

#[uniffi::export]
impl LiveViewKey {
    /// A new key (first launch).
    #[uniffi::constructor]
    pub fn generate() -> Self {
        LiveViewKey {
            key: DeviceKey::generate(),
        }
    }

    /// The key kept in the keychain.
    #[uniffi::constructor]
    pub fn restore(secret: Vec<u8>) -> Result<Self, CoreError> {
        Ok(LiveViewKey {
            key: DeviceKey::from_secret(arr32(&secret, "live view key")?),
        })
    }

    pub fn secret(&self) -> Vec<u8> {
        self.key.secret().to_vec()
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.key.public().to_vec()
    }

    /// Opens a session the VM offered (its public key and session id).
    pub fn accept(
        &self,
        vm_pub: Vec<u8>,
        session: Vec<u8>,
    ) -> Result<std::sync::Arc<LiveViewSession>, CoreError> {
        let session: [u8; SESSION_ID_LEN] = session
            .as_slice()
            .try_into()
            .map_err(|_| bad("live view session"))?;
        let keys = self.key.accept(&Offer {
            vm_pub: arr32(&vm_pub, "live view offer")?,
            session,
        });
        Ok(std::sync::Arc::new(LiveViewSession {
            frames: Mutex::new(keys.opener(Direction::Frames)),
            input: Mutex::new(keys.sealer(Direction::Input)),
        }))
    }
}

/// One live session on the phone: opens frames, seals takeover input.
#[derive(uniffi::Object)]
pub struct LiveViewSession {
    frames: Mutex<Opener>,
    input: Mutex<Sealer>,
}

impl LiveViewSession {
    fn seal(&self, ev: &InputEvent) -> Vec<u8> {
        let json = serde_json::to_vec(ev).expect("input events serialize");
        self.input.lock().expect("live view input").seal(&json)
    }
}

#[uniffi::export]
impl LiveViewSession {
    /// A frame (JPEG bytes) or an error for anything forged, replayed or out of order.
    pub fn open_frame(&self, sealed: Vec<u8>) -> Result<Vec<u8>, CoreError> {
        self.frames
            .lock()
            .expect("live view frames")
            .open(&sealed)
            .map_err(|e| bad(&e.to_string()))
    }

    /// The number of the last frame shown (drops anything not newer).
    pub fn last_frame(&self) -> u64 {
        self.frames.lock().expect("live view frames").last_seq()
    }

    pub fn seal_text(&self, text: String) -> Vec<u8> {
        self.seal(&InputEvent::Text { text })
    }

    pub fn seal_click(&self, x: f64, y: f64) -> Vec<u8> {
        self.seal(&InputEvent::Click { x, y })
    }

    pub fn seal_key(&self, key: String) -> Vec<u8> {
        self.seal(&InputEvent::Key { key })
    }

    /// The owner is finished: the agent may continue.
    pub fn seal_done(&self) -> Vec<u8> {
        self.seal(&InputEvent::Done)
    }
}

/// What the demo VM read from one sealed input message.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum LiveViewInput {
    /// Text the owner typed. The VM keeps it away from the model; here only its length.
    Text {
        chars: u32,
    },
    Click {
        x: f64,
        y: f64,
    },
    Key {
        key: String,
    },
    Done,
}

/// The VM's half, for the showcase build (same crypto as `zoen-guestd`).
#[derive(uniffi::Object)]
pub struct LiveViewDemoVm {
    vm_pub: [u8; 32],
    session: [u8; SESSION_ID_LEN],
    frames: Mutex<Sealer>,
    input: Mutex<Opener>,
}

#[uniffi::export]
impl LiveViewDemoVm {
    #[uniffi::constructor]
    pub fn start(device_pub: Vec<u8>) -> Result<Self, CoreError> {
        let (offer, keys) = zoen_liveview::vm_start(&arr32(&device_pub, "device key")?);
        Ok(LiveViewDemoVm {
            vm_pub: offer.vm_pub,
            session: offer.session,
            frames: Mutex::new(keys.sealer(Direction::Frames)),
            input: Mutex::new(keys.opener(Direction::Input)),
        })
    }

    pub fn vm_pub(&self) -> Vec<u8> {
        self.vm_pub.to_vec()
    }

    pub fn session(&self) -> Vec<u8> {
        self.session.to_vec()
    }

    pub fn seal_frame(&self, jpeg: Vec<u8>) -> Vec<u8> {
        self.frames.lock().expect("demo frames").seal(&jpeg)
    }

    pub fn open_input(&self, sealed: Vec<u8>) -> Result<LiveViewInput, CoreError> {
        let pt = self
            .input
            .lock()
            .expect("demo input")
            .open(&sealed)
            .map_err(|e| bad(&e.to_string()))?;
        let ev: InputEvent = serde_json::from_slice(&pt).map_err(|_| bad("live view input"))?;
        Ok(match ev {
            InputEvent::Text { text } => LiveViewInput::Text {
                chars: text.chars().count() as u32,
            },
            InputEvent::Click { x, y } => LiveViewInput::Click { x, y },
            InputEvent::Key { key } => LiveViewInput::Key { key },
            InputEvent::ClickSelector { .. } => LiveViewInput::Key {
                key: "select".into(),
            },
            InputEvent::Done => LiveViewInput::Done,
        })
    }
}
