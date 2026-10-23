-- Existing tokens remain notification tokens. A VoIP refresh/clear must never
-- replace the same device's ordinary APNs registration (or vice versa).
ALTER TABLE push_tokens ADD COLUMN purpose TEXT NOT NULL DEFAULT 'notification';
ALTER TABLE push_tokens ADD CONSTRAINT push_tokens_purpose_check
    CHECK (purpose IN ('notification', 'voip'));
ALTER TABLE push_tokens DROP CONSTRAINT push_tokens_pkey;
ALTER TABLE push_tokens ADD PRIMARY KEY (identity, device, purpose);
ALTER TABLE push_tokens ADD CONSTRAINT push_tokens_platform_purpose_check CHECK (
    (purpose = 'notification' AND platform IN ('apns', 'apns-sandbox', 'fcm')) OR
    (purpose = 'voip' AND platform IN ('apns-voip', 'apns-voip-sandbox'))
);
