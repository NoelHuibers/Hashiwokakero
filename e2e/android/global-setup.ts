// Before the tests: the device offline first, then the APK installed. The app never starts
// with a network.
import { existsSync, mkdirSync } from 'node:fs';

import { ACTIVITY, APK, connect, describe, goOffline, PKG, SCREENS, shell } from './device';

export default async function globalSetup() {
  if (!existsSync(APK)) throw new Error(`no APK at ${APK}, see app/README.md (E2E_APK)`);
  mkdirSync(SCREENS, { recursive: true });
  const device = await connect();
  try {
    await goOffline(device);
    await device.installApk(APK);
    // The first start after the install does work of its own (the WebView's first run, the app's
    // profile): once here, offline too, until the WebView is up
    await shell(device, `am start -W -n ${ACTIVITY}`);
    await device.webView({ pkg: PKG }, { timeout: 60_000 });
    await shell(device, `am force-stop ${PKG}`);
    const info = await describe(device);
    console.log(`${info.model}, Android ${info.android} (API ${info.sdk}, ${info.abi}), WebView ${info.webView}`);
  } finally {
    await device.close();
  }
}
