/**
 * Shared test data and constants across E2E test suites.
 */
export const TEST_CONFIG = {
  guestPortalUrl: process.env.GUEST_PORTAL_URL || 'http://localhost:3000',
  adminPortalUrl: process.env.ADMIN_PORTAL_URL || 'http://localhost:3002',
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
