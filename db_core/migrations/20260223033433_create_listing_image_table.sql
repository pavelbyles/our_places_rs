CREATE TYPE image_status AS ENUM (
    'PendingUpload',
    'Uploaded',
    'Processing',
    'Processed',
    'Failed'
);

CREATE TABLE listing_image (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    client_file_id VARCHAR(255) NOT NULL,
    status image_status NOT NULL DEFAULT 'PendingUpload',
    upload_url VARCHAR(1024),
    content_type VARCHAR(255),
    size_bytes BIGINT,
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_listing_image_listing_id ON listing_image(listing_id);
