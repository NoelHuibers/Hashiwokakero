// Before the tests: the device offline first, then the APK installed. The app never starts
// with a network.
import { existsSync, mkdirSync } from 'node:fs';

import { APK, connect, describe, goOffline, SCREENS } from './device';

export default async function globalSetup() {
  if (!existsSync(APK)) throw new Error(`no APK at ${APK}, see app/README.md (E2E_APK)`);
  mkdirSync(SCREENS, { recursive: true });
  const device = await connect();
  try {
    await goOffline(device);
    await device.installApk(APK);
    const info = await describe(device);
    console.log(`${info.model}, Android ${info.android} (API ${info.sdk}, ${info.abi}), WebView ${info.webView}`);
  } finally {
    await device.close();
  }
}
