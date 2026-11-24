-- Allow 'staff_accept' action tokens and store pending roster invites for existing accounts
-- until the invited user explicitly accepts.

ALTER TABLE account_action_token
    DROP CONSTRAINT IF EXISTS account_action_token_purpose_check;

ALTER TABLE account_action_token
    ADD CONSTRAINT account_action_token_purpose_check
    CHECK (purpose IN ('staff_invite', 'staff_accept', 'password_reset'));

CREATE TABLE pending_staff_invite (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    host_user_id    UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    invited_by      UUID REFERENCES "user"(id) ON DELETE SET NULL,
    target_user_id  UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    listing_ids     UUID[] NOT NULL,
    action_token_id UUID NOT NULL REFERENCES account_action_token(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at     TIMESTAMPTZ
);

CREATE INDEX idx_pending_staff_invite_token
    ON pending_staff_invite (action_token_id)
    WHERE accepted_at IS NULL;
