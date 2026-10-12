-- One financial database owns one immutable execution deployment. A store
-- pairing marker identifies that deployment; it is not a restore-continuity
-- witness and cannot authorize a paid dispatch or an absence-based refund.
CREATE TABLE runtime_deployment_binding (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    namespace TEXT NOT NULL CHECK (octet_length(namespace) BETWEEN 1 AND 128),
    key_digest TEXT NOT NULL CHECK (key_digest ~ '^[a-f0-9]{64}$'),
    witness TEXT NOT NULL CHECK (witness ~ '^[a-f0-9]{64}$')
);
CREATE TRIGGER immutable_deployment BEFORE UPDATE OR DELETE ON runtime_deployment_binding
    FOR EACH ROW EXECUTE FUNCTION runtime_history_immutable();
CREATE TRIGGER immutable_deployment_truncate BEFORE TRUNCATE ON runtime_deployment_binding
    FOR EACH STATEMENT EXECUTE FUNCTION runtime_history_immutable();

ALTER TABLE runtime_attempts ADD COLUMN deployment BOOLEAN NOT NULL DEFAULT TRUE CHECK (deployment);
-- Retained attempts from the unbound format need explicit reconciliation.
-- New attempts cannot predate a known, committed store pairing.
ALTER TABLE runtime_attempts ADD CONSTRAINT runtime_attempt_deployment
    FOREIGN KEY (deployment) REFERENCES runtime_deployment_binding(singleton) NOT VALID;
