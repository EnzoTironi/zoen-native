//! # zoen-liveview
//!
//! The owner watches a browser microVM live and can take it over (login, 2FA, CAPTCHA). Both
//! directions are end-to-end encrypted between the VM and the owner's device; the sandbox
//! node, the relay and the model in between only ever carry ciphertext.
//!
//! - **Keys.** The device has a long-lived X25519 key (its public half is in the owner's
//!   device record). For each live session the VM makes a fresh X25519 key and a random
//!   session id. Both sides run HKDF-SHA256 over the shared secret, both public keys and the
//!   session id, and get two ChaCha20-Poly1305 keys: one for frames (VM → device), one for
//!   input (device → VM).
//! - **Frames** ([`Sealer`] / [`Opener`]): `seq (8 bytes, big-endian) ‖ ciphertext`. The
//!   nonce is the sequence number; the associated data is the session id, the direction and
//!   the sequence number. The device drops anything not newer than what it has shown.
//! - **Input** (the same types, the other key): device events — text, clicks, keys, and
//!   `Done`, which is the only way a takeover ends. The VM refuses replays (a sequence
//!   number not above the last one it accepted), anything from another session, and
//!   anything the host made up (it can't produce a valid tag).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

pub const VERSION: &[u8] = b"zoen-liveview/1";
pub const SESSION_ID_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Wrong key, wrong session, wrong direction or tampered bytes.
    Auth,
    /// Not newer than the last accepted message.
    Replay,
    Malformed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::Auth => "live view: authentication failed",
            Error::Replay => "live view: replayed or out-of-order message",
            Error::Malformed => "live view: malformed message",
        })
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// VM → device: screen frames.
    Frames,
    /// Device → VM: takeover input.
    Input,
}

impl Direction {
    fn label(self) -> &'static [u8] {
        match self {
            Direction::Frames => b"frames",
            Direction::Input => b"input",
        }
    }
}

/// The two directional keys of one session.
#[derive(Clone)]
pub struct SessionKeys {
    pub session: [u8; SESSION_ID_LEN],
    frames: [u8; 32],
    input: [u8; 32],
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKeys")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

fn derive(
    shared: &[u8; 32],
    vm_pub: &[u8; 32],
    device_pub: &[u8; 32],
    session: [u8; SESSION_ID_LEN],
) -> SessionKeys {
    let hk = Hkdf::<Sha256>::new(Some(VERSION), shared);
    let mut info = Vec::with_capacity(80);
    info.extend_from_slice(vm_pub);
    info.extend_from_slice(device_pub);
    info.extend_from_slice(&session);
    let mut okm = [0u8; 64];
    hk.expand(&info, &mut okm)
        .expect("64 bytes is a valid HKDF length");
    let mut frames = [0u8; 32];
    let mut input = [0u8; 32];
    frames.copy_from_slice(&okm[..32]);
    input.copy_from_slice(&okm[32..]);
    SessionKeys {
        session,
        frames,
        input,
    }
}

/// What the VM sends the device to open a session (in the clear: public values only).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Offer {
    pub vm_pub: [u8; 32],
    pub session: [u8; SESSION_ID_LEN],
}

/// VM side: a fresh key per session, agreed with the device's public key.
pub fn vm_start(device_pub: &[u8; 32]) -> (Offer, SessionKeys) {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("os randomness");
    let mut session = [0u8; SESSION_ID_LEN];
    getrandom::getrandom(&mut session).expect("os randomness");
    // Used once and dropped at the end of this function (an ephemeral key; x25519-dalek's
    // EphemeralSecret type needs a rand_core RNG, which getrandom stands in for here).
    let secret = StaticSecret::from(seed);
    let vm_pub = PublicKey::from(&secret).to_bytes();
    let shared = secret.diffie_hellman(&PublicKey::from(*device_pub));
    let keys = derive(shared.as_bytes(), &vm_pub, device_pub, session);
    (Offer { vm_pub, session }, keys)
}

/// The owner's device key.
pub struct DeviceKey {
    secret: StaticSecret,
}

impl DeviceKey {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("os randomness");
        DeviceKey {
            secret: StaticSecret::from(seed),
        }
    }

    pub fn public(&self) -> [u8; 32] {
        PublicKey::from(&self.secret).to_bytes()
    }

    /// Device side of [`vm_start`].
    pub fn accept(&self, offer: &Offer) -> SessionKeys {
        let shared = self.secret.diffie_hellman(&PublicKey::from(offer.vm_pub));
        derive(
            shared.as_bytes(),
            &offer.vm_pub,
            &self.public(),
            offer.session,
        )
    }
}

fn nonce(seq: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&seq.to_be_bytes());
    Nonce::from(n)
}

fn aad(session: &[u8; SESSION_ID_LEN], dir: Direction, seq: u64) -> Vec<u8> {
    let mut a = Vec::with_capacity(48);
    a.extend_from_slice(VERSION);
    a.extend_from_slice(session);
    a.extend_from_slice(dir.label());
    a.extend_from_slice(&seq.to_be_bytes());
    a
}

impl SessionKeys {
    fn key(&self, dir: Direction) -> &[u8; 32] {
        match dir {
            Direction::Frames => &self.frames,
            Direction::Input => &self.input,
        }
    }

    pub fn sealer(&self, dir: Direction) -> Sealer {
        Sealer {
            aead: ChaCha20Poly1305::new_from_slice(self.key(dir)).expect("32-byte key"),
            session: self.session,
            dir,
            next: 1,
        }
    }

    pub fn opener(&self, dir: Direction) -> Opener {
        Opener {
            aead: ChaCha20Poly1305::new_from_slice(self.key(dir)).expect("32-byte key"),
            session: self.session,
            dir,
            last: 0,
        }
    }
}

/// Encrypts one direction; sequence numbers start at 1 and only go up.
pub struct Sealer {
    aead: ChaCha20Poly1305,
    session: [u8; SESSION_ID_LEN],
    dir: Direction,
    next: u64,
}

impl Sealer {
    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let seq = self.next;
        self.next += 1;
        let ct = self
            .aead
            .encrypt(
                &nonce(seq),
                Payload {
                    msg: plaintext,
                    aad: &aad(&self.session, self.dir, seq),
                },
            )
            .expect("chacha20poly1305 encryption does not fail");
        let mut out = Vec::with_capacity(8 + ct.len());
        out.extend_from_slice(&seq.to_be_bytes());
        out.extend_from_slice(&ct);
        out
    }
}

/// Decrypts one direction and refuses anything not newer than the last accepted message.
pub struct Opener {
    aead: ChaCha20Poly1305,
    session: [u8; SESSION_ID_LEN],
    dir: Direction,
    last: u64,
}

impl Opener {
    pub fn open(&mut self, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        if sealed.len() < 8 + 16 {
            return Err(Error::Malformed);
        }
        let seq = u64::from_be_bytes(sealed[..8].try_into().unwrap());
        if seq <= self.last {
            return Err(Error::Replay);
        }
        let pt = self
            .aead
            .decrypt(
                &nonce(seq),
                Payload {
                    msg: &sealed[8..],
                    aad: &aad(&self.session, self.dir, seq),
                },
            )
            .map_err(|_| Error::Auth)?;
        self.last = seq;
        Ok(pt)
    }

    pub fn last_seq(&self) -> u64 {
        self.last
    }
}

/// What the owner's device sends during a takeover (inside an `Input` envelope).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum InputEvent {
    /// Types text into the focused element (a username, a password, a code).
    Text { text: String },
    /// A click at page coordinates (CSS pixels of the frame).
    Click { x: f64, y: f64 },
    /// A named key: `Enter`, `Tab`, `Backspace`, `Escape`, arrows.
    Key { key: String },
    /// Clicks the element matching a CSS selector (devices without a pointer, and tests).
    ClickSelector { selector: String },
    /// The owner is finished; the agent may continue.
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (SessionKeys, SessionKeys) {
        let device = DeviceKey::generate();
        let (offer, vm) = vm_start(&device.public());
        (vm, device.accept(&offer))
    }

    #[test]
    fn frames_reach_the_device_and_input_reaches_the_vm() {
        let (vm, dev) = pair();
        let mut s = vm.sealer(Direction::Frames);
        let mut o = dev.opener(Direction::Frames);
        let f = s.seal(b"\xff\xd8jpeg");
        assert!(!f.windows(4).any(|w| w == b"jpeg"));
        assert_eq!(o.open(&f).unwrap(), b"\xff\xd8jpeg");

        let ev = serde_json::to_vec(&InputEvent::Done).unwrap();
        let mut up = dev.sealer(Direction::Input);
        let mut down = vm.opener(Direction::Input);
        assert_eq!(down.open(&up.seal(&ev)).unwrap(), ev);
    }

    #[test]
    fn replays_reflections_and_other_sessions_are_refused() {
        let (vm, dev) = pair();
        let mut up = dev.sealer(Direction::Input);
        let mut down = vm.opener(Direction::Input);
        let m1 = up.seal(b"one");
        let m2 = up.seal(b"two");
        assert_eq!(down.open(&m2).unwrap(), b"two");
        // Older than the last accepted, or the same one again.
        assert_eq!(down.open(&m1), Err(Error::Replay));
        assert_eq!(down.open(&m2), Err(Error::Replay));
        // A frame (other direction) presented as input.
        let mut frames = vm.sealer(Direction::Frames);
        for _ in 0..5 {
            frames.seal(b"x");
        }
        assert_eq!(down.open(&frames.seal(b"x")), Err(Error::Auth));
        // Another session's input.
        let (vm2, dev2) = pair();
        let _ = vm2;
        let mut other = dev2.sealer(Direction::Input);
        for _ in 0..9 {
            other.seal(b"x");
        }
        assert_eq!(down.open(&other.seal(b"x")), Err(Error::Auth));
        // A flipped bit.
        let mut m3 = up.seal(b"three");
        let n = m3.len();
        m3[n - 1] ^= 1;
        assert_eq!(down.open(&m3), Err(Error::Auth));
        assert_eq!(down.open(b"short"), Err(Error::Malformed));
    }

    #[test]
    fn someone_without_the_device_key_derives_nothing_useful() {
        let device = DeviceKey::generate();
        let (offer, vm) = vm_start(&device.public());
        let impostor = DeviceKey::generate();
        let wrong = impostor.accept(&offer);
        let mut s = vm.sealer(Direction::Frames);
        assert_eq!(
            wrong.opener(Direction::Frames).open(&s.seal(b"frame")),
            Err(Error::Auth)
        );
    }
}
