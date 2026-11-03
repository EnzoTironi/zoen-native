-- Scoped consent for an authenticated sender to wake a recipient for calls.
-- Conversation membership remains authoritative in the event log.
CREATE TABLE voip_consent (
    recipient TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    sender TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    space TEXT NOT NULL,
    PRIMARY KEY (recipient, sender, space),
    CHECK (recipient <> sender)
);
