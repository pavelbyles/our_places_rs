CREATE TABLE currency_exchange_rates (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    base_currency VARCHAR(3) NOT NULL,
    target_currency VARCHAR(3) NOT NULL,
    rate NUMERIC(15, 6) NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP NOT NULL,
    effective_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT unique_currency_pair UNIQUE (base_currency, target_currency, effective_at)
);

CREATE INDEX idx_exchange_rates_pair ON currency_exchange_rates (base_currency, target_currency, effective_at);

INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at) VALUES
('USD', 'JMD', 160.000000, CURRENT_DATE),
('JMD', 'USD', 0.006250, CURRENT_DATE),
('USD', 'GBP', 0.790000, CURRENT_DATE),
('GBP', 'USD', 1.265822, CURRENT_DATE),
('JMD', 'GBP', 0.005096, CURRENT_DATE),
('GBP', 'JMD', 196.200000, CURRENT_DATE);

ALTER TABLE listing ADD COLUMN base_currency VARCHAR(3) NOT NULL DEFAULT 'USD';