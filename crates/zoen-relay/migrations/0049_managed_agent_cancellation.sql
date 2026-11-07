-- Cancellation is a request until the host proves it has not consumed consent.
ALTER TABLE managed_agent_requests
    DROP CONSTRAINT managed_agent_requests_state_check,
    DROP CONSTRAINT managed_agent_requests_check;
ALTER TABLE managed_agent_requests ADD CONSTRAINT managed_request_state CHECK (
    state IN ('queued','offered','registered','expired','rejected','cancel_requested','cancelled'));
ALTER TABLE managed_agent_requests ADD CONSTRAINT managed_request_offer CHECK (
    (agent='' AND offer_digest='' AND octet_length(sealed_offer)=0 AND expires_ms=0
        AND state IN ('queued','rejected','cancel_requested','cancelled'))
    OR (agent ~ '^[0-9a-f]{64}$' AND offer_digest ~ '^[0-9a-f]{64}$'
        AND octet_length(sealed_offer) BETWEEN 40 AND 16384 AND expires_ms>0
        AND state IN ('offered','registered','expired','cancel_requested','cancelled')));
CREATE FUNCTION managed_agent_cancellation() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.state='cancelled' AND NEW.state<>'cancelled'
       OR OLD.state='cancel_requested' AND NEW.state NOT IN ('cancel_requested','cancelled','registered')
       OR NEW.state='cancel_requested' AND OLD.state NOT IN ('queued','offered','cancel_requested')
       OR NEW.state='cancelled' AND OLD.state NOT IN ('cancel_requested','cancelled') THEN
        RAISE EXCEPTION 'managed cancellation cannot restart' USING ERRCODE='23514';
    END IF;
    IF NEW.state IN ('cancel_requested','cancelled') AND
       ROW(NEW.agent,NEW.offer_digest,NEW.sealed_offer,NEW.expires_ms) IS DISTINCT FROM
       ROW(OLD.agent,OLD.offer_digest,OLD.sealed_offer,OLD.expires_ms) THEN
        RAISE EXCEPTION 'managed cancellation preserves the original' USING ERRCODE='23514';
    END IF;
    IF NEW.state='cancelled' AND EXISTS (SELECT 1 FROM identities WHERE id=NEW.agent) THEN
        RAISE EXCEPTION 'registered identity cannot be cancelled' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER managed_cancellation BEFORE UPDATE ON managed_agent_requests
    FOR EACH ROW EXECUTE FUNCTION managed_agent_cancellation();
