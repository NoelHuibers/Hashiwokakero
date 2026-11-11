// node --test scripts/tests/: the Microsoft Store's package and upload (scripts/microsoft-store.ts):
// the manifest, the package folder, the bundle's checks, and the release and the listing against a
// fake of the submission API and of the Azure blob it uploads to
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';
import zlib from 'node:zlib';

import {
  ASSETS,
  accessToken,
  attributes,
  BLOB_VERSION,
  bundleInfo,
  bundleProblems,
  type Connection,
  compareVersions,
  environment,
  errorMessage,
  identityOf,
  LIMITS,
  layout,
  listing,
  listingImages,
  listingTexts,
  listingWith,
  MICROSOFT,
  manifest,
  msixVersion,
  PACKAGE_LANGUAGES,
  PLACEHOLDER,
  planListing,
  publishedState,
  RESOURCE,
  release,
  releaseNotes,
  replacePackages,
  textProblems,
  writeZip,
  zipEntries,
  zipFile,
  zipRead,
} from '../microsoft-store.ts';

const API = 'https://manage.devcenter.microsoft.com/v1.0/my';
const TOKEN_URL = 'https://login.microsoftonline.com/tenant/oauth2/token';
const BLOB = 'https://blob.example/upload/123?sv=2023&sig=secret';
const APP = '9NBLGGH4R315';
const IDENTITY = {
  name: '12345NoelHuibers.Hashi',
  publisher: 'CN=0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9',
  publisherDisplayName: 'Noel Huibers',
  displayName: 'Hashi',
};

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});
const tmp = () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'microsoft-store-test-'));
  dirs.push(dir);
  return dir;
};

// --- The package ----------------------------------------------------------------------------

test("versions: x.y.z.0, the fourth part is the Store's", () => {
  assert.equal(msixVersion('1.2.0'), '1.2.0.0');
  assert.equal(msixVersion('10.65535.3'), '10.65535.3.0');
  for (const wrong of ['0.1.0', '1.2', '1.2.3.4', '1.65536.0', 'v1.2.0'])
    assert.throws(() => msixVersion(wrong), /isn't x\.y\.z/, wrong);
  assert.ok(compareVersions('1.10.0.0', '1.9.0.0') > 0);
  assert.equal(compareVersions('1.2.0.0', '1.2.0.0'), 0);
  assert.ok(compareVersions('1.2.0.0', '1.2.1.0') < 0);
});

test("the identity: Partner Center's from the environment, a placeholder without it", () => {
  const env = {
    MS_STORE_IDENTITY_NAME: IDENTITY.name,
    MS_STORE_PUBLISHER: ` ${IDENTITY.publisher} `,
  };
  assert.deepEqual(identityOf(env), { ...IDENTITY, placeholder: false });
  assert.deepEqual(
    identityOf({ ...env, MS_STORE_DISPLAY_NAME: 'Hashi Bridges' }).displayName,
    'Hashi Bridges',
  );
  const placeholder = identityOf({ MS_STORE_IDENTITY_NAME: IDENTITY.name });
  assert.equal(placeholder.placeholder, true);
  assert.equal(placeholder.name, PLACEHOLDER.name);
  assert.equal(placeholder.publisher, PLACEHOLDER.publisher);
  assert.throws(
    () => identityOf({ ...env, MS_STORE_IDENTITY_NAME: 'Noel Huibers.Hashi' }),
    /isn't a package name/,
  );
  assert.throws(
    () => identityOf({ ...env, MS_STORE_PUBLISHER: '0A1B2C3D-4E5F' }),
    /isn't a publisher \(CN=/,
  );
});

test('the manifest: identity, version, desktop, full trust, the languages, the logos', () => {
  const text = manifest({
    ...IDENTITY,
    publisherDisplayName: 'Noel & "Co"',
    version: '1.2.0',
    arch: 'arm64',
    description: 'Link the <islands>',
  });
  assert.deepEqual(attributes(text, 'Identity'), {
    Name: IDENTITY.name,
    Publisher: IDENTITY.publisher,
    Version: '1.2.0.0',
    ProcessorArchitecture: 'arm64',
  });
  assert.match(text, /<PublisherDisplayName>Noel &amp; &quot;Co&quot;<\/PublisherDisplayName>/);
  assert.match(text, /Description="Link the &lt;islands&gt;"/);
  assert.deepEqual(attributes(text, 'TargetDeviceFamily'), {
    Name: 'Windows.Desktop',
    MinVersion: '10.0.17763.0',
    MaxVersionTested: '10.0.26100.0',
  });
  assert.deepEqual(attributes(text, 'Application'), {
    Id: 'Hashi',
    Executable: 'hashi.exe',
    EntryPoint: 'Windows.FullTrustApplication',
  });
  assert.deepEqual(
    [...text.matchAll(/<Resource Language="([^"]+)"/g)].map((m) => m[1]),
    [...PACKAGE_LANGUAGES],
  );
  assert.match(text, /<Capability Name="internetClient" \/>/);
  assert.match(text, /<rescap:Capability Name="runFullTrust" \/>/);
  for (const logo of text.matchAll(/Assets\\([\w.]+\.png)/g))
    assert.ok(
      ASSETS.some(([file]) => file === logo[1]),
      `${logo[1]} is in the package's assets`,
    );
  // One language per language of the app (locales/)
  assert.equal(PACKAGE_LANGUAGES.length, fs.readdirSync('locales').length);
});

test('the package folder: the program, the manifest and every logo', () => {
  const dir = tmp();
  const exe = path.join(dir, 'hashi.exe');
  fs.writeFileSync(exe, 'MZ program');
  const out = path.join(dir, 'x64');
  const files = layout({ ...IDENTITY, version: '1.2.0', arch: 'x64', exe, out });
  for (const file of files) assert.ok(fs.statSync(path.join(out, file)).isFile(), file);
  assert.equal(fs.readFileSync(path.join(out, 'hashi.exe'), 'utf8'), 'MZ program');
  const text = fs.readFileSync(path.join(out, 'AppxManifest.xml'), 'utf8');
  assert.equal(attributes(text, 'Identity')?.ProcessorArchitecture, 'x64');
  // The description of tauri.conf.json
  assert.match(text, /Description="Hashiwokakero logic puzzle/);
  // The targetsize logos are the icons of their size
  const png = (file: string) => fs.readFileSync(path.join(out, 'Assets', file)).readUInt32BE(16);
  assert.equal(png('Square44x44Logo.png'), 44);
  assert.equal(png('Square44x44Logo.targetsize-32.png'), 32);
  assert.equal(png('Square44x44Logo.targetsize-256_altform-unplated.png'), 256);
  assert.equal(png('StoreLogo.png'), 50);
  assert.throws(
    () => layout({ ...IDENTITY, version: '1.2.0', arch: 'x64', exe, out }),
    /isn't empty/,
  );
});

// --- Zip archives and the bundle ------------------------------------------------------------

test('zips: written stored, read back', () => {
  const dir = tmp();
  const a = path.join(dir, 'a.txt');
  const b = path.join(dir, 'b.bin');
  fs.writeFileSync(a, 'Hashi');
  fs.writeFileSync(b, Buffer.alloc(70_000, 7));
  const zip = path.join(dir, 'out.zip');
  writeZip(zip, [
    ['a.txt', a],
    ['de-de/1-daily.png', b],
  ]);
  const data = fs.readFileSync(zip);
  assert.deepEqual(
    zipEntries(data).map((e) => [e.name, e.method, e.size]),
    [
      ['a.txt', 0, 5],
      ['de-de/1-daily.png', 0, 70_000],
    ],
  );
  assert.equal(zipFile(data, 'a.txt')?.toString(), 'Hashi');
  assert.deepEqual(zipFile(data, 'de-de/1-daily.png'), fs.readFileSync(b));
  assert.equal(zipFile(data, 'missing'), null);
  assert.throws(() => zipEntries(Buffer.from('not a zip at all, just text here')), /not a zip/);
});

/**
 * A ZIP64 archive as MakeAppx writes one: deflated entries, sizes and offsets in the ZIP64 extra
 * field, the ZIP64 end record and its locator.
 */
function zip64(files: [string, Buffer][]): Buffer {
  const parts: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const [name, content] of files) {
    const data = zlib.deflateRawSync(content);
    const nameBytes = Buffer.from(name);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(45, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(zlib.crc32(content), 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(content.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    const extra = Buffer.alloc(28);
    extra.writeUInt16LE(0x0001, 0);
    extra.writeUInt16LE(24, 2);
    extra.writeBigUInt64LE(BigInt(content.length), 4);
    extra.writeBigUInt64LE(BigInt(data.length), 12);
    extra.writeBigUInt64LE(BigInt(offset), 20);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(45, 4);
    entry.writeUInt16LE(45, 6);
    entry.writeUInt16LE(8, 10);
    entry.writeUInt32LE(zlib.crc32(content), 16);
    entry.writeUInt32LE(0xffffffff, 20);
    entry.writeUInt32LE(0xffffffff, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt16LE(extra.length, 30);
    entry.writeUInt32LE(0xffffffff, 42);
    central.push(entry, nameBytes, extra);
    parts.push(local, nameBytes, data);
    offset += local.length + nameBytes.length + data.length;
  }
  const directory = Buffer.concat(central);
  const record = Buffer.alloc(56);
  record.writeUInt32LE(0x06064b50, 0);
  record.writeBigUInt64LE(44n, 4);
  record.writeBigUInt64LE(BigInt(files.length), 24);
  record.writeBigUInt64LE(BigInt(files.length), 32);
  record.writeBigUInt64LE(BigInt(directory.length), 40);
  record.writeBigUInt64LE(BigInt(offset), 48);
  const locator = Buffer.alloc(20);
  locator.writeUInt32LE(0x07064b50, 0);
  locator.writeBigUInt64LE(BigInt(offset + directory.length), 8);
  locator.writeUInt32LE(1, 16);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(0xffff, 8);
  end.writeUInt16LE(0xffff, 10);
  end.writeUInt32LE(0xffffffff, 12);
  end.writeUInt32LE(0xffffffff, 16);
  return Buffer.concat([...parts, directory, record, locator, end]);
}

test('zips: ZIP64 and deflated, as MakeAppx writes them', () => {
  const big = Buffer.from('<Package/>'.repeat(1000));
  const data = zip64([
    ['AppxManifest.xml', big],
    ['hashi.exe', Buffer.from('MZ')],
  ]);
  const entries = zipEntries(data);
  assert.deepEqual(
    entries.map((e) => [e.name, e.method, e.size]),
    [
      ['AppxManifest.xml', 8, big.length],
      ['hashi.exe', 8, 2],
    ],
  );
  assert.deepEqual(zipRead(data, entries[0] as (typeof entries)[0]), big);
  assert.equal(zipFile(data, 'hashi.exe')?.toString(), 'MZ');
});

/** An .msix of an architecture, as MakeAppx would make it of `layout`'s folder. */
function msix(
  arch: string,
  o: { version?: string; identity?: typeof IDENTITY; manifest?: (text: string) => string } = {},
): Buffer {
  const files: [string, Buffer][] = [
    [
      'AppxManifest.xml',
      Buffer.from(
        (o.manifest ?? ((t: string) => t))(
          manifest({
            ...(o.identity ?? IDENTITY),
            version: o.version ?? '1.2.0',
            arch: arch as 'x64',
            description: 'Hashi',
          }),
        ),
      ),
    ],
    ['hashi.exe', Buffer.from(`MZ ${arch}`)],
    ['resources.pri', Buffer.from('mrm_pri2')],
    ['Assets/Square44x44Logo.png', Buffer.from('png')],
  ];
  return zip64(files);
}

/** An .msixbundle of packages, with the bundle manifest MakeAppx writes. */
function bundle(
  packages: [file: string, arch: string, data: Buffer][],
  o: { version?: string; identity?: typeof IDENTITY } = {},
): Buffer {
  const id = o.identity ?? IDENTITY;
  const v = msixVersion(o.version ?? '1.2.0');
  const text = `<?xml version="1.0" encoding="UTF-8"?>
<Bundle SchemaVersion="5.0" IgnorableNamespaces="b4 b5" xmlns="http://schemas.microsoft.com/appx/2013/bundle">
  <Identity Name="${id.name}" Publisher="${id.publisher}" Version="${v}"/>
  <Packages>
${packages.map(([file, arch]) => `    <Package Type="application" Version="${v}" Architecture="${arch}" FileName="${file}" Offset="57" Size="1000"><Resources><Resource Language="EN-US"/></Resources></Package>`).join('\n')}
  </Packages>
</Bundle>`;
  return zip64([
    ...packages.map(([file, , data]): [string, Buffer] => [file, data]),
    ['AppxMetadata/AppxBundleManifest.xml', Buffer.from(text)],
  ]);
}

const goodBundle = () =>
  bundle([
    ['hashi-1.2.0-windows-x64.msix', 'x64', msix('x64')],
    ['hashi-1.2.0-windows-arm64.msix', 'arm64', msix('arm64')],
  ]);
const expected = { ...IDENTITY, version: '1.2.0' };

test("the bundle: the identity, x64 and arm64, every package's manifest", () => {
  const info = bundleInfo(goodBundle());
  assert.equal(info.name, IDENTITY.name);
  assert.equal(info.version, '1.2.0.0');
  assert.deepEqual(
    info.packages.map((p) => [p.file, p.arch, p.executable, p.family]),
    [
      ['hashi-1.2.0-windows-x64.msix', 'x64', 'hashi.exe', 'Windows.Desktop'],
      ['hashi-1.2.0-windows-arm64.msix', 'arm64', 'hashi.exe', 'Windows.Desktop'],
    ],
  );
  assert.deepEqual(info.packages[0]?.capabilities, ['internetClient', 'runFullTrust']);
  assert.equal(info.packages[0]?.languages.length, 20);
  assert.equal(info.packages[0]?.displayName, 'Hashi');
  assert.deepEqual(bundleProblems(info, expected), []);
});

test("the bundle's problems: another identity, an architecture missing, the version, a manifest", () => {
  const other = { ...IDENTITY, publisher: 'CN=Someone' };
  assert.match(
    bundleProblems(bundleInfo(goodBundle()), { ...expected, publisher: 'CN=Someone' }).join('\n'),
    /the bundle is "12345NoelHuibers.Hashi" by "CN=0A1B.*", the Store's app is .* by "CN=Someone"/,
  );
  const placeholder = bundle(
    [['a.msix', 'x64', msix('x64', { identity: { ...IDENTITY, ...PLACEHOLDER } })]],
    { identity: { ...IDENTITY, ...PLACEHOLDER } },
  );
  const problems = bundleProblems(bundleInfo(placeholder), expected).join('\n');
  assert.match(problems, /rs\.hashi\.placeholder/);
  assert.match(problems, /packages for x64, not x64 and arm64/);
  assert.match(
    bundleProblems(bundleInfo(goodBundle()), { ...expected, version: '1.3.0' }).join('\n'),
    /version is 1\.2\.0\.0, not 1\.3\.0\.0/,
  );
  const mixed = bundle([
    ['a.msix', 'x64', msix('x64')],
    ['b.msix', 'arm64', msix('arm64', { identity: other })],
  ]);
  assert.match(
    bundleProblems(bundleInfo(mixed), expected).join('\n'),
    /b\.msix: .*unlike the bundle/,
  );
  const plain = bundle([
    [
      'a.msix',
      'x64',
      msix('x64', { manifest: (t) => t.replace(/\s*<rescap:Capability[^>]*>/, '') }),
    ],
    [
      'b.msix',
      'arm64',
      msix('arm64', { manifest: (t) => t.replace('Windows.Desktop', 'Windows.Universal') }),
    ],
  ]);
  const found = bundleProblems(bundleInfo(plain), {
    ...expected,
    publisherDisplayName: 'Someone',
  }).join('\n');
  assert.match(found, /a\.msix: lacks the capability runFullTrust/);
  assert.match(found, /b\.msix: the device family is Windows\.Universal/);
  assert.match(found, /display name is "Noel Huibers", not "Someone"/);
  assert.throws(() => bundleInfo(msix('x64')), /not a bundle/);
});

// --- The submission API ---------------------------------------------------------------------

interface Call {
  method: string;
  url: URL;
  headers: Record<string, string>;
  json?: Record<string, unknown>;
  body?: string;
  blob?: Buffer;
}

interface State {
  app: Record<string, unknown>;
  submissions: Map<string, Record<string, unknown>>;
  /** What the blob has after the upload. */
  uploaded: Buffer | null;
  /** The statuses the commit's checks see, one each. */
  statuses: string[];
  nextId: number;
  fail?: Record<string, [number, unknown]>;
}

const listings = () => ({
  'en-us': {
    baseListing: { title: 'Hashi', description: 'old', releaseNotes: 'Hashi 1.1', images: [] },
    platformOverrides: {},
  },
  'de-de': {
    baseListing: { title: 'Hashi', description: 'alt', releaseNotes: 'Hashi 1.1', images: [] },
    platformOverrides: {},
  },
});

function newState(o: Partial<State> & { publishedVersion?: string | null; pending?: string } = {}) {
  const submissions = new Map<string, Record<string, unknown>>();
  const published = o.publishedVersion === null ? null : '100';
  if (published !== null)
    submissions.set(published, {
      id: published,
      status: 'Published',
      applicationPackages: [
        {
          fileName: 'hashi-1.1.0-windows-store.msixbundle',
          fileStatus: 'Uploaded',
          id: 'p1',
          version: o.publishedVersion ?? '1.1.0.0',
          architecture: 'Neutral',
          targetPlatforms: [{ name: 'Windows.Desktop' }],
        },
      ],
      listings: listings(),
      pricing: { priceId: 'Free' },
    });
  if (o.pending !== undefined)
    submissions.set('200', {
      ...structuredClone(submissions.get('100') ?? {}),
      id: '200',
      status: o.pending,
      fileUploadUrl: BLOB,
    });
  return {
    app: {
      id: APP,
      primaryName: 'Hashi',
      packageFamilyName: `${IDENTITY.name}_abc`,
      packageIdentityName: IDENTITY.name,
      publisherName: IDENTITY.publisher,
      ...(published === null ? {} : { lastPublishedApplicationSubmission: { id: published } }),
      ...(o.pending === undefined ? {} : { pendingApplicationSubmission: { id: '200' } }),
    },
    submissions,
    uploaded: null,
    statuses: o.statuses ?? ['CommitStarted', 'PreProcessing'],
    nextId: 300,
    ...(o.fail === undefined ? {} : { fail: o.fail }),
  } satisfies State;
}

/** A fetch that answers as the submission API, Entra's token endpoint and the blob would. */
function fake(state: State): { fetch: typeof fetch; calls: Call[] } {
  const calls: Call[] = [];
  const reply = (status: number, body?: unknown) =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: body === undefined ? {} : { 'content-type': 'application/json' },
    });
  const f = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const url = new URL(String(input));
    const method = init?.method ?? 'GET';
    const call: Call = { method, url, headers: { ...(init?.headers as Record<string, string>) } };
    if (typeof init?.body === 'string') {
      call.body = init.body;
      if (call.headers['Content-Type'] === 'application/json') call.json = JSON.parse(init.body);
    } else if (init?.body instanceof Blob) {
      call.blob = Buffer.from(await init.body.arrayBuffer());
    }
    calls.push(call);
    const route = `${method} ${url.pathname}`;
    const failure = state.fail?.[route];
    if (failure !== undefined) return reply(...failure);
    if (url.href === TOKEN_URL) {
      const form = new URLSearchParams(call.body);
      if (form.get('client_secret') !== 'secret')
        return reply(401, {
          error: 'invalid_client',
          error_description: 'AADSTS7000215: Invalid client secret provided.',
        });
      return reply(200, { token_type: 'Bearer', expires_in: '3599', access_token: 'eyJ.token' });
    }
    if (url.origin === 'https://blob.example') {
      if (url.href !== BLOB) return reply(403);
      state.uploaded = call.blob ?? null;
      return new Response(null, { status: 201 });
    }
    if (call.headers.Authorization !== 'Bearer eyJ.token')
      return reply(401, { statusCode: 401, message: 'Unauthorized' });
    const base = `/v1.0/my/applications/${APP}`;
    if (!url.pathname.startsWith(base))
      return reply(404, { code: 'NotFound', message: 'no such app' });
    const rest = url.pathname.slice(base.length);
    if (rest === '' && method === 'GET') return reply(200, state.app);
    if (rest === '/submissions' && method === 'POST') {
      if (state.app.pendingApplicationSubmission !== undefined)
        return reply(409, { code: 'InvalidState', message: 'a pending submission exists' });
      const id = String(state.nextId++);
      const published = state.submissions.get('100') ?? {};
      state.submissions.set(id, {
        ...structuredClone(published),
        id,
        status: 'PendingCommit',
        fileUploadUrl: BLOB,
      });
      state.app.pendingApplicationSubmission = { id };
      return reply(201, state.submissions.get(id));
    }
    const sub = /^\/submissions\/(\d+)(\/commit|\/status)?$/.exec(rest);
    const s = sub === null ? undefined : state.submissions.get(sub[1] ?? '');
    if (s === undefined) return reply(404, { code: 'NotFound', message: `no ${rest}` });
    if (sub?.[2] === undefined && method === 'GET') return reply(200, s);
    if (sub?.[2] === undefined && method === 'PUT') {
      if (s.status !== 'PendingCommit')
        return reply(409, { code: 'InvalidState', message: 'not a draft' });
      const { id: _, status: __, ...changes } = call.json ?? {};
      Object.assign(s, changes);
      return reply(200, s);
    }
    if (sub?.[2] === '/commit' && method === 'POST') {
      s.status = 'CommitStarted';
      return reply(202, { status: 'CommitStarted' });
    }
    if (sub?.[2] === '/status' && method === 'GET') {
      const status = state.statuses.shift() ?? 'Certification';
      if (status === 'CommitFailed')
        return reply(200, {
          status,
          statusDetails: {
            errors: [{ code: 'InvalidParameterValue', details: 'The package identity is wrong.' }],
            warnings: [],
          },
        });
      s.status = status;
      return reply(200, { status, statusDetails: { errors: [], warnings: [] } });
    }
    return reply(405, { code: 'MethodNotAllowed', message: route });
  };
  return { fetch: f as typeof fetch, calls };
}

const connection = (f: typeof fetch): Connection => ({
  fetch: f,
  api: API,
  appId: APP,
  token: 'eyJ.token',
  // No waiting between the status checks
  sleep: () => Promise.resolve(),
});
const lines: string[] = [];
const log = (line: string) => lines.push(line);

/** A bundle file and the release's options for it. */
function releaseOptions(o: { submit?: boolean; data?: Buffer } = {}) {
  const dir = tmp();
  const file = path.join(dir, 'hashi-1.2.0-windows-store.msixbundle');
  const data = o.data ?? goodBundle();
  fs.writeFileSync(file, data);
  return {
    version: '1.2.0',
    bundle: file,
    info: bundleInfo(data),
    releaseNotes: 'New: rivals',
    submit: o.submit ?? true,
    workDir: dir,
  };
}

test('the access token: client credentials for the submission API', async () => {
  const { fetch: f, calls } = fake(newState());
  assert.equal(await accessToken(f, TOKEN_URL, 'client', 'secret'), 'eyJ.token');
  assert.deepEqual(Object.fromEntries(new URLSearchParams(calls[0]?.body)), {
    grant_type: 'client_credentials',
    client_id: 'client',
    client_secret: 'secret',
    resource: 'https://manage.devcenter.microsoft.com',
  });
  assert.equal(RESOURCE, 'https://manage.devcenter.microsoft.com');
  await assert.rejects(
    accessToken(f, TOKEN_URL, 'client', 'wrong'),
    /accessToken: HTTP 401: AADSTS7000215: Invalid client secret provided\. \(MS_STORE_TENANT_ID/,
  );
});

test("a release: a new submission, the bundle as its only package, what's new, committed", async () => {
  const state = newState();
  const { fetch: f, calls } = fake(state);
  const o = releaseOptions();
  const summary = await release(connection(f), o, log);
  const base = `/v1.0/my/applications/${APP}`;
  assert.deepEqual(
    calls.map((c) => `${c.method} ${c.url.pathname.replace(base, '')}`),
    [
      'GET ',
      'GET /submissions/100',
      'POST /submissions',
      'PUT /submissions/300',
      'PUT /upload/123',
      'POST /submissions/300/commit',
      'GET /submissions/300/status',
      'GET /submissions/300/status',
    ],
  );
  const put = calls[3]?.json ?? {};
  // What the API sets itself isn't sent back
  for (const key of ['id', 'status', 'statusDetails', 'fileUploadUrl'])
    assert.equal(put[key], undefined, key);
  assert.deepEqual(put.applicationPackages, [
    {
      fileName: 'hashi-1.1.0-windows-store.msixbundle',
      fileStatus: 'PendingDelete',
      id: 'p1',
      version: '1.1.0.0',
      architecture: 'Neutral',
      targetPlatforms: [{ name: 'Windows.Desktop' }],
    },
    {
      fileName: 'hashi-1.2.0-windows-store.msixbundle',
      fileStatus: 'PendingUpload',
      minimumDirectXVersion: 'None',
      minimumSystemRam: 'None',
    },
  ]);
  const putListings = put.listings as Record<string, { baseListing: Record<string, unknown> }>;
  assert.equal(putListings['de-de']?.baseListing.releaseNotes, 'New: rivals');
  // The rest of the listing as it was
  assert.equal(putListings['de-de']?.baseListing.description, 'alt');
  assert.equal((put.pricing as Record<string, unknown>).priceId, 'Free');
  // The upload: a zip with the bundle, as Azure's Put Blob takes it
  const upload = calls[4];
  assert.equal(upload?.headers['x-ms-blob-type'], 'BlockBlob');
  assert.equal(upload?.headers['x-ms-version'], BLOB_VERSION);
  assert.ok(state.uploaded !== null);
  assert.deepEqual(
    zipFile(state.uploaded, 'hashi-1.2.0-windows-store.msixbundle'),
    fs.readFileSync(o.bundle),
  );
  assert.equal(state.submissions.get('300')?.status, 'PreProcessing');
  assert.match(summary.join('\n'), /1\.2\.0 in the submission 300.*x64, arm64.*2 languages/);
  assert.match(summary.join('\n'), /Submitted for certification \(PreProcessing\)/);
  // The zip is gone
  assert.deepEqual(fs.readdirSync(o.workDir), ['hashi-1.2.0-windows-store.msixbundle']);
});

test("a re-run takes the draft, replaces its package, and doesn't commit without submit", async () => {
  const state = newState({ pending: 'PendingCommit' });
  const draft = state.submissions.get('200') as Record<string, unknown>;
  draft.applicationPackages = [
    ...(draft.applicationPackages as unknown[]),
    { fileName: 'hashi-1.2.0-windows-store.msixbundle', fileStatus: 'PendingUpload' },
  ];
  const { fetch: f, calls } = fake(state);
  const summary = await release(connection(f), releaseOptions({ submit: false }), log);
  assert.deepEqual(
    calls.map((c) => `${c.method} ${c.url.pathname.split('/').slice(-2).join('/')}`),
    [
      'GET applications/9NBLGGH4R315',
      'GET submissions/100',
      'GET submissions/200',
      'PUT submissions/200',
      'PUT upload/123',
    ],
  );
  // One package of 1.2.0, not two
  assert.deepEqual(
    (
      state.submissions.get('200')?.applicationPackages as Record<string, unknown>[] | undefined
    )?.map((p) => `${p.fileName} ${p.fileStatus}`),
    [
      'hashi-1.1.0-windows-store.msixbundle PendingDelete',
      'hashi-1.2.0-windows-store.msixbundle PendingUpload',
    ],
  );
  assert.match(summary.join('\n'), /Not submitted \(MS_STORE_SUBMIT is false\)/);
});

test('refuses: in certification, a newer version, another identity, no first submission', async () => {
  const review = fake(newState({ pending: 'Certification' }));
  await assert.rejects(
    release(connection(review.fetch), releaseOptions(), log),
    /submission 200 is Certification, not a draft.*Wait for Microsoft/,
  );
  const newer = fake(newState({ publishedVersion: '1.3.0.0' }));
  await assert.rejects(
    release(connection(newer.fetch), releaseOptions(), log),
    /the Store has a newer version: hashi-1\.1\.0-windows-store\.msixbundle is 1\.3\.0\.0/,
  );
  const same = fake(newState({ publishedVersion: '1.2.0.0' }));
  assert.deepEqual(await release(connection(same.fetch), releaseOptions(), log), [
    'Microsoft Store: 1.2.0 is published already, nothing to do',
  ]);
  assert.equal(same.calls.length, 2);
  const other = newState();
  other.app.publisherName = 'CN=Someone';
  await assert.rejects(
    release(connection(fake(other).fetch), releaseOptions(), log),
    /doesn't fit the Store's app: .*CN=Someone.*-f desktop=true/,
  );
  const first = fake(newState({ publishedVersion: null }));
  await assert.rejects(
    release(connection(first.fetch), releaseOptions(), log),
    /no published submission: the first one is made in Partner Center/,
  );
  assert.ok(!first.calls.some((c) => c.method === 'POST'));
});

test("the commit's failure: Microsoft's errors, and what to do", async () => {
  const { fetch: f } = fake(newState({ statuses: ['CommitStarted', 'CommitFailed'] }));
  await assert.rejects(
    release(connection(f), releaseOptions(), log),
    /refused the submission 300: InvalidParameterValue: The package identity is wrong\. \(fix it in Partner Center/,
  );
  const slow = fake(newState({ statuses: Array(40).fill('CommitStarted') }));
  assert.match(
    (await release(connection(slow.fetch), releaseOptions(), log)).join('\n'),
    /still processing it/,
  );
  const denied = fake(newState());
  await assert.rejects(
    release({ ...connection(denied.fetch), token: 'other' }, releaseOptions(), log),
    /getApp: HTTP 401: Unauthorized \(the Entra application has to be added in Partner Center/,
  );
  const blob = fake(newState({ fail: { 'PUT /upload/123': [403, undefined] } }));
  await assert.rejects(release(connection(blob.fetch), releaseOptions(), log), /upload: HTTP 403/);
});

test('packages and notes: the plan', () => {
  assert.equal(
    publishedState({ applicationPackages: [{ version: '1.1.0.0' }, { fileName: 'x' }] }, '1.2.0'),
    'older',
  );
  assert.equal(publishedState({}, '1.2.0'), 'older');
  assert.deepEqual(
    replacePackages([{ fileName: 'a', fileStatus: 'Uploaded' }], 'b').map((p) => p.fileStatus),
    ['PendingDelete', 'PendingUpload'],
  );
  assert.deepEqual(releaseNotes('New: rivals', '1.2.0'), { text: 'New: rivals' });
  assert.deepEqual(releaseNotes('', '1.2.0'), { text: 'Hashi 1.2.0' });
  const play = releaseNotes('Your stamps are achievements in Google Play Games.', '1.2.0');
  assert.equal(play.text, 'Hashi 1.2.0');
  assert.match(play.note ?? '', /name Google Play Games/);
  assert.throws(
    () => releaseNotes('x'.repeat(LIMITS.releaseNotes + 1), '1.2.0'),
    /1501 characters/,
  );
});

test("errors: the API's, Entra's, a submission's", () => {
  assert.equal(errorMessage(409, { code: 'InvalidState', message: 'busy' }), 'InvalidState: busy');
  assert.equal(errorMessage(401, { statusCode: 401, message: 'Unauthorized' }), 'Unauthorized');
  assert.equal(
    errorMessage(400, { error: 'invalid_client', error_description: 'AADSTS: bad' }),
    'AADSTS: bad',
  );
  assert.equal(
    errorMessage(200, {
      errors: [
        { code: 'A', details: 'one' },
        { code: 'B', details: 'two' },
      ],
    }),
    'A: one; B: two',
  );
  assert.equal(errorMessage(500, ''), 'HTTP 500');
});

test('the environment: GUIDs, a Store ID; nothing needed for a dry run', () => {
  const env = {
    MS_STORE_TENANT_ID: '0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9',
    MS_STORE_CLIENT_ID: '1a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9',
    MS_STORE_CLIENT_SECRET: 'secret',
    MS_STORE_APP_ID: APP,
  };
  assert.equal(environment(env, false).appId, APP);
  assert.deepEqual(environment({}, true), {
    tenant: '',
    clientId: '',
    clientSecret: '',
    appId: '',
  });
  assert.throws(() => environment({}, false), /MS_STORE_TENANT_ID, MS_STORE_CLIENT_ID, .* not set/);
  assert.throws(() => environment({ ...env, MS_STORE_APP_ID: 'rs.hashi' }, false), /Store ID/);
  assert.throws(() => environment({ ...env, MS_STORE_TENANT_ID: 'contoso' }, false), /tenant ID/);
});

// --- Listing --------------------------------------------------------------------------------

test("the listing's texts: Google Play's, within the Store's limits in every language", () => {
  const texts = listingTexts();
  assert.equal(texts.length, MICROSOFT.length);
  for (const t of texts) assert.deepEqual(textProblems(t), [], t.code);
  const en = texts.find((t) => t.code === 'en-us');
  assert.match(en?.shortDescription ?? '', /^Hashiwokakero logic puzzle/);
  assert.equal(en?.features.length, 4);
  assert.match(en?.features[0] ?? '', /^A new puzzle every day/);
  assert.equal(planListing(texts).texts.length, MICROSOFT.length);
  const bad = { ...(en as (typeof texts)[0]), code: 'de-de', description: 'On the App Store' };
  assert.match(planListing([bad]).skipped[0] ?? '', /names App Store/);
  assert.throws(() => planListing([{ ...bad, code: 'en-us' }]), /default language's texts/);
});

/** A PNG header of that size: what pngInfo reads. */
function png(file: string, width: number, height: number) {
  const head = Buffer.alloc(33);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(head);
  head.writeUInt32BE(13, 8);
  head.write('IHDR', 12, 'latin1');
  head.writeUInt32BE(width, 16);
  head.writeUInt32BE(height, 20);
  head.writeUInt8(8, 24);
  head.writeUInt8(2, 25);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, head);
}

function images(
  sizes: (lang: string) => [number, number][] = () => [
    [1920, 1080],
    [1920, 1080],
  ],
) {
  const dir = tmp();
  for (const site of new Set([
    'en',
    'de',
    'fr',
    'it',
    'es',
    'pt',
    'nl',
    'pl',
    'tr',
    'ru',
    'uk',
    'ar',
    'hi',
    'id',
    'vi',
    'th',
    'ja',
    'ko',
    'zh',
    'zh-Hant',
  ]))
    for (const [i, [w, h]] of sizes(site).entries())
      png(path.join(dir, site, 'desktop', `${i + 1}-scene.png`), w, h);
  return dir;
}

test('the screenshots: the desktop frame of each language, 1366×768 or larger', () => {
  const found = listingImages(images());
  assert.equal(found.size, MICROSOFT.length);
  assert.match(found.get('zh-tw')?.[0] ?? '', /zh-Hant\/desktop\/1-scene\.png$/);
  // es-ES and es-MX both take the Spanish screenshots
  assert.deepEqual(found.get('es-mx'), found.get('es-es'));
  assert.throws(
    () => listingImages(images((l) => (l === 'de' ? [[1280, 720]] : [[1920, 1080]]))),
    /de\/desktop\/1-scene\.png is 1280×720, the Store takes 1366×768 or larger/,
  );
  assert.throws(
    () => listingImages(images((l) => (l === 'ja' ? [] : [[1920, 1080]]))),
    /ja-jp: 0 screenshots/,
  );
});

test("a language's listing: the texts and screenshots on what the Store has", () => {
  const [en] = listingTexts();
  if (en === undefined) throw new Error('no texts');
  const existing = {
    baseListing: {
      title: 'Hashi',
      keywords: ['bridges'],
      releaseNotes: 'Hashi 1.1',
      images: [
        { fileName: 'old.png', fileStatus: 'Uploaded', imageType: 'Screenshot', id: 'i1' },
        { fileName: 'tile.png', fileStatus: 'Uploaded', imageType: 'Icon', id: 'i2' },
        { fileName: 'en-us/1-x.png', fileStatus: 'PendingUpload', imageType: 'Screenshot' },
      ],
    },
    platformOverrides: {},
  };
  const changed = listingWith(existing, en, 'Hashi Bridges', ['en-us/1-daily.png']);
  const base = changed.baseListing as Record<string, unknown>;
  assert.equal(base.title, 'Hashi');
  assert.deepEqual(base.keywords, ['bridges']);
  assert.equal(base.releaseNotes, 'Hashi 1.1');
  assert.equal(base.description, en.description);
  assert.deepEqual(base.features, en.features);
  assert.deepEqual(
    (base.images as Record<string, unknown>[]).map((i) => `${i.fileName} ${i.fileStatus}`),
    ['old.png PendingDelete', 'tile.png Uploaded', 'en-us/1-daily.png PendingUpload'],
  );
  assert.deepEqual(changed.platformOverrides, {});
  // A new language: the app's name as its title; texts only: the images as they are
  const fresh = listingWith(undefined, en, 'Hashi Bridges', null).baseListing as Record<
    string,
    unknown
  >;
  assert.equal(fresh.title, 'Hashi Bridges');
  assert.equal(fresh.images, undefined);
});

test('the listing flow: every language into a new submission, the screenshots uploaded, not committed', async () => {
  const state = newState();
  const { fetch: f, calls } = fake(state);
  const dir = tmp();
  const summary = await listing(connection(f), listingTexts(), listingImages(images()), dir, log);
  assert.deepEqual(
    calls.map((c) => `${c.method} ${c.url.pathname.split('/').slice(-2).join('/')}`),
    [
      'GET applications/9NBLGGH4R315',
      'POST 9NBLGGH4R315/submissions',
      'PUT submissions/300',
      'PUT upload/123',
    ],
  );
  const s = state.submissions.get('300') as Record<string, unknown>;
  assert.equal(s.status, 'PendingCommit');
  const all = s.listings as Record<string, { baseListing: Record<string, unknown> }>;
  const byName = (a: string, b: string) => a.localeCompare(b);
  assert.deepEqual(Object.keys(all).sort(byName), MICROSOFT.map((m) => m.code).sort(byName));
  assert.equal(all['ja-jp']?.baseListing.title, 'Hashi');
  assert.equal(all['de-de']?.baseListing.releaseNotes, 'Hashi 1.1');
  const zipped = state.uploaded ?? Buffer.alloc(0);
  const names = zipEntries(zipped).map((e) => e.name);
  assert.equal(names.length, MICROSOFT.length * 2);
  assert.ok(names.includes('pt-pt/2-2-scene.png'));
  assert.match(
    summary[0] ?? '',
    /22 languages .*desktop screenshots, in the submission 300 \(not submitted/,
  );
  assert.deepEqual(fs.readdirSync(dir), []);
  // Texts only: no upload
  const texts = fake(newState({ pending: 'PendingCommit' }));
  await listing(connection(texts.fetch), listingTexts(), null, dir, log);
  assert.deepEqual(
    texts.calls.map((c) => c.method),
    ['GET', 'GET', 'PUT'],
  );
});
