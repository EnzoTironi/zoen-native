-- Logs, heads, membership and invites now live in FoundationDB (ADR 0008). Postgres keeps
-- the directory: identities, handles and devices.
DROP TABLE invites;
DROP TABLE events;
DROP TABLE members;
DROP TABLE spaces;
