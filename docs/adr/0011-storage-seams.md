# ADR 0011: Storage behind traits

Status: accepted. `LogStore` implemented in S3 (`crates/zoen-relay/src/log/`, FoundationDB).
The directory is still a set of concrete Postgres functions (`db.rs`): it gets a trait when a
second implementation exists, not before. `KeyStore` lands with M2 (MLS key packages).

## Decision
sync talks to `LogStore` (append, read range, heads, dedupe, outbox), `Directory`
(identities, handles, devices; Postgres) and `KeyStore` (key packages; FDB). FoundationDB
implements `LogStore` and `KeyStore`; Postgres implements `Directory`. An in-memory `LogStore`
is allowed in unit tests only. The journey suite runs against FoundationDB and Postgres.

## At 1B users
The traits are partition-aware (every call names its Space or identity), so cells and
regions are a routing concern above the trait, not a change below it.
