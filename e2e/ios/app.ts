// The `test` of the iOS specs: every test starts the installed app afresh (no saved games,
// settings or session) and gets the page in its WKWebView as `app`, through Appium (its XCUITest
// driver, the WebKit inspector of the simulator: debug builds' WKWebViews are inspectable). Like
// e2e/android/app.ts it fails a test on any error in the page, a content security policy
// violation, and any answer from the server, which the app must not reach.
import { test as base, expect, type TestInfo } from '@playwright/test';

import { APPIUM_URL, BUNDLE, device, ORIGIN, PROCESS, screenshot, SERVER, WDA } from './device';
import { NATIVE, Session } from './webdriver';

export { expect };

/**
 * WebKit's console messages about requests that couldn't go out, expected with hashi.rs blocked:
 * the address doesn't answer, and the CORS check of a call that failed.
 */
const NETWORK_ERROR = /Could not connect to the server|hostname could not be found|network connection was lost|Internet connection appears to be offline|due to access control checks|Load failed/;

/** A console message of the page, as the WebKit inspector reports it (Appium's `safariConsole` log). */
interface ConsoleMessage {
  source?: string;
  level?: string;
  text?: string;
  url?: string;
  line?: number;
}

/** What `instrument` keeps in the page, for the checks after the test. */
interface Records {
  /** Every `fetch` of the page since it loaded: the server functions, Tauri's `ipc://` commands. */
  calls: string[];
  /** Their answers: the status and the URL. */
  answers: string[];
  /** The fetches without an answer or an error yet, and when the last one started or ended. */
  pending: number;
  last: number;
  errors: string[];
  csp: string[];
}

/** Which element to click: the first visible one of `css` whose accessible name is (or matches) `name`. */
export interface Target {
  css: string;
  name?: string | RegExp;
}

/** The app's page in its WKWebView, driven through the WebKit inspector. */
export class App {
  constructor(
    readonly session: Session,
    readonly udid: string,
  ) {}

  /** Runs `fn` with `arg` in the page and returns its result (awaited, as JSON). */
  async evaluate<R, A = null>(fn: (arg: A) => R | Promise<R>, arg?: A): Promise<R> {
    return this.session.execute<R>(`return (${fn.toString()}).call(null, arguments[0]);`, [arg ?? null]);
  }

  url() {
    return this.evaluate(() => location.href);
  }

  /** Opens a page of the app, a new document like a reload, and waits until it hydrated. */
  async visit(path: string) {
    const mark = await this.evaluate(() => {
      const mark = String(Math.random());
      Object.assign(window, { e2eDocument: mark });
      return mark;
    });
    // WebDriver's navigation waits for the load event: a script sent while the page changes waits
    // for the next document, up to 20 s
    await this.session.command('POST', '/url', { url: `${ORIGIN}${path}` });
    await expect
      .poll(() => this.evaluate((m) => (window as unknown as { e2eDocument?: string }).e2eDocument !== m, mark).catch(() => false), {
        message: `the page left for ${path}`,
        timeout: 30_000,
      })
      .toBe(true);
    await this.hydrated();
  }

  /**
   * Waits for the mark App sets once the WASM rendered the page (src/lib.rs), then keeps what the
   * page does from then on (`records`).
   */
  async hydrated() {
    await expect
      .poll(() => this.evaluate(() => document.documentElement.hasAttribute('data-hydrated')).catch(() => false), {
        message: 'the page hydrated',
        timeout: 60_000,
      })
      .toBe(true);
    await this.instrument();
  }

  /**
   * Notes the page's fetches and their answers, errors and the content security policy's
   * violations from now on, until the next document. WebDriver can't run a script before the
   * page's own, as Playwright's `addInitScript` does: what happened before is in the inspector's
   * console log (`problems`).
   */
  async instrument() {
    await this.evaluate(() => {
      if ('e2e' in window) return;
      const records: Records = { calls: [], answers: [], pending: 0, last: Date.now(), errors: [], csp: [] };
      Object.defineProperty(window, 'e2e', { value: records });
      const fetch = window.fetch;
      window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
        const url = input instanceof Request ? input.url : String(input);
        records.calls.push(url);
        records.pending++;
        records.last = Date.now();
        const done = () => {
          records.pending--;
          records.last = Date.now();
        };
        return fetch.call(window, input, init).then(
          (r) => {
            done();
            records.answers.push(`${r.status} ${url}`);
            return r;
          },
          (e: unknown) => {
            done();
            throw e;
          },
        );
      };
      addEventListener('error', (e) => records.errors.push(`page error: ${e.message}`));
      addEventListener('unhandledrejection', (e) => records.errors.push(`unhandled rejection: ${String(e.reason)}`));
      document.addEventListener('securitypolicyviolation', (e) =>
        records.csp.push(`${e.effectiveDirective} blocked ${e.blockedURI || 'inline'} (${e.sourceFile || e.documentURI}:${e.lineNumber})`),
      );
    });
  }

  /** Waits until the page's fetches ended, half a second ago (Playwright's `networkidle`). */
  async idle() {
    await expect
      .poll(() => this.evaluate(() => {
        const r = (window as unknown as { e2e: Records }).e2e;
        return r.pending === 0 && Date.now() - r.last > 500;
      }), { message: 'no fetch for half a second' })
      .toBe(true);
  }

  /** What `instrument` kept since the page loaded. */
  records() {
    return this.evaluate(() => (window as unknown as { e2e?: Records }).e2e ?? null);
  }

  /** Clicks the first visible element `target` names, with WebDriver's click (a user's gesture). */
  async click(target: Target) {
    const mark = await this.evaluate(
      ({ css, name, regex }) => {
        const matches = (el: Element) => {
          if (name === null) return true;
          // The text that shows (without a translation hidden on narrow screens), or all of it
          // (without the capitals of a `text-transform`)
          const labels = [el.getAttribute('aria-label'), (el as HTMLElement).innerText, el.textContent].map((l) => (l ?? '').trim());
          return labels.some((l) => (regex ? new RegExp(name).test(l) : l === name));
        };
        const el = [...document.querySelectorAll(css)].find((e) => e.getClientRects().length > 0 && matches(e));
        if (!el) return null;
        const mark = String(Math.random()).slice(2);
        el.setAttribute('data-e2e-click', mark);
        return mark;
      },
      {
        css: target.css,
        name: target.name instanceof RegExp ? target.name.source : (target.name ?? null),
        regex: target.name instanceof RegExp,
      },
    );
    expect(mark, `a visible ${target.css} named ${target.name}`).not.toBeNull();
    await this.session.click(await this.session.find(`[data-e2e-click="${mark}"]`));
  }

  /** How many visible elements `css` finds whose text contains `text`. */
  count(css: string, text = '') {
    return this.evaluate(
      ({ css, text }) => [...document.querySelectorAll(css)].filter((e) => e.getClientRects().length > 0 && ((e as HTMLElement).innerText ?? e.textContent ?? '').includes(text)).length,
      { css, text },
    );
  }

  /** The text of the first element `css` finds, `null` without one. */
  text(css: string) {
    return this.evaluate((css) => document.querySelector(css)?.textContent?.trim() ?? null, css);
  }

  /** Saves the screen as `name` in the screenshots of the job. */
  screenshot(name: string) {
    return screenshot(this.udid, name);
  }
}

export const test = base.extend<{ app: App; problems: string[] }, { session: Session; udid: string }>({
  udid: [async ({}, use) => use((await device()).udid), { scope: 'worker' }],

  /**
   * One Appium session for all tests. It starts WebDriverAgent on the simulator (the prebuilt
   * one of E2E_WDA, else Appium builds it), not the app: `app` does.
   */
  session: [
    async ({ udid }, use) => {
      const d = await device();
      const session = await Session.create(APPIUM_URL, {
        platformName: 'iOS',
        'appium:automationName': 'XCUITest',
        'appium:udid': udid,
        'appium:deviceName': d.name,
        'appium:platformVersion': d.ios,
        'appium:bundleId': BUNDLE,
        // The WebKit inspector lists the app's WKWebView by its process's name, not the bundle ID
        'appium:additionalWebviewBundleIds': [`process-${PROCESS}`],
        'appium:autoLaunch': false,
        'appium:noReset': true,
        'appium:newCommandTimeout': 600,
        'appium:wdaLaunchTimeout': 180_000,
        'appium:webviewConnectTimeout': 20_000,
        'appium:reduceMotion': true,
        // Appium waits for the simulator's boot itself, which took over two minutes once after
        // `simctl bootstatus` said it had booted
        'appium:simulatorStartupTimeout': 300_000,
        // The page's console from the inspector, kept for the `safariConsole` log (`true` also
        // writes it into Appium's log, 60 MB of it)
        'appium:showSafariConsoleLog': false,
        ...(WDA ? { 'appium:usePreinstalledWDA': true, 'appium:prebuiltWDAPath': WDA } : {}),
      });
      await use(session);
      await session.delete().catch(() => {});
    },
    { scope: 'worker', timeout: 480_000 },
  ],

  app: async ({ session, udid }, use, testInfo) => {
    const app = await launch(session, udid, testInfo);
    await use(app);
    if (testInfo.status !== testInfo.expectedStatus) {
      const shot = `failed-${testInfo.title.replace(/\W+/g, '-')}-${testInfo.retry}.png`;
      await app.screenshot(shot).catch(() => {});
    }
  },

  problems: [
    async ({ app }, use, testInfo) => {
      const problems: string[] = [];
      await use(problems);
      // The inspector's console since the page was selected: the app's start included
      const console = (await app.session.logs('safariConsole').catch(() => []))
        .map((e) => {
          try {
            return JSON.parse(e.message) as ConsoleMessage;
          } catch {
            return { text: e.message } as ConsoleMessage;
          }
        });
      if (console.length) {
        const lines = console.map((m) => `${m.level ?? ''} ${m.source ?? ''}: ${m.text ?? ''}${m.url ? ` (${m.url}:${m.line})` : ''}`);
        await testInfo.attach('console', { body: lines.join('\n'), contentType: 'text/plain' });
      }
      for (const m of console) {
        const text = m.text ?? '';
        if (/Content.Security.Policy/i.test(text)) problems.push(`CSP: ${text}`);
        else if (m.level === 'error' && !NETWORK_ERROR.test(text)) problems.push(`console error: ${text}`);
      }
      // A test that stopped the app or left its page has nothing to read
      const records = await app.records().catch(() => null);
      if (records) {
        problems.push(...records.errors, ...records.csp.map((v) => `CSP: ${v}`));
        // hashi.rs is blocked: an answer from it means a request got out
        problems.push(...records.answers.filter((a) => new URL(a.split(' ')[1]).hostname === SERVER).map((a) => `reached the server: ${a}`));
      }
      expect(problems, 'errors in the app').toEqual([]);
    },
    { auto: true },
  ],
});

/**
 * Starts the app with its data cleared and waits for its page to hydrate: Appium finds the page
 * in the WebKit inspector once the WKWebView loaded it (`mobile: getContexts`).
 */
async function launch(session: Session, udid: string, testInfo: TestInfo): Promise<App> {
  return test.step('start the app', async () => {
    await session.setContext(NATIVE);
    // Ends the app, then empties its data container: local storage, WebKit's caches
    await session.mobile('clearApp', { bundleId: BUNDLE });
    const started = Date.now();
    await session.mobile('launchApp', { bundleId: BUNDLE });
    let context: string | undefined;
    await expect
      .poll(
        async () => {
          const contexts = await session.mobile<{ id: string; url?: string; bundleId?: string }[]>('getContexts', { waitForWebviewMs: 5000 });
          context = contexts.find((c) => c.id !== NATIVE && c.url?.startsWith(ORIGIN))?.id;
          return context ? 'found' : JSON.stringify(contexts);
        },
        { message: "the app's page in the WebKit inspector", timeout: 60_000 },
      )
      .toBe('found');
    await session.setContext(context!);
    const app = new App(session, udid);
    await app.hydrated();
    testInfo.annotations.push({ type: 'launch', description: `${Date.now() - started} ms` });
    return app;
  });
}

/** The app in front: its bundle ID (`mobile: activeAppInfo`, WebDriverAgent's view). */
export async function inFront(session: Session) {
  return (await session.mobile<{ bundleId: string }>('activeAppInfo')).bundleId;
}
