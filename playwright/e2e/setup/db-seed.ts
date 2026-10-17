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

    -- 1b. Ensure test guest user exists
    INSERT INTO "user" (
        id, email, password_hash, first_name, last_name, roles, is_active, is_verified, default_currency, created_at, updated_at
    ) VALUES (
        '01a0bcbd-014d-7062-99b2-6a42d05b9ee8',
        'guest.tester@example.com',
        '$2b$12$G3TzzR2meVcZAQU2OL49tel/uqz8pfao3/6jDPvWsyc9bL4EUdgk2',
        'Jordan',
        'Tester',
        '{booker}',
        true,
        true,
        'USD',
        NOW(),
        NOW()
    ) ON CONFLICT (email) DO UPDATE SET
        password_hash = EXCLUDED.password_hash,
        roles = '{booker}',
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
        cleaning_fee,
        damage_deposit,
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
        75.00,
        150.00,
        '[]'::jsonb
    ) ON CONFLICT (slug) DO UPDATE SET
        price_per_night = EXCLUDED.price_per_night,
        cleaning_fee = EXCLUDED.cleaning_fee,
        damage_deposit = EXCLUDED.damage_deposit,
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

    -- 4. Ensure deterministic test booking exists for E2E tests
    INSERT INTO booking (
        id,
        confirmation_code,
        guest_id,
        listing_id,
        status,
        date_from,
        date_to,
        currency,
        daily_rate,
        number_of_persons,
        total_days,
        sub_total_price,
        discount_value,
        tax_value,
        fee_breakdown,
        total_price,
        cancellation_policy,
        metadata,
        door_access_code,
        cleaning_fee,
        damage_deposit,
        platform_fee,
        tax_rate,
        exchange_rate,
        created_at,
        updated_at
    ) VALUES (
        '018f3a5e-6b9c-7000-8000-000000000010',
        'OP-E2E-7788',
        '01a0bcbd-014d-7062-99b2-6a42d05b9ee8',
        '018f3a5e-6b9c-7000-8000-000000000001',
        'confirmed',
        CURRENT_DATE + INTERVAL '2 days',
        CURRENT_DATE + INTERVAL '5 days',
        'USD',
        650.00,
        2,
        3,
        1950.00,
        0.00,
        303.75,
        '[{"name":"Cleaning Fee","amount":"75.00"},{"name":"Damage Deposit","amount":"150.00"},{"name":"Platform Fee","amount":"50.00"}]'::jsonb,
        2528.75,
        'flexible',
        '{"num_adults": 2, "num_children": 0, "num_infants": 0, "num_pets": 0, "is_business_trip": false}'::jsonb,
        NULL,
        75.00,
        150.00,
        50.00,
        0.1500,
        1.000000,
        NOW(),
        NOW()
    ) ON CONFLICT (id) DO UPDATE SET
        status = 'confirmed',
        total_price = EXCLUDED.total_price,
        fee_breakdown = EXCLUDED.fee_breakdown,
        metadata = EXCLUDED.metadata;

    -- 5. Currency exchange rate for JMD tax audit
    INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
    VALUES ('USD', 'JMD', 160.000000, NOW())
    ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING;

    -- 6. Seed deterministic payment receipt for OP-E2E-7788
    INSERT INTO tax_document (
        id,
        document_type,
        document_number,
        revision,
        status,
        supersedes_id,
        booking_id,
        booking_history_id,
        recipient_id,
        listing_id,
        issue_date,
        supplier_name,
        supplier_address,
        supplier_trn,
        recipient_name,
        recipient_email,
        recipient_company_name,
        recipient_trn,
        currency,
        exchange_rate,
        jmd_exchange_rate,
        accommodation_net,
        cleaning_fee,
        taxable_amount,
        tax_rate,
        tax_amount,
        platform_fee,
        damage_deposit,
        total_amount,
        amount_previously_paid,
        balance_due,
        refund_pct,
        retained_amount,
        jmd_tax_amount,
        jmd_total_amount,
        line_items,
        created_at
    ) VALUES (
        '018f3a5e-6b9c-7000-8000-000000000030',
        'payment_receipt',
        'REC-2026-OP-E2E-7788',
        1,
        'issued',
        NULL,
        '018f3a5e-6b9c-7000-8000-000000000010',
        NULL,
        '01a0bcbd-014d-7062-99b2-6a42d05b9ee8',
        '018f3a5e-6b9c-7000-8000-000000000001',
        NOW(),
        'Our Places Jamaica Limited',
        'Main Street, Ocho Rios, St. Ann, Jamaica',
        '000-000-000',
        'Jordan Tester',
        'guest.tester@example.com',
        NULL,
        NULL,
        'USD',
        1.000000,
        160.000000,
        1950.00,
        75.00,
        2025.00,
        0.1500,
        303.75,
        50.00,
        150.00,
        2528.75,
        NULL,
        NULL,
        NULL,
        NULL,
        48600.00,
        404600.00,
        '[{"description":"Accommodation (3 nights)","quantity":3,"unit_price":"650.00","tax_category":"gct","tax_rate":"0.1500","tax_amount":"292.50","total_amount":"1950.00"},{"description":"Cleaning fee","quantity":1,"unit_price":"75.00","tax_category":"gct","tax_rate":"0.1500","tax_amount":"11.25","total_amount":"75.00"},{"description":"Platform fee","quantity":1,"unit_price":"50.00","tax_category":"exempt","tax_rate":"0.0000","tax_amount":"0.00","total_amount":"50.00"},{"description":"Security deposit (held)","quantity":1,"unit_price":"150.00","tax_category":"exempt","tax_rate":"0.0000","tax_amount":"0.00","total_amount":"150.00"}]'::jsonb,
        NOW()
    ) ON CONFLICT (document_number) DO UPDATE SET
        status = 'issued',
        line_items = EXCLUDED.line_items,
        total_amount = EXCLUDED.total_amount;
  `;

  try {
    try {
      execSync(`psql "${dbUrl}"`, {
        input: seedSql,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    } catch {
      execSync(`devenv shell psql "${dbUrl}"`, {
        input: seedSql,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    }
    console.log('✓ Seeded test villa listing & deterministic booking: the-courtyard-studio-new-kingston');
  } catch (err: unknown) {
    const error = err as { message?: string; stderr?: Buffer };
    const stderrMsg = error.stderr ? error.stderr.toString() : '';
    console.warn('⚠ Note: DB seed command could not be completed directly:', error.message, stderrMsg);
  }
}
