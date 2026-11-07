-- Onboarding disposition is separate from the independently running citizen.
ALTER TABLE managed_agent_requests ADD COLUMN onboarding_completed boolean NOT NULL DEFAULT false;
ALTER TABLE managed_agent_requests ADD CONSTRAINT managed_completion_registered
    CHECK (NOT onboarding_completed OR state='registered');
CREATE FUNCTION managed_onboarding_completion() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.onboarding_completed AND NOT NEW.onboarding_completed THEN
        RAISE EXCEPTION 'completed onboarding cannot restart' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER managed_onboarding_completion BEFORE UPDATE ON managed_agent_requests
    FOR EACH ROW EXECUTE FUNCTION managed_onboarding_completion();
CREATE INDEX managed_agent_owner_pending ON managed_agent_requests (owner,operation)
    WHERE NOT onboarding_completed AND state IN ('queued','offered','cancel_requested','registered');
