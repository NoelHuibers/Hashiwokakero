// The device the tests run on, and the app on it: the debug APK of app/src-tauri.
import { writeFileSync } from 'node:fs';
import path from 'node:path';

import { _android, type AndroidDevice } from '@playwright/test';

import { ROOT } from '../env';

/** The debug build's application ID (`debugApplicationIdSuffix` in tauri.conf.json). */
export const PKG = 'rs.hashi.debug';
/** The launcher's activity: the debug ID, the class in the package of the code, `rs.hashi`. */
export const ACTIVITY = `${PKG}/rs.hashi.MainActivity`;
/** Where the WebView's page runs (`useHttpsScheme`), app/README.md. */
export const ORIGIN = 'https://tauri.localhost';
/** The server the app calls when it's online. The tests never let it get there. */
export const SERVER = 'hashi.rs';

/** `cargo tauri android build --debug --apk` writes it, CI builds it for the emulator (x86_64). */
export const APK = path.resolve(
  ROOT,
  process.env.E2E_APK ?? 'app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk',
);
/** What the job summary shows about the device: its model, Android and WebView. */
export const DEVICE_INFO = path.join(import.meta.dirname, '..', 'test-results-android-device.json');
/** The screenshots of the real app, an artifact of the CI job. */
export const SCREENS = path.join(import.meta.dirname, '..', 'android-screens');

/** The one device on adb (Playwright's own driver app isn't needed for a WebView). */
export async function connect(): Promise<AndroidDevice> {
  const devices = await _android.devices({ omitDriverInstall: true });
  if (devices.length !== 1) {
    await Promise.all(devices.map((d) => d.close()));
    throw new Error(`expected one Android device on adb, found ${devices.length}`);
  }
  return devices[0];
}

export const shell = async (device: AndroidDevice, command: string) => (await device.shell(command)).toString().trim();

/** Whether the device reaches the internet: an address, or a name to look up. */
async function online(device: AndroidDevice) {
  for (const host of ['1.1.1.1', SERVER]) {
    if ((await shell(device, `ping -c 1 -W 2 ${host} > /dev/null 2>&1; echo $?`)) === '0') return true;
  }
  return false;
}

/**
 * Cuts the device off: airplane mode, Wi-Fi and mobile data off, then waits until nothing
 * answers. The debug APK calls the production server when it can, and the tests must not
 * make rows there (a guest, a started daily).
 */
export async function goOffline(device: AndroidDevice) {
  await shell(device, 'cmd connectivity airplane-mode enable');
  await shell(device, 'svc wifi disable');
  await shell(device, 'svc data disable');
  for (let i = 0; i < 30; i++) {
    if (!(await online(device))) return;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  throw new Error('the device is still online with airplane mode on');
}

/** Notes the device for the job summary. */
export async function describe(device: AndroidDevice) {
  const webView = (await shell(device, 'dumpsys webviewupdate')).match(/Current WebView package \(name, version\): \((.*)\)/);
  const info = {
    model: device.model(),
    android: await shell(device, 'getprop ro.build.version.release'),
    sdk: await shell(device, 'getprop ro.build.version.sdk'),
    abi: await shell(device, 'getprop ro.product.cpu.abi'),
    webView: webView?.[1] ?? 'unknown',
  };
  writeFileSync(DEVICE_INFO, JSON.stringify(info, null, 2));
  return info;
}
