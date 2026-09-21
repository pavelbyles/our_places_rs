import { execSync } from 'node:child_process';
import * as net from 'node:net';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { seedTestData } from './db-seed.js';

/**
 * Checks whether a TCP port is accepting connections.
 */
function isPortOpen(port: number, host = '127.0.0.1', timeoutMs = 1000): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    let status = false;

    socket.setTimeout(timeoutMs);
    socket.once('connect', () => {
      status = true;
      socket.end();
    });
    socket.once('timeout', () => {
      socket.destroy();
      resolve(false);
    });
    socket.once('error', () => {
      resolve(false);
    });
    socket.once('close', () => {
      resolve(status);
    });

    socket.connect(port, host);
  });
}

/**
 * Ensures the PostgreSQL database container is running and accepting connections.
 */
async function ensureDatabaseRunning(): Promise<void> {
  const dbReady = await isPortOpen(5432);
  if (dbReady) {
    console.log('✓ PostgreSQL is active on localhost:5432');
    return;
  }

  console.log('PostgreSQL is not responding on localhost:5432. Starting via devenv script...');
  try {
    try {
      execSync('db-start', { stdio: 'inherit' });
    } catch {
      execSync('devenv shell db-start', { stdio: 'inherit' });
    }
  } catch (err) {
    console.warn('Failed to execute devenv db-start script:', err);
  }

  // Poll for up to 15 seconds
  const start = Date.now();
  while (Date.now() - start < 15000) {
    if (await isPortOpen(5432)) {
      console.log('✓ PostgreSQL is now ready on localhost:5432');
      return;
    }
    await new Promise((r) => setTimeout(r, 1000));
  }

  throw new Error('Database (PostgreSQL) failed to start on localhost:5432 within 15 seconds.');
}

/**
 * Validates backend microservices readiness via direct HTTP calls.
 */
async function checkBackendServices(): Promise<void> {
  const services = [
    { name: 'booking_api', url: 'http://127.0.0.1:8081/health' },
    { name: 'listing_api', url: 'http://127.0.0.1:8082/api/v1/listings/the-courtyard-studio-new-kingston' },
    { name: 'user_api', url: 'http://127.0.0.1:8083/health' },
  ];

  for (const svc of services) {
    try {
      const res = await fetch(svc.url);
      if (res.ok) {
        console.log(`✓ ${svc.name} is healthy and responding at ${svc.url} (status ${res.status})`);
      } else {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
    } catch (err) {
      throw new Error(
        `Fatal: ${svc.name} is NOT responding at ${svc.url}. Ensure backend APIs are running (run 'apis' from devenv). Error: ${err}`
      );
    }
  }
}

/**
 * Playwright Global Setup Hook
 */
export default async function globalSetup(): Promise<void> {
  console.log('\n=== [Playwright Global Setup] Checking Environment & Infrastructure ===');

  // 1. Ensure DB is running
  await ensureDatabaseRunning();

  // 2. Seed database with deterministic test entities (villa, test users)
  await seedTestData();

  // 3. Check Backend APIs
  await checkBackendServices();

  // 4. Ensure .auth directory exists for session state caching
  const authDir = path.resolve(process.cwd(), '.auth');
  if (!fs.existsSync(authDir)) {
    fs.mkdirSync(authDir, { recursive: true });
  }

  console.log('=== [Playwright Global Setup] Completed ===\n');
}
