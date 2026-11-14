/**
 * Shared test data and constants across E2E test suites.
 */
import bcrypt from 'bcryptjs';

export const TEST_CONFIG = {
  guestPortalUrl: process.env.GUEST_PORTAL_URL || 'http://localhost:3010',
  adminPortalUrl: process.env.ADMIN_PORTAL_URL || 'http://localhost:3012',
};

export const TEST_USERS = {
  admin: {
    email: 'admin@ourplaces.io',
    password: 'admin_changeme_2026',
    firstName: 'System',
    lastName: 'Admin',
    role: 'admin',
  },
  guest: {
    email: 'guest.tester@example.com',
    password: 'guestPassword123!',
    firstName: 'Jordan',
    lastName: 'Tester',
    phone: '+18765550199',
  },
};

const passwordHashes = new Map<string, string>();

/**
 * bcrypt hash of a test password, for inserting users straight into the
 * database. Hashed at runtime so no hash is committed; cached because each
 * worker would otherwise pay the bcrypt cost for every user it creates.
 */
export function passwordHash(password: string): string {
  let hash = passwordHashes.get(password);
  if (hash === undefined) {
    hash = bcrypt.hashSync(password, 10);
    passwordHashes.set(password, hash);
  }
  return hash;
}

export const KNOWN_LISTINGS = {
  courtyardStudio: {
    id: '018f3a5e-6b9c-7000-8000-000000000001',
    slug: 'the-courtyard-studio-new-kingston',
    name: 'The Courtyard Studio',
    city: 'New Kingston',
    country: 'Jamaica',
    pricePerNight: '650.00',
  },
  skylineSuite: {
    slug: 'kingston-skyline-luxury-suite',
    name: 'Kingston Skyline Suite',
    city: 'Kingston',
  },
};

/**
 * The seeded listing's stored point (spec 103): New Kingston, kept
 * approximate, so public pages must never show it. Copies made with
 * `createListingCopy` start from it too.
 */
export const KNOWN_LOCATION = { latitude: 18.012345, longitude: -76.790123 };

/** An exact point for listing copies that test the Exact setting. */
export const EXACT_LOCATION = { latitude: 18.407412, longitude: -77.103134 };
