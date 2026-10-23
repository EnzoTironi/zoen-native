-- Preserve the existing evidence/claim/journal authority. Images retain a
-- bounded binary frame; legacy chat keeps its original 98 KiB SQL ceiling.
ALTER TABLE runtime_evidence DROP CONSTRAINT runtime_evidence_sealed_check;
ALTER TABLE runtime_evidence ADD CONSTRAINT runtime_evidence_sealed_check
    CHECK (octet_length(sealed) BETWEEN 40 AND 33554432);

CREATE FUNCTION runtime_image_evidence_bound() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF octet_length(NEW.sealed) > 98304 AND NOT EXISTS (
        SELECT 1 FROM runtime_attempts a WHERE a.attempt=NEW.attempt
          AND jsonb_typeof(a.binding->'price'->'image')='object'
          AND a.binding->'price'->'image'->>'count'='1'
          AND a.binding->'price'->'image'->>'format'='png'
          AND a.binding->>'requested_output_tokens'='0'
          AND a.binding->'price'->>'currency'='USD'
          AND a.binding->'price'->>'scale'='6'
    ) THEN
        RAISE EXCEPTION 'large runtime evidence requires an explicit image obligation'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER image_evidence_bound BEFORE INSERT ON runtime_evidence
    FOR EACH ROW EXECUTE FUNCTION runtime_image_evidence_bound();
