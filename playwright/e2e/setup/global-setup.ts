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
      execSync('db-start', { stdio: 'pipe' });
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
      try {
        try {
          execSync('db-migrate', { stdio: 'pipe' });
        } catch {
          execSync('devenv shell db-migrate', { stdio: 'inherit' });
        }
      } catch (err) {
        console.warn('Warning: db-migrate invocation failed:', err);
      }
      return;
    }
    await new Promise((r) => setTimeout(r, 1000));
  }

  throw new Error('Database (PostgreSQL) failed to start on localhost:5432 within 15 seconds.');
}

/**
 * Ensures backend microservices are running and healthy.
 * Automatically invokes devenv apis-start if any service is offline,
 * and polls for up to 180 seconds to allow time for compilation and startup.
 */
async function ensureBackendServicesRunning(): Promise<void> {
  const services = [
    { name: 'booking_api', url: 'http://127.0.0.1:8081/health' },
    { name: 'listing_api', url: 'http://127.0.0.1:8082/health' },
    { name: 'user_api', url: 'http://127.0.0.1:8083/health' },
  ];

  const checkAll = async (): Promise<boolean> => {
    for (const svc of services) {
      try {
        const res = await fetch(svc.url);
        if (!res.ok) return false;
      } catch {
        return false;
      }
    }
    return true;
  };

  if (await checkAll()) {
    console.log('✓ All backend APIs are healthy and responding.');
    return;
  }

  console.log('Backend APIs are not responding. Starting via devenv apis-start script...');
  try {
    try {
      execSync('apis-start', { stdio: 'pipe' });
    } catch {
      try {
        execSync('devenv shell apis-start', { stdio: 'inherit' });
      } catch {
        execSync('devenv up -d listing_api booking_api user_api', { stdio: 'inherit' });
      }
    }
  } catch (err) {
    console.warn('Failed to invoke apis-start script synchronously, polling for readiness...', err);
  }

  // Poll for up to 180 seconds to allow for compilation and database migration
  const timeoutMs = 180000;
  const start = Date.now();
  let lastLoggedSec = 0;

  while (Date.now() - start < timeoutMs) {
    if (await checkAll()) {
      console.log('✓ All backend APIs are now ready and accepting connections.');
      return;
    }
    const elapsedSec = Math.floor((Date.now() - start) / 1000);
    if (elapsedSec - lastLoggedSec >= 10) {
      console.log(`Waiting for backend APIs to compile and start... (${elapsedSec}s elapsed)`);
      lastLoggedSec = elapsedSec;
    }
    await new Promise((r) => setTimeout(r, 1000));
  }

  throw new Error('Fatal: Backend APIs failed to become ready within 180 seconds.');
}

/**
 * Ensures Playwright skips Nix-incompatible host dependency checks for cached browsers
 * by creating the DEPENDENCIES_VALIDATED marker file.
 */
function ensureBrowserDependenciesMarker(): void {
  const homeDir = process.env.HOME || '/home/pav';
  const msPlaywrightDir = path.join(homeDir, '.cache', 'ms-playwright');
  if (fs.existsSync(msPlaywrightDir)) {
    const entries = fs.readdirSync(msPlaywrightDir);
    for (const entry of entries) {
      const fullPath = path.join(msPlaywrightDir, entry);
      if (fs.statSync(fullPath).isDirectory() && entry.startsWith('firefox-')) {
        const marker = path.join(fullPath, 'DEPENDENCIES_VALIDATED');
        if (!fs.existsSync(marker)) {
          fs.writeFileSync(marker, '');
        }
      }
    }
  }
}

/**
 * Playwright Global Setup Hook
 */
export default async function globalSetup(): Promise<void> {
  console.log('\n=== [Playwright Global Setup] Checking Environment & Infrastructure ===');

  // 0. Ensure browser validation markers exist for Nix environment
  ensureBrowserDependenciesMarker();

  // 1. Ensure DB is running
  await ensureDatabaseRunning();

  // 2. Ensure Backend APIs are running
  await ensureBackendServicesRunning();

  // 3. Seed database with deterministic test entities (villa, test users)
  await seedTestData();

  // 4. Ensure .auth directory exists for session state caching
  const authDir = path.resolve(__dirname, '../../.auth');
  if (!fs.existsSync(authDir)) {
    fs.mkdirSync(authDir, { recursive: true });
  }

  console.log('=== [Playwright Global Setup] Completed ===\n');
}
