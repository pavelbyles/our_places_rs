-- Create function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create user_role enum
CREATE TYPE user_role AS ENUM ('booker', 'host', 'admin');

-- Add roles to user table
ALTER TABLE "user" ADD COLUMN roles user_role[] NOT NULL DEFAULT '{}';

-- Add roles to user_history table
ALTER TABLE user_history ADD COLUMN roles user_role[] DEFAULT '{}';

-- Create booker_profiles table
CREATE TABLE booker_profiles (
    user_id UUID PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    emergency_contacts JSONB,
    booking_preferences JSONB,
    loyalty JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create host_profiles table
CREATE TABLE host_profiles (
    user_id UUID PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    verified_status TEXT,
    payout_details JSONB,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create trigger to update updated_at for booker_profiles
CREATE TRIGGER update_booker_profiles_updated_at
BEFORE UPDATE ON booker_profiles
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

-- Create trigger to update updated_at for host_profiles
CREATE TRIGGER update_host_profiles_updated_at
BEFORE UPDATE ON host_profiles
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();
