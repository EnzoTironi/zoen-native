-- Public bootstrap metadata only. Host keys never enter this queue; enrollment
-- offers are HPKE ciphertext for the initiating owner's retained recipient key.
CREATE TABLE managed_agent_services (
    service TEXT PRIMARY KEY CHECK (service ~ '^[0-9a-f]{64}$'),
    template_sha256 TEXT NOT NULL CHECK (template_sha256 ~ '^[0-9a-f]{64}$'),
    heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE managed_agent_requests (
    owner TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    operation TEXT NOT NULL CHECK (operation ~ '^[0-9a-f]{64}$'),
    service TEXT NOT NULL REFERENCES managed_agent_services(service),
    template_sha256 TEXT NOT NULL CHECK (template_sha256 ~ '^[0-9a-f]{64}$'),
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    handle TEXT NOT NULL CHECK (octet_length(handle) BETWEEN 1 AND 64),
    recipient_public_key TEXT NOT NULL CHECK (recipient_public_key ~ '^[0-9a-f]{64}$'),
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued','offered','registered','expired','rejected')),
    agent TEXT NOT NULL DEFAULT '',
    offer_digest TEXT NOT NULL DEFAULT '',
    sealed_offer BYTEA NOT NULL DEFAULT ''::bytea,
    expires_ms BIGINT NOT NULL DEFAULT 0 CHECK (expires_ms >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(owner,operation),
    CHECK (
        (agent='' AND offer_digest='' AND octet_length(sealed_offer)=0 AND expires_ms=0
            AND state IN ('queued','rejected'))
        OR (agent ~ '^[0-9a-f]{64}$' AND offer_digest ~ '^[0-9a-f]{64}$'
            AND octet_length(sealed_offer) BETWEEN 40 AND 16384 AND expires_ms>0
            AND state IN ('offered','registered','expired'))
    )
);
CREATE INDEX managed_agent_service_requests ON managed_agent_requests(service,owner,operation);

CREATE FUNCTION managed_agent_original() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME='managed_agent_services' THEN
        IF NEW.service<>OLD.service OR NEW.template_sha256<>OLD.template_sha256 THEN
            RAISE EXCEPTION 'managed service identity/template is immutable' USING ERRCODE='23514';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP='INSERT' THEN
        IF NEW.state<>'queued' THEN
            RAISE EXCEPTION 'managed request must start queued' USING ERRCODE='23514';
        END IF;
        RETURN NEW;
    END IF;
    IF ROW(NEW.owner,NEW.operation,NEW.service,NEW.template_sha256,NEW.name,
           NEW.handle,NEW.recipient_public_key,NEW.created_at)
       IS DISTINCT FROM
       ROW(OLD.owner,OLD.operation,OLD.service,OLD.template_sha256,OLD.name,
           OLD.handle,OLD.recipient_public_key,OLD.created_at) THEN
        RAISE EXCEPTION 'managed request binding is immutable' USING ERRCODE='23514';
    END IF;
    IF OLD.state<>'queued' AND ROW(NEW.agent,NEW.offer_digest,NEW.sealed_offer,NEW.expires_ms)
       IS DISTINCT FROM ROW(OLD.agent,OLD.offer_digest,OLD.sealed_offer,OLD.expires_ms) THEN
        RAISE EXCEPTION 'managed enrollment original is immutable' USING ERRCODE='23514';
    END IF;
    IF OLD.state='registered' AND NEW.state<>'registered'
       OR OLD.state='rejected' AND NEW.state<>'rejected'
       OR OLD.state='expired' AND NEW.state NOT IN ('expired','registered') THEN
        RAISE EXCEPTION 'managed request cannot restart' USING ERRCODE='23514';
    END IF;
    IF NEW.state='registered' AND NOT EXISTS (
        SELECT 1 FROM identities i JOIN agent_citizens a ON a.agent=i.id
        WHERE i.id=NEW.agent AND i.kind='Agent' AND i.owner=NEW.owner AND a.owner=NEW.owner
    ) THEN
        RAISE EXCEPTION 'managed request has no canonical registered agent' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER managed_service_original BEFORE UPDATE ON managed_agent_services
    FOR EACH ROW EXECUTE FUNCTION managed_agent_original();
CREATE TRIGGER managed_request_original BEFORE INSERT OR UPDATE ON managed_agent_requests
    FOR EACH ROW EXECUTE FUNCTION managed_agent_original();
