-- Restore detector only. This table grants no provider or output capability,
-- stores no money, and is insufficient without the independently retained
-- local anchor. Never include that anchor in SQL/FDB restore operations.
CREATE TABLE runtime_image_checkpoint (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    namespace TEXT NOT NULL,
    witness TEXT NOT NULL CHECK (witness ~ '^[a-f0-9]{64}$'),
    agent TEXT NOT NULL,
    device TEXT NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    sequence BIGINT NOT NULL CHECK (sequence >= 0),
    digest TEXT NOT NULL CHECK (digest ~ '^[a-f0-9]{64}$'),
    FOREIGN KEY (singleton) REFERENCES runtime_deployment_binding(singleton)
);
