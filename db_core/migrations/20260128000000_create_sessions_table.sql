CREATE TABLE sessions (
    id VARCHAR PRIMARY KEY, -- Session ID (random string from cookie)
    state BYTEA NOT NULL,   -- Serialized session state
    ttl BIGINT NOT NULL     -- Expiration timestamp (epoch seconds)
);
