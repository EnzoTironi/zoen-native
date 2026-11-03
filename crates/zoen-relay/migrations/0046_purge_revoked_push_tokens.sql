-- Older relays retained tokens after revoking devices.
DELETE FROM push_tokens p
USING devices d
WHERE p.identity = d.identity
  AND p.device = d.device
  AND d.revoked_at IS NOT NULL;
