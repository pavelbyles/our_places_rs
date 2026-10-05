-- 001_admin_user.sql
-- Prime database with default administrative user for initial login.
-- Credentials: admin@ourplaces.io / admin_changeme_2026
-- Password hash: bcrypt (cost 12)
INSERT INTO "user" (
    id,
    email,
    password_hash,
    first_name,
    last_name,
    phone_number,
    is_active,
    is_verified,
    default_currency,
    roles,
    attributes,
    created_at,
    updated_at
) VALUES (
    '01a0bcbd-014d-7062-99b2-6a42d05b9ed7',
    'admin@ourplaces.io',
    '$2b$12$H09UlIMmYBCR3TpiZ44.SOkrYELsOkplVxMYH6Aup52FbrMF5VGCW',
    'System',
    'Admin',
    NULL,
    TRUE,
    TRUE,
    'USD',
    '{admin,host}'::user_role[],
    '{}'::jsonb,
    NOW(),
    NOW()
)
ON CONFLICT (email) DO UPDATE SET
    password_hash = EXCLUDED.password_hash,
    roles = EXCLUDED.roles,
    is_active = TRUE,
    is_verified = TRUE,
    updated_at = NOW();
