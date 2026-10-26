// Before the tests: hashi.rs out of reach first, then the app installed and Appium started. The
// simulator has the Mac's network, which has no airplane mode: blocking hashi.rs on the Mac
// (/etc/hosts, as CI does) is what keeps the app off the production server.
import { existsSync, mkdirSync } from 'node:fs';

import { APP, describe, device, SCREENS, SERVER, simctl, startAppium } from './device';

/** Whether the server answers from here, and so from the simulator. */
async function reachable() {
  try {
    await fetch(`https://${SERVER}/`, { signal: AbortSignal.timeout(10_000) });
    return true;
  } catch {
    return false;
  }
}

export default async function globalSetup() {
  if (!existsSync(APP)) throw new Error(`no app at ${APP}, see app/README.md (E2E_IOS_APP)`);
  if (await reachable()) throw new Error(`${SERVER} answers: block it first (app/README.md), the debug build calls the production server`);
  mkdirSync(SCREENS, { recursive: true });
  const d = await device();
  if (d.state !== 'Booted') throw new Error(`the simulator ${d.name} isn't booted`);
  // Also replaces an older build, with its data
  await simctl('install', d.udid, APP);
  const info = await describe(d.udid);
  console.log(`${info.model}, iOS ${info.ios} (${info.udid})`);
  const appium = await startAppium();
  return async () => {
    appium?.kill();
  };
}
