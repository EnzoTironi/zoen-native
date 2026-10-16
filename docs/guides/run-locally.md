# Tutorial: run Zoen on your machine

In about fifteen minutes you'll run the whole Zoen backend locally and have two people — Ana and Bruno — talk over an **end-to-end encrypted** chat from two terminals, then link a second device and make a backup. Everything uses the **same Rust core** the iPhone, Mac and Android apps use.

> **What you need:** Linux or macOS, `git`, `curl`, a C toolchain, and Rust **1.99.0** (`rustup`). The first run downloads a private FoundationDB (and, unless you bring your own, a private Postgres).

## 1. Get the toolchain

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
```

## 2. Start the backend

In its own terminal, from the repository root:

```sh
scripts/dev-stack.sh
```

It starts a private FoundationDB under `.dev/fdb`, a private Postgres under `.dev/pg`, builds `zoen-relay` and the `zoen` CLI, and stays attached to the relay on port **8787**. You'll see:

```text
▸ FoundationDB
▸ building zoen-relay and the zoen CLI
▸ relay on 0.0.0.0:8787 (private Postgres in .dev/pg; first run downloads it)
… INFO zoen_relay: log store ready (FoundationDB) cell=c0
… INFO zoen_relay: zoen-relay listening bind=0.0.0.0:8787 relay=zoen-dev
```

Already have Postgres? `DATABASE_URL=postgres://user@host/db scripts/dev-stack.sh`. Want it on loopback only? `ZOEN_BIND=127.0.0.1:8787 scripts/dev-stack.sh`.

## 3. Create two people

In a second terminal. Each `--home` is a separate device with its own keys and database:

```sh
alias zoen=target/debug/zoen
zoen --home /tmp/zoen/ana   init --name Ana   --handle ana   --relay http://127.0.0.1:8787
zoen --home /tmp/zoen/bruno init --name Bruno --handle bruno --relay http://127.0.0.1:8787
```

```text
@ana (9d2d8a38af30) on http://127.0.0.1:8787 — registered
@bruno (2077f0848426) on http://127.0.0.1:8787 — registered
```

## 4. Talk, end to end

```sh
zoen --home /tmp/zoen/ana   dm @bruno "oi Bruno"
zoen --home /tmp/zoen/bruno read @ana
zoen --home /tmp/zoen/bruno send @ana "Oi Ana! Tudo certo para sábado?"
zoen --home /tmp/zoen/ana   read @bruno
```

```text
Ana: oi Bruno
Bruno: Oi Ana! Tudo certo para sábado?
```

The relay ordered and delivered those messages but only ever held ciphertext. Look at the MLS group behind the chat:

```sh
zoen --home /tmp/zoen/ana chats
zoen --home /tmp/zoen/ana keys @bruno
```

```text
sp_01m52j1d…	Bruno	Direct	relay	e2e	unread=0	Oi Ana! Tudo certo para sábado?
epoch=1	digest=dbc070a2…	members=2	devices=2
```

`e2e` means end-to-end; `members=2 devices=2` is one MLS leaf per device. Make a group the same way: `zoen --home /tmp/zoen/ana group "Barbearia" @bruno`.

## 5. Link a second device, with history

Start a "laptop" for Ana — it prints a code and six check digits, then waits:

```sh
zoen --home /tmp/zoen/ana-laptop link-request --relay http://127.0.0.1:8787
```

```text
code: zoen-link:1:bf5ef68b…
check: 419560
```

On Ana's existing device, approve it (in the app, this is where you compare the six digits):

```sh
zoen --home /tmp/zoen/ana link "zoen-link:1:bf5ef68b…"
```

```text
check: 419560
in 2 of 2 groups
history sent: 2 messages
```

The laptop finishes with `linked: @ana …` and `history: 2 messages in 1 chunks`. `zoen --home /tmp/zoen/ana devices` now lists both devices; `unlink DEVICE` revokes one.

## 6. Back up, then recover on a brand-new device

```sh
zoen --home /tmp/zoen/ana backup on --recovery-key
```

```text
recovery-key	1874 2009 3864 … (64 digits in groups of four)
backup on mode=recovery_key bytes=147833
```

Simulate losing everything and restore from the key:

```sh
zoen --home /tmp/zoen/ana-new recover @ana --recovery-key "1874 2009 3864 …"
```

```text
@ana (9d2d8a38af30) restored on a new device e4b9e972… — 3 chats, connection=online
```

## What you just did

- Ran the relay with FoundationDB and Postgres — the same building blocks as staging.
- Created two identities, each with a certified device key.
- Exchanged MLS-encrypted messages through the durable outbox and sync.
- Linked a device with encrypted history transfer, and recovered an account from a 64-digit recovery key.

## Troubleshooting

| You see | Why | Fix |
| --- | --- | --- |
| `slow down, retry in N s` | The relay's abuse limits (per IP, per device) kicked in — e.g. many registrations in a short time. Buckets live in the relay process. | Wait, or restart the relay with a looser development quota: `ZOEN_LIMITS="register_ip=100/h:100,connect_ip=600/m:600" scripts/dev-stack.sh`. |
| `handle_taken` | The handle is already registered on this relay. | Pick another handle. |
| `unable to find library -lfdb_c` when building by hand | FoundationDB client library not on the path. | Use `scripts/dev-stack.sh`, or `scripts/fdb.sh up && eval "$(scripts/fdb.sh env)"` first. |
| Postgres "Address already in use" | Another Postgres uses the private port. | Pass your own `DATABASE_URL`. |

## Next

- [Run the Apple apps](apple.md) against this relay — the simulator connects to `127.0.0.1:8787` by default.
- [Run Android](android.md) — the emulator reaches it at `http://10.0.2.2:8787`.
- [Build your first mini-app](first-mini-app.md).
- See every CLI command with `target/debug/zoen` (no arguments).
