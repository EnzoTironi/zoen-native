ALTER TABLE push_tokens
    ADD COLUMN registration_id bigint GENERATED ALWAYS AS IDENTITY;
