-- Cancellation policy
ALTER TYPE cancellation_policy ADD VALUE IF NOT EXISTS 'firm';
ALTER TYPE cancellation_policy ADD VALUE IF NOT EXISTS 'limited';

-- Listing fees (base currency). 0 = not offered.
ALTER TABLE listing
  ADD COLUMN cleaning_fee   NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (cleaning_fee >= 0),
  ADD COLUMN damage_deposit NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (damage_deposit >= 0);

ALTER TABLE listing_history
  ADD COLUMN cleaning_fee   NUMERIC(12,2),
  ADD COLUMN damage_deposit NUMERIC(12,2);

-- Booking financial snapshot. Existing rows: tax_rate backfilled to the 10% actually charged.
ALTER TABLE booking
  ADD COLUMN cleaning_fee   NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN damage_deposit NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN platform_fee   NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN tax_rate       NUMERIC(5,4)  NOT NULL DEFAULT 0.1000,
  ADD COLUMN exchange_rate  NUMERIC(15,6) NOT NULL DEFAULT 1;

UPDATE booking SET platform_fee = COALESCE(
  (SELECT SUM((f->>'amount')::numeric) FROM jsonb_array_elements(fee_breakdown) f
    WHERE f->>'name' = 'Platform Fee'), 0);

-- booking_history: nullable so existing pg_cron INSERTs keep working
ALTER TABLE booking_history
  ADD COLUMN cleaning_fee NUMERIC(10,2),
  ADD COLUMN damage_deposit NUMERIC(10,2),
  ADD COLUMN platform_fee NUMERIC(10,2),
  ADD COLUMN tax_rate NUMERIC(5,4),
  ADD COLUMN exchange_rate NUMERIC(15,6);

-- Tax documents
CREATE TYPE tax_document_type   AS ENUM ('payment_receipt', 'tax_invoice', 'refund_receipt');
CREATE TYPE tax_document_status AS ENUM ('issued', 'superseded');

CREATE TABLE tax_document (
  id                     UUID PRIMARY KEY DEFAULT uuidv7(),
  document_type          tax_document_type   NOT NULL,
  document_number        VARCHAR(40)         NOT NULL UNIQUE,
  revision               INTEGER             NOT NULL DEFAULT 1 CHECK (revision >= 1),
  status                 tax_document_status NOT NULL DEFAULT 'issued',
  supersedes_id          UUID REFERENCES tax_document(id),
  booking_id             UUID NOT NULL REFERENCES booking(id) ON DELETE RESTRICT,
  booking_history_id     UUID REFERENCES booking_history(id),
  recipient_id           UUID NOT NULL REFERENCES "user"(id),
  listing_id             UUID NOT NULL REFERENCES listing(id),
  issue_date             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  supplier_name          VARCHAR(255) NOT NULL,
  supplier_address       TEXT         NOT NULL,
  supplier_trn           VARCHAR(32)  NOT NULL,
  recipient_name         VARCHAR(255) NOT NULL,
  recipient_email        VARCHAR(255) NOT NULL,
  recipient_company_name VARCHAR(255),
  recipient_trn          VARCHAR(32),
  currency               CHAR(3)       NOT NULL,
  exchange_rate          NUMERIC(15,6) NOT NULL,   -- base -> payment
  jmd_exchange_rate      NUMERIC(15,6) NOT NULL,   -- payment -> JMD
  accommodation_net      NUMERIC(12,2) NOT NULL,
  cleaning_fee           NUMERIC(12,2) NOT NULL,
  taxable_amount         NUMERIC(12,2) NOT NULL,
  tax_rate               NUMERIC(5,4)  NOT NULL,
  tax_amount             NUMERIC(12,2) NOT NULL,
  platform_fee           NUMERIC(12,2) NOT NULL,
  damage_deposit         NUMERIC(12,2) NOT NULL,
  total_amount           NUMERIC(12,2) NOT NULL,
  amount_previously_paid NUMERIC(12,2),            -- revised receipts only
  balance_due            NUMERIC(12,2),            -- revised receipts only (+ owed / - refundable)
  refund_pct             NUMERIC(5,4),             -- refund receipts only
  retained_amount        NUMERIC(12,2),            -- refund receipts only
  jmd_tax_amount         NUMERIC(14,2) NOT NULL,
  jmd_total_amount       NUMERIC(14,2) NOT NULL,
  line_items             JSONB NOT NULL DEFAULT '[]'::jsonb,
  created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (booking_id, document_type, revision)
);

CREATE UNIQUE INDEX uq_tax_document_active ON tax_document (booking_id, document_type) WHERE status = 'issued';
CREATE INDEX idx_tax_document_booking   ON tax_document (booking_id);
CREATE INDEX idx_tax_document_recipient ON tax_document (recipient_id);
