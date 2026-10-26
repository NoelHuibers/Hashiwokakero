// The simulator the tests run on, and the app on it: the debug build of app/src-tauri for the
// simulator. Its commands go through `xcrun simctl`; what needs the running app (its page, which
// app is in front) goes through Appium (app.ts).
import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { openSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';

import { ROOT } from '../env';

/** The app's bundle ID (`identifier` in tauri.conf.json; iOS's debug build has no suffix). */
export const BUNDLE = 'rs.hashi';
/** The app's executable (`productName` in tauri.conf.json), the name of its process. */
export const PROCESS = 'Hashi';
/** Where the WKWebView's page runs on iOS, app/README.md. */
export const ORIGIN = 'tauri://localhost';
/** The server the app calls when it's online. The tests never let it get there. */
export const SERVER = 'hashi.rs';
/** The browser the links out of the app open in. */
export const SAFARI = 'com.apple.mobilesafari';

/** `cargo tauri ios build --debug --target aarch64-sim` writes it, CI keeps it as an artifact. */
export const APP = path.resolve(ROOT, process.env.E2E_IOS_APP ?? 'app/src-tauri/gen/apple/build/arm64-sim/Hashi.app');
/** The simulator: its UDID, or the one that's booted. */
export const SIMULATOR = process.env.E2E_SIMULATOR ?? 'booted';
/** An Appium server that is already running; without it the global setup starts one. */
export const APPIUM_URL = process.env.E2E_APPIUM_URL ?? 'http://127.0.0.1:4723';
/** A WebDriverAgent built for the simulator (`appium driver run xcuitest download-wda`), else Appium builds one. */
export const WDA = process.env.E2E_WDA ? path.resolve(ROOT, process.env.E2E_WDA) : undefined;

/** What the job summary shows about the device: its model and iOS. */
export const DEVICE_INFO = path.join(import.meta.dirname, '..', 'test-results-ios-device.json');
/** Appium's log, for a failed run's report. */
export const APPIUM_LOG = path.join(import.meta.dirname, '..', 'test-results-ios-appium.log');
/** The screenshots of the real app, an artifact of the CI job. */
export const SCREENS = path.join(import.meta.dirname, '..', 'ios-screens');

const run = promisify(execFile);

/** `xcrun simctl <args>`, its output. */
export async function simctl(...args: string[]) {
  const { stdout } = await run('xcrun', ['simctl', ...args], { maxBuffer: 64 * 1024 * 1024 });
  return stdout.trim();
}

/** The simulator's UDID, model and iOS (`simctl list`). */
export async function device() {
  const list = JSON.parse(await simctl('list', 'devices', '--json')) as {
    devices: Record<string, { udid: string; name: string; state: string; deviceTypeIdentifier?: string }[]>;
  };
  const all = Object.entries(list.devices).flatMap(([runtime, devices]) => devices.map((d) => ({ ...d, runtime })));
  const found = all.filter((d) => (SIMULATOR === 'booted' ? d.state === 'Booted' : d.udid === SIMULATOR));
  if (found.length !== 1) throw new Error(`expected one simulator for ${SIMULATOR}, found ${found.length} (E2E_SIMULATOR)`);
  const [d] = found;
  // com.apple.CoreSimulator.SimRuntime.iOS-26-5
  const ios = d.runtime.replace(/^.*\.iOS-/, '').replaceAll('-', '.');
  return { udid: d.udid, name: d.name, state: d.state, ios };
}

/**
 * Saves the screen as `name` in SCREENS. `simctl io screenshot` hung for minutes once: it gets
 * 30 s, and a screenshot that fails fails no test, it's for people.
 */
export async function screenshot(udid: string, name: string) {
  try {
    await run('xcrun', ['simctl', 'io', udid, 'screenshot', path.join(SCREENS, name)], { timeout: 30_000 });
  } catch (e) {
    console.warn(`no screenshot ${name}: ${(e as Error).message}`);
  }
}

/** Notes the device for the job summary. */
export async function describe(udid: string) {
  const d = await device();
  const info = { model: d.name, ios: d.ios, udid };
  writeFileSync(DEVICE_INFO, JSON.stringify(info, null, 2));
  return info;
}

/** Whether `url` answers: the Appium server's status. */
async function up(url: string) {
  try {
    return (await fetch(`${url}/status`, { signal: AbortSignal.timeout(2000) })).ok;
  } catch {
    return false;
  }
}

/**
 * Starts Appium (`appium` on the PATH, with its XCUITest driver installed) unless one answers at
 * APPIUM_URL, and waits until it answers. Its log goes to APPIUM_LOG.
 */
export async function startAppium(): Promise<ChildProcess | null> {
  if (await up(APPIUM_URL)) return null;
  const { port, hostname } = new URL(APPIUM_URL);
  const log = openSync(APPIUM_LOG, 'w');
  const appium = spawn('appium', ['--address', hostname, '--port', port, '--log-timestamp', '--log-no-colors'], {
    stdio: ['ignore', log, log],
  });
  const exited = new Promise<never>((_, reject) =>
    appium.once('exit', (code) => reject(new Error(`appium exited with ${code}, see ${APPIUM_LOG}`))),
  );
  const ready = (async () => {
    for (let i = 0; i < 120; i++) {
      if (await up(APPIUM_URL)) return;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    throw new Error(`appium didn't answer within a minute, see ${APPIUM_LOG}`);
  })();
  await Promise.race([ready, exited]);
  // Once it answered, its end is the teardown's
  exited.catch(() => {});
  return appium;
}
