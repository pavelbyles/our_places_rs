import { execSync } from 'node:child_process';

/**
 * Seeds the database with essential test entities (test user, listing, image)
 * to ensure all listing and checkout E2E tests have deterministic data.
 */
export async function seedTestData(): Promise<void> {
  const dbUrl = process.env.DATABASE_URL || 'postgres://postgres:password@localhost:5432/our_places';

  const seedSql = `
    -- 1. Ensure test admin/host user exists
    INSERT INTO "user" (
        id, email, password_hash, first_name, last_name, roles, is_active, is_verified, default_currency, created_at, updated_at
    ) VALUES (
        '01a0bcbd-014d-7062-99b2-6a42d05b9ed7',
        'admin@ourplaces.io',
        '$2b$12$H09UlIMmYBCR3TpiZ44.SOkrYELsOkplVxMYH6Aup52FbrMF5VGCW',
        'System',
        'Admin',
        '{admin,host}',
        true,
        true,
        'USD',
        NOW(),
        NOW()
    ) ON CONFLICT (email) DO UPDATE SET
        roles = '{admin,host}',
        is_active = true;

    -- 2. Ensure test villa listing exists
    INSERT INTO listing (
        id,
        user_id,
        name,
        description,
        listing_structure_id,
        country,
        city,
        price_per_night,
        is_active,
        added_at,
        weekly_discount_percentage,
        monthly_discount_percentage,
        slug,
        max_guests,
        bedrooms,
        beds,
        full_bathrooms,
        half_bathrooms,
        base_currency,
        minimum_stay,
        days_between_bookings,
        commission_pct,
        listing_details
    ) VALUES (
        '018f3a5e-6b9c-7000-8000-000000000001',
        (SELECT id FROM "user" WHERE email = 'admin@ourplaces.io' LIMIT 1),
        'The Courtyard Studio',
        'Luxury courtyard studio nestled in New Kingston with lush private garden, dedicated chef, and infinity pool.',
        1,
        'Jamaica',
        'New Kingston',
        650.00,
        true,
        NOW(),
        10.00,
        20.00,
        'the-courtyard-studio-new-kingston',
        4,
        2,
        2,
        2,
        0,
        'USD',
        1,
        0,
        0.1000,
        '[]'::jsonb
    ) ON CONFLICT (slug) DO UPDATE SET
        price_per_night = EXCLUDED.price_per_night,
        is_active = true;

    -- 3. Ensure primary image and thumbnail exist
    INSERT INTO listing_image (
        id,
        listing_id,
        client_file_id,
        status,
        upload_url,
        content_type,
        display_order,
        resolution,
        is_primary
    ) VALUES (
        '018f3a5e-6b9c-7000-8000-000000000002',
        '018f3a5e-6b9c-7000-8000-000000000001',
        'primary_courtyard_image',
        'Processed',
        'https://images.unsplash.com/photo-1540541338287-41700207dee6?auto=format&fit=crop&w=1000&q=85',
        'image/jpeg',
        0,
        'Raw',
        true
    ) ON CONFLICT (id) DO UPDATE SET status = 'Processed';

    INSERT INTO listing_image (
        id,
        listing_id,
        client_file_id,
        status,
        upload_url,
        content_type,
        display_order,
        resolution,
        parent_id,
        is_primary
    ) VALUES (
        '018f3a5e-6b9c-7000-8000-000000000003',
        '018f3a5e-6b9c-7000-8000-000000000001',
        'thumb_courtyard_image',
        'Processed',
        'https://images.unsplash.com/photo-1540541338287-41700207dee6?auto=format&fit=crop&w=400&q=80',
        'image/jpeg',
        0,
        'Thumbnail400w',
        '018f3a5e-6b9c-7000-8000-000000000002',
        false
    ) ON CONFLICT (id) DO UPDATE SET status = 'Processed';
  `;

  try {
    execSync(`psql "${dbUrl}" -c "${seedSql.replace(/"/g, '\\"')}"`, {
      stdio: 'pipe',
    });
    console.log('✓ Seeded test villa listing: the-courtyard-studio-new-kingston');
  } catch (err) {
    console.warn('⚠ Note: DB seed command could not be completed directly:', (err as Error).message);
  }
}
