// The few WebDriver commands the iOS tests send to Appium (its XCUITest driver), over plain
// HTTP: a session, its contexts (the app's native views, or the page in its WKWebView), scripts in
// the page, elements and clicks, the driver's `mobile:` commands and logs. WebdriverIO would bring
// 216 packages into every job's install for these.

/** Appium's answer to a command that failed: the WebDriver error and its message. */
export class WebDriverError extends Error {
  constructor(
    readonly error: string,
    message: string,
  ) {
    super(`${error}: ${message}`);
  }
}

/** The key of an element in WebDriver's JSON. */
const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';

/** The context of the app's native views (WDA); the page's are `WEBVIEW_<id>`. */
export const NATIVE = 'NATIVE_APP';

async function send<T>(url: string, method: string, body?: unknown, timeoutMs = 120_000): Promise<T> {
  const response = await fetch(url, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const text = await response.text();
  let json: { value?: unknown };
  try {
    json = JSON.parse(text);
  } catch {
    throw new Error(`${method} ${url}: ${response.status} ${text.slice(0, 500)}`);
  }
  const value = json.value as { error?: string; message?: string } | undefined;
  if (!response.ok || (value && typeof value === 'object' && typeof value.error === 'string')) {
    throw new WebDriverError(value?.error ?? `HTTP ${response.status}`, value?.message ?? text.slice(0, 500));
  }
  return json.value as T;
}

export class Session {
  private constructor(
    readonly base: string,
    readonly id: string,
    readonly capabilities: Record<string, unknown>,
  ) {}

  /** A new session on the Appium server at `server`, with `capabilities` (`appium:` ones included). */
  static async create(server: string, capabilities: Record<string, unknown>): Promise<Session> {
    // Waiting for the simulator, installing and starting WebDriverAgent take minutes on a busy runner
    const created = await send<{ sessionId: string; capabilities: Record<string, unknown> }>(
      `${server}/session`,
      'POST',
      { capabilities: { alwaysMatch: capabilities, firstMatch: [{}] } },
      480_000,
    );
    return new Session(`${server}/session/${created.sessionId}`, created.sessionId, created.capabilities);
  }

  command<T>(method: 'GET' | 'POST' | 'DELETE', path: string, body?: unknown): Promise<T> {
    return send<T>(`${this.base}${path}`, method, method === 'POST' ? (body ?? {}) : undefined);
  }

  delete() {
    return send(this.base, 'DELETE');
  }

  contexts() {
    return this.command<string[]>('GET', '/appium/contexts');
  }

  context() {
    return this.command<string>('GET', '/appium/context');
  }

  setContext(name: string) {
    return this.command('POST', '/appium/context', { name });
  }

  /** A driver's extension (`mobile: <name>`), in any context. */
  mobile<T = unknown>(name: string, args: Record<string, unknown> = {}) {
    return this.command<T>('POST', '/execute/sync', { script: `mobile: ${name}`, args: [args] });
  }

  /** Runs `script`, a function's body, in the page; a promise it returns is awaited. */
  execute<T>(script: string, args: unknown[] = []) {
    return this.command<T>('POST', '/execute/sync', { script, args });
  }

  /** The page's URL, in the page's context. */
  url() {
    return this.command<string>('GET', '/url');
  }

  /** The first element `selector` (CSS) finds in the page, or an error. */
  async find(selector: string): Promise<string> {
    const found = await this.command<Record<string, string>>('POST', '/element', { using: 'css selector', value: selector });
    return found[ELEMENT];
  }

  /** A click on an element of the page (WebDriver's atom: mouse events, as a user's gesture). */
  click(element: string) {
    return this.command('POST', `/element/${element}/click`);
  }

  /** The log `type`'s entries since the last call (`safariConsole`, `syslog`, ...). */
  logs(type: string) {
    return this.command<{ timestamp: number; level: string; message: string }[]>('POST', '/se/log', { type });
  }

  /** The screen, as a PNG. */
  async screenshot(): Promise<Buffer> {
    return Buffer.from(await this.command<string>('GET', '/screenshot'), 'base64');
  }
}
