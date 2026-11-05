-- Keep the original ledger, immutable history and image ceiling. Larger chat
-- frames require an explicitly larger frozen profile and original admission.
-- Ciphertext is incompressible: retain ordinary TOAST storage without spending
-- the statement budget attempting compression of each large encrypted frame.
ALTER TABLE runtime_evidence ALTER COLUMN sealed SET STORAGE EXTERNAL;
ALTER TABLE runtime_evidence DROP CONSTRAINT runtime_evidence_sealed_check;
ALTER TABLE runtime_evidence ADD CONSTRAINT runtime_evidence_sealed_check
    CHECK (octet_length(sealed) BETWEEN 40 AND 35651584);

DROP TRIGGER image_evidence_bound ON runtime_evidence;
DROP FUNCTION runtime_image_evidence_bound();

CREATE FUNCTION runtime_capture_evidence_bound() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    size_bytes BIGINT := octet_length(NEW.sealed);
    original_binding JSONB;
    response_bound NUMERIC;
BEGIN
    IF size_bytes <= 98304 THEN
        RETURN NEW;
    END IF;
    SELECT a.binding INTO original_binding
        FROM runtime_attempts a WHERE a.attempt=NEW.attempt;
    IF original_binding->'price'->>'currency'='USD'
       AND original_binding->'price'->>'scale'='6' THEN
        IF jsonb_typeof(original_binding->'price'->'image')='object'
           AND original_binding->'price'->'image'->>'count'='1'
           AND original_binding->'price'->'image'->>'format'='png'
           AND original_binding->>'requested_output_tokens'='0'
           AND size_bytes <= 33554432 THEN
            RETURN NEW;
        END IF;
        IF NOT (original_binding->'price' ? 'image')
           AND jsonb_typeof(original_binding->'price'->'max_response_bytes')='number' THEN
            response_bound := (original_binding->'price'->>'max_response_bytes')::NUMERIC;
            IF response_bound BETWEEN 16385 AND 16777216
               AND trunc(response_bound)=response_bound
               AND size_bytes <= 2*response_bound+262209
               AND EXISTS (
                   SELECT 1 FROM runtime_claims c
                   JOIN runtime_admission_witnesses w USING(attempt)
                   WHERE c.attempt=NEW.attempt AND w.nonce=c.nonce
               ) THEN
                RETURN NEW;
            END IF;
        END IF;
    END IF;
    RAISE EXCEPTION 'large runtime evidence requires a bounded original obligation and admission'
        USING ERRCODE='23514';
END;
$$;
CREATE TRIGGER capture_evidence_bound BEFORE INSERT ON runtime_evidence
    FOR EACH ROW EXECUTE FUNCTION runtime_capture_evidence_bound();
