-- Migration: Enhance sessions table with user tracking, namespace isolation, and audit timestamps
ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES "user"(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS email VARCHAR(255),
    ADD COLUMN IF NOT EXISTS namespace VARCHAR(32) NOT NULL DEFAULT 'guest',
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE INDEX IF NOT EXISTS idx_sessions_user_namespace ON sessions (user_id, namespace);
CREATE INDEX IF NOT EXISTS idx_sessions_email ON sessions (email);
CREATE INDEX IF NOT EXISTS idx_sessions_ttl ON sessions (ttl);
