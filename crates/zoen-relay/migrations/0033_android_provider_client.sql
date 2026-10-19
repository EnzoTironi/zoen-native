-- Preserve deployed challenges and their replay/consumption state. Android uses
-- a separately configured provider audience/presenter, never an iOS fallback.
ALTER TABLE provider_challenges DROP CONSTRAINT provider_challenges_client_check;
ALTER TABLE provider_challenges ADD CONSTRAINT provider_challenges_client_check
    CHECK (client IN ('ios', 'android', 'web'));
