// The Windows app in the Microsoft Store (docs/stores.md, Microsoft Store): its MSIX package, and
// its upload through the Microsoft Store submission API
// (https://learn.microsoft.com/windows/uwp/monetize/create-and-manage-submissions-using-windows-store-services):
//
//   node scripts/microsoft-store.ts layout --version <x.y.z> --arch <x64|arm64> --exe <hashi.exe>
//                                          --out <dir>
//   node scripts/microsoft-store.ts release --version <x.y.z> [--whatsnew <file>] [--submit]
//                                           [--dry-run] <msixbundle>
//   node scripts/microsoft-store.ts listing [--images <dir>] [--dry-run]
//
// layout: the folder of one architecture's MSIX package: the program (the release build's
// hashi.exe, the same as in the NSIS and MSI installers), its manifest (AppxManifest.xml: the
// Store's identity, version x.y.z.0, full trust, the app's languages) and the logos from
// app/src-tauri/icons. The release workflow packs it with the Windows SDK's MakeAppx (after
// MakePri, for the logos' sizes) and bundles both architectures into one .msixbundle.
// Unsigned: the Store signs what it publishes.
//
// release: the .msixbundle replaces the packages of the app's next submission (the one a run
// before or Partner Center left as a draft, else a new one, copied from the published one), the
// release notes go into every language's "what's new", and with --submit the submission is
// committed, which sends it to Microsoft's certification; it goes live after it. Before that the
// bundle is checked: the Store's identity, the version, x64 and arm64, every package's manifest.
// It refuses a submission in certification and a newer version in the Store, and does nothing
// when this version is published already.
//
// listing: the texts of Google Play's listing (fastlane/metadata/android: the description, the
// short description, the first point of each of its lists as the features) in the Store's
// languages (MICROSOFT below), with --images each language's desktop screenshots from a folder
// like target/store/images (the "Store listing" workflow's artifact). Into the next submission;
// never committed (the next release commits it).
//
// --dry-run: the local checks (the bundle's manifests, the texts, the images) and the plan, no
// request to Microsoft.
//
// The identity (Partner Center, the app, Product management, Product identity) in the
// environment: MS_STORE_IDENTITY_NAME (Package/Identity/Name), MS_STORE_PUBLISHER
// (Package/Identity/Publisher, CN=<...>), MS_STORE_PUBLISHER_DISPLAY_NAME
// (Package/Properties/PublisherDisplayName, default "Noel Huibers") and MS_STORE_DISPLAY_NAME
// (the reserved name, default "Hashi"). Without the first two `layout` makes a package with a
// placeholder identity, which only the checks take. The API's Microsoft Entra application:
// MS_STORE_TENANT_ID, MS_STORE_CLIENT_ID, MS_STORE_CLIENT_SECRET, and MS_STORE_APP_ID (the Store
// ID, 9<...>). MS_STORE_API and MS_STORE_TOKEN_URL point it at another server (the tests' mock).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';

import { pngInfo } from '../store/assemble.ts';
import { PLAY, PLAY_METADATA } from '../store/locales.ts';
import { featureBullets } from './amazon-appstore.ts';
import { otherStores } from './galaxy-store.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const API = 'https://manage.devcenter.microsoft.com/v1.0/my';
/** The token's audience: the submission API. */
export const RESOURCE = 'https://manage.devcenter.microsoft.com';
export const tokenUrl = (tenant: string) =>
  `https://login.microsoftonline.com/${encodeURIComponent(tenant)}/oauth2/token`;
/** Azure Blob Storage's API version for the upload: one that takes a single upload of 5000 MiB. */
export const BLOB_VERSION = '2023-11-03';
const ICONS = path.join(ROOT, 'app/src-tauri/icons');
const TAURI_CONF = path.join(ROOT, 'app/src-tauri/tauri.conf.json');

// --- The package --------------------------------------------------------------------------

/** The architectures of the Windows builds (docs/stores.md, Desktop builds), as MSIX names them. */
export const ARCHS = ['x64', 'arm64'] as const;
export type Arch = (typeof ARCHS)[number];

/** The program in the package, at its root (`mainBinaryName` in tauri.conf.json). */
export const EXECUTABLE = 'hashi.exe';
/** The application's ID in the package (letters and digits). */
export const APPLICATION_ID = 'Hashi';
/**
 * Windows 10 1809 and newer: the oldest Windows 10 Microsoft still supports (LTSC 2019), which
 * runs WebView2 and full-trust MSIX packages. MaxVersionTested: Windows 11 24H2.
 */
export const MIN_VERSION = '10.0.17763.0';
export const MAX_VERSION_TESTED = '10.0.26100.0';
/**
 * The languages of the app (locales/), one variant each, as the package declares them: the
 * Store lists them as the app's languages. The first is the default.
 */
export const PACKAGE_LANGUAGES = [
  'en-US',
  'de-DE',
  'fr-FR',
  'it-IT',
  'es-ES',
  'pt-BR',
  'nl-NL',
  'pl-PL',
  'tr-TR',
  'ru-RU',
  'uk-UA',
  'ar-SA',
  'hi-IN',
  'id-ID',
  'vi-VN',
  'th-TH',
  'ja-JP',
  'ko-KR',
  'zh-CN',
  'zh-TW',
] as const;
/** Every package's capabilities: the internet (hashi.rs), and a desktop program's full trust. */
export const CAPABILITIES = ['internetClient', 'runFullTrust'] as const;

/**
 * The logos in the package (Assets/): the file and where it comes from (app/src-tauri/icons,
 * `cargo tauri icon`). Square44x44Logo is the app's icon in the taskbar, the Start menu and the
 * title bar: its target sizes (with the unplated variants Windows 11 takes) keep it sharp there;
 * MakePri indexes them into resources.pri. StoreLogo is the icon of the package in the Store
 * app's installer, Square150x150Logo and Square71x71Logo Windows 10's Start tiles.
 */
export const ASSETS: readonly (readonly [file: string, icon: string])[] = [
  ['Square44x44Logo.png', 'Square44x44Logo.png'],
  ...[
    ['32', '32x32.png'],
    ['64', '64x64.png'],
    ['256', '128x128@2x.png'],
  ].flatMap(([size, icon]) => [
    [`Square44x44Logo.targetsize-${size}.png`, icon as string] as const,
    [`Square44x44Logo.targetsize-${size}_altform-unplated.png`, icon as string] as const,
  ]),
  ['Square71x71Logo.png', 'Square71x71Logo.png'],
  ['Square150x150Logo.png', 'Square150x150Logo.png'],
  ['StoreLogo.png', 'StoreLogo.png'],
];

/** The Store's identity of the app (Partner Center, Product identity). */
export interface Identity {
  name: string;
  publisher: string;
  publisherDisplayName: string;
  displayName: string;
}

/** What the package says it is when the identity isn't set: good for the checks, not the Store. */
export const PLACEHOLDER = { name: 'rs.hashi.placeholder', publisher: 'CN=Hashi Placeholder' };

/** The identity from the environment, the placeholder's where it isn't set. */
export function identityOf(env: NodeJS.ProcessEnv): Identity & { placeholder: boolean } {
  const value = (name: string) => env[name]?.trim() ?? '';
  const name = value('MS_STORE_IDENTITY_NAME');
  const publisher = value('MS_STORE_PUBLISHER');
  const placeholder = name === '' || publisher === '';
  const identity = {
    name: placeholder ? PLACEHOLDER.name : name,
    publisher: placeholder ? PLACEHOLDER.publisher : publisher,
    publisherDisplayName: value('MS_STORE_PUBLISHER_DISPLAY_NAME') || 'Noel Huibers',
    displayName: value('MS_STORE_DISPLAY_NAME') || 'Hashi',
    placeholder,
  };
  const problems = identityProblems(identity);
  if (problems.length > 0) throw new Error(problems.join('; '));
  return identity;
}

/** An identity's problems with the manifest's rules (the schema's patterns and lengths). */
export function identityProblems(i: Identity): string[] {
  const problems: string[] = [];
  if (!/^[-.A-Za-z0-9]{3,50}$/.test(i.name) || /\.$/.test(i.name))
    problems.push(
      `MS_STORE_IDENTITY_NAME "${i.name}" isn't a package name (3 to 50 letters, digits, dots and dashes): Package/Identity/Name of Partner Center's Product identity`,
    );
  if (!/^CN=\S/.test(i.publisher) || i.publisher.length > 8192)
    problems.push(
      `MS_STORE_PUBLISHER "${i.publisher}" isn't a publisher (CN=<...>): Package/Identity/Publisher of Partner Center's Product identity`,
    );
  if (i.publisherDisplayName === '' || i.publisherDisplayName.length > 256)
    problems.push('MS_STORE_PUBLISHER_DISPLAY_NAME: 1 to 256 characters');
  if (i.displayName === '' || i.displayName.length > 256)
    problems.push('MS_STORE_DISPLAY_NAME: 1 to 256 characters');
  return problems;
}

/** x.y.z as the package's four-part version, x.y.z.0: the fourth part is the Store's. */
export function msixVersion(version: string): string {
  const parts = /^(\d+)\.(\d+)\.(\d+)$/.exec(version)?.slice(1).map(Number);
  if (parts === undefined || parts.some((p) => p > 65535) || parts[0] === 0)
    throw new Error(
      `the version ${version} isn't x.y.z with x from 1 and each at most 65535 (MSIX's version)`,
    );
  return `${parts.join('.')}.0`;
}

const xml = (text: string) =>
  text.replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[c] ?? c,
  );

/** The package's manifest, AppxManifest.xml. */
export function manifest(
  o: Identity & { version: string; arch: Arch; description: string },
): string {
  const v = msixVersion(o.version);
  return `<?xml version="1.0" encoding="utf-8"?>
<!-- Made by scripts/microsoft-store.ts (docs/stores.md, Microsoft Store) -->
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">
  <Identity Name="${xml(o.name)}" Publisher="${xml(o.publisher)}" Version="${v}" ProcessorArchitecture="${o.arch}" />
  <Properties>
    <DisplayName>${xml(o.displayName)}</DisplayName>
    <PublisherDisplayName>${xml(o.publisherDisplayName)}</PublisherDisplayName>
    <Logo>Assets\\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="${MIN_VERSION}" MaxVersionTested="${MAX_VERSION_TESTED}" />
  </Dependencies>
  <Resources>
${PACKAGE_LANGUAGES.map((l) => `    <Resource Language="${l}" />`).join('\n')}
  </Resources>
  <Applications>
    <Application Id="${APPLICATION_ID}" Executable="${EXECUTABLE}" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements
        DisplayName="${xml(o.displayName)}"
        Description="${xml(o.description)}"
        BackgroundColor="transparent"
        Square150x150Logo="Assets\\Square150x150Logo.png"
        Square44x44Logo="Assets\\Square44x44Logo.png">
        <uap:DefaultTile Square71x71Logo="Assets\\Square71x71Logo.png" ShortName="${xml(o.displayName)}" />
      </uap:VisualElements>
    </Application>
  </Applications>
  <Capabilities>
    <Capability Name="internetClient" />
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
`;
}

/** The app's short description (tauri.conf.json, the Play listing's): the manifest's. */
export function shortDescription(): string {
  const conf = JSON.parse(fs.readFileSync(TAURI_CONF, 'utf8')) as {
    bundle?: { shortDescription?: string };
  };
  const text = conf.bundle?.shortDescription ?? '';
  if (text === '') throw new Error('tauri.conf.json has no bundle.shortDescription');
  return text;
}

/** Writes one architecture's package folder: the program, the manifest and the logos. */
export function layout(
  o: Identity & { version: string; arch: Arch; exe: string; out: string },
): string[] {
  if (!fs.statSync(o.exe).isFile()) throw new Error(`${o.exe} is no file`);
  if (fs.existsSync(o.out) && fs.readdirSync(o.out).length > 0)
    throw new Error(`${o.out} isn't empty`);
  fs.mkdirSync(path.join(o.out, 'Assets'), { recursive: true });
  fs.copyFileSync(o.exe, path.join(o.out, EXECUTABLE));
  fs.writeFileSync(
    path.join(o.out, 'AppxManifest.xml'),
    manifest({ ...o, description: shortDescription() }),
  );
  for (const [file, icon] of ASSETS)
    fs.copyFileSync(path.join(ICONS, icon), path.join(o.out, 'Assets', file));
  return [EXECUTABLE, 'AppxManifest.xml', ...ASSETS.map(([file]) => `Assets/${file}`)];
}

// --- Zip archives: the bundle to check, the submission's upload ----------------------------

export interface ZipEntry {
  name: string;
  method: number;
  compressedSize: number;
  size: number;
  offset: number;
}

/**
 * The entries of a zip archive (an .msix, an .msixbundle: MakeAppx writes ZIP64), from its
 * central directory.
 */
export function zipEntries(zip: Buffer): ZipEntry[] {
  const max = 0xffffffff;
  let eocd = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 22 - 65535); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd === -1) throw new Error('not a zip archive (no end of central directory)');
  let count = zip.readUInt16LE(eocd + 10);
  let offset = zip.readUInt32LE(eocd + 16);
  if (offset === max || count === 0xffff) {
    // ZIP64: the locator before the end record points to the ZIP64 end record
    const locator = eocd - 20;
    if (locator < 0 || zip.readUInt32LE(locator) !== 0x07064b50)
      throw new Error('a ZIP64 archive without its locator');
    const record = Number(zip.readBigUInt64LE(locator + 8));
    if (zip.readUInt32LE(record) !== 0x06064b50) throw new Error('a broken ZIP64 end record');
    count = Number(zip.readBigUInt64LE(record + 32));
    offset = Number(zip.readBigUInt64LE(record + 48));
  }
  const entries: ZipEntry[] = [];
  let p = offset;
  for (let n = 0; n < count; n++) {
    if (zip.readUInt32LE(p) !== 0x02014b50) throw new Error('a broken central directory');
    const method = zip.readUInt16LE(p + 10);
    let compressedSize = zip.readUInt32LE(p + 20);
    let size = zip.readUInt32LE(p + 24);
    const nameLength = zip.readUInt16LE(p + 28);
    const extraLength = zip.readUInt16LE(p + 30);
    const commentLength = zip.readUInt16LE(p + 32);
    let local = zip.readUInt32LE(p + 42);
    const name = zip.toString('utf8', p + 46, p + 46 + nameLength);
    // The ZIP64 extra field has the values that didn't fit, in this order
    let e = p + 46 + nameLength;
    const end = e + extraLength;
    while (e + 4 <= end) {
      const id = zip.readUInt16LE(e);
      const length = zip.readUInt16LE(e + 2);
      if (id === 0x0001) {
        let q = e + 4;
        if (size === max) {
          size = Number(zip.readBigUInt64LE(q));
          q += 8;
        }
        if (compressedSize === max) {
          compressedSize = Number(zip.readBigUInt64LE(q));
          q += 8;
        }
        if (local === max) local = Number(zip.readBigUInt64LE(q));
      }
      e += 4 + length;
    }
    entries.push({ name, method, compressedSize, size, offset: local });
    p = end + commentLength;
  }
  return entries;
}

/** An entry's content: stored or deflated. */
export function zipRead(zip: Buffer, entry: ZipEntry): Buffer {
  const p = entry.offset;
  if (zip.readUInt32LE(p) !== 0x04034b50) throw new Error(`${entry.name}: no local header`);
  const start = p + 30 + zip.readUInt16LE(p + 26) + zip.readUInt16LE(p + 28);
  const data = zip.subarray(start, start + entry.compressedSize);
  if (entry.method === 0) return data;
  if (entry.method === 8) return zlib.inflateRawSync(data);
  throw new Error(`${entry.name}: compression method ${entry.method}`);
}

/** The content of the entry `name`, null without one. */
export function zipFile(zip: Buffer, name: string): Buffer | null {
  const entry = zipEntries(zip).find((e) => e.name === name);
  return entry === undefined ? null : zipRead(zip, entry);
}

/**
 * Writes a zip archive of `files` (its name in the archive, the file), stored: the bundle and
 * the PNGs are compressed already. Not ZIP64: the upload stays far below 4 GB.
 */
export function writeZip(out: string, files: readonly (readonly [name: string, file: string])[]) {
  const fd = fs.openSync(out, 'w');
  const central: Buffer[] = [];
  let offset = 0;
  try {
    for (const [name, file] of files) {
      const data = fs.readFileSync(file);
      const crc = zlib.crc32(data);
      const nameBytes = Buffer.from(name, 'utf8');
      if (offset + data.length > 0xfffffff0) throw new Error('the upload is too big for a zip');
      const header = Buffer.alloc(30);
      header.writeUInt32LE(0x04034b50, 0);
      header.writeUInt16LE(20, 4);
      // UTF-8 names
      header.writeUInt16LE(0x0800, 6);
      header.writeUInt16LE(0, 8);
      header.writeUInt32LE(0, 10);
      header.writeUInt32LE(crc, 14);
      header.writeUInt32LE(data.length, 18);
      header.writeUInt32LE(data.length, 22);
      header.writeUInt16LE(nameBytes.length, 26);
      header.writeUInt16LE(0, 28);
      const entry = Buffer.alloc(46);
      entry.writeUInt32LE(0x02014b50, 0);
      entry.writeUInt16LE(20, 4);
      entry.writeUInt16LE(20, 6);
      entry.writeUInt16LE(0x0800, 8);
      entry.writeUInt32LE(crc, 16);
      entry.writeUInt32LE(data.length, 20);
      entry.writeUInt32LE(data.length, 24);
      entry.writeUInt16LE(nameBytes.length, 28);
      entry.writeUInt32LE(offset, 42);
      central.push(entry, nameBytes);
      fs.writeSync(fd, header);
      fs.writeSync(fd, nameBytes);
      fs.writeSync(fd, data);
      offset += header.length + nameBytes.length + data.length;
    }
    const directory = Buffer.concat(central);
    const end = Buffer.alloc(22);
    end.writeUInt32LE(0x06054b50, 0);
    end.writeUInt16LE(files.length, 8);
    end.writeUInt16LE(files.length, 10);
    end.writeUInt32LE(directory.length, 12);
    end.writeUInt32LE(offset, 16);
    fs.writeSync(fd, directory);
    fs.writeSync(fd, end);
  } finally {
    fs.closeSync(fd);
  }
}

// --- The bundle's manifests ----------------------------------------------------------------

const unxml = (text: string) =>
  text.replace(
    /&(amp|lt|gt|quot|apos);/g,
    (_, e: string) => ({ amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" })[e] ?? '',
  );

/** The attributes of the first element `tag` (with its prefix, if any) in `text`. */
export function attributes(text: string, tag: string): Record<string, string> | null {
  const element = new RegExp(`<${tag}(\\s[^>]*?)?/?>`).exec(text);
  if (element === null) return null;
  const found: Record<string, string> = {};
  for (const [, name, value] of (element[1] ?? '').matchAll(/([\w:]+)\s*=\s*"([^"]*)"/g))
    if (name !== undefined && value !== undefined) found[name] = unxml(value);
  return found;
}

/** The text of the first element `tag`. */
const elementText = (text: string, tag: string) =>
  unxml(new RegExp(`<${tag}>([^<]*)</${tag}>`).exec(text)?.[1]?.trim() ?? '');

export interface PackageInfo {
  file: string;
  arch: string;
  version: string;
  name: string;
  publisher: string;
  publisherDisplayName: string;
  displayName: string;
  family: string | null;
  executable: string | null;
  entryPoint: string | null;
  capabilities: string[];
  languages: string[];
  files: string[];
}

export interface BundleInfo {
  name: string;
  publisher: string;
  version: string;
  packages: PackageInfo[];
}

/** One package's manifest and files. */
export function packageInfo(file: string, msix: Buffer): PackageInfo {
  const text = zipFile(msix, 'AppxManifest.xml')?.toString('utf8');
  if (text === undefined) throw new Error(`${file} has no AppxManifest.xml`);
  const identity = attributes(text, 'Identity') ?? {};
  const application = attributes(text, 'Application') ?? {};
  return {
    file,
    arch: identity.ProcessorArchitecture ?? '',
    version: identity.Version ?? '',
    name: identity.Name ?? '',
    publisher: identity.Publisher ?? '',
    publisherDisplayName: elementText(text, 'PublisherDisplayName'),
    displayName: elementText(text, 'DisplayName'),
    family: attributes(text, 'TargetDeviceFamily')?.Name ?? null,
    executable: application.Executable ?? null,
    entryPoint: application.EntryPoint ?? null,
    capabilities: [...text.matchAll(/<(?:\w+:)?Capability\s+Name="([^"]+)"/g)].map(
      (m) => m[1] ?? '',
    ),
    languages: [...text.matchAll(/<Resource\s+Language="([^"]+)"/g)].map((m) => m[1] ?? ''),
    files: zipEntries(msix).map((e) => e.name),
  };
}

/** The bundle's manifest and its packages'. */
export function bundleInfo(bundle: Buffer): BundleInfo {
  const text = zipFile(bundle, 'AppxMetadata/AppxBundleManifest.xml')?.toString('utf8');
  if (text === undefined) throw new Error('no AppxMetadata/AppxBundleManifest.xml: not a bundle');
  const identity = attributes(text, 'Identity') ?? {};
  const packages: PackageInfo[] = [];
  for (const [element] of text.matchAll(/<Package\s[^>]*>/g)) {
    const a = attributes(element, 'Package') ?? {};
    if (a.Type !== 'application' || a.FileName === undefined) continue;
    const inner = zipFile(bundle, a.FileName);
    if (inner === null) throw new Error(`the bundle names ${a.FileName}, but hasn't got it`);
    packages.push(packageInfo(a.FileName, inner));
  }
  return {
    name: identity.Name ?? '',
    publisher: identity.Publisher ?? '',
    version: identity.Version ?? '',
    packages,
  };
}

/**
 * The bundle's problems for the Store: the identity (the expected one: the app's in Partner
 * Center, or the environment's), the version, both architectures, each package's manifest.
 */
export function bundleProblems(
  b: BundleInfo,
  expected: { name: string; publisher: string; version: string; publisherDisplayName?: string },
): string[] {
  const problems: string[] = [];
  const v = msixVersion(expected.version);
  if (b.name !== expected.name || b.publisher !== expected.publisher)
    problems.push(
      `the bundle is "${b.name}" by "${b.publisher}", the Store's app is "${expected.name}" by "${expected.publisher}"`,
    );
  if (b.version !== v) problems.push(`the bundle's version is ${b.version}, not ${v}`);
  const archs = b.packages.map((p) => p.arch);
  if (
    archs.length !== ARCHS.length ||
    ARCHS.some((a) => archs.filter((arch) => arch === a).length !== 1)
  )
    problems.push(
      `the bundle has packages for ${archs.join(', ') || 'nothing'}, not x64 and arm64`,
    );
  for (const p of b.packages) {
    const at = `${p.file}:`;
    if (p.name !== b.name || p.publisher !== b.publisher || p.version !== b.version)
      problems.push(`${at} "${p.name}" by "${p.publisher}" ${p.version}, unlike the bundle`);
    if (
      expected.publisherDisplayName !== undefined &&
      p.publisherDisplayName !== expected.publisherDisplayName
    )
      problems.push(
        `${at} the publisher's display name is "${p.publisherDisplayName}", not "${expected.publisherDisplayName}"`,
      );
    if (p.family !== 'Windows.Desktop')
      problems.push(`${at} the device family is ${p.family ?? 'missing'}, not Windows.Desktop`);
    if (p.executable !== EXECUTABLE || p.entryPoint !== 'Windows.FullTrustApplication')
      problems.push(
        `${at} the application is ${p.executable ?? 'missing'} (${p.entryPoint ?? 'no entry point'}), not ${EXECUTABLE}, full trust`,
      );
    if (!p.files.includes(EXECUTABLE)) problems.push(`${at} has no ${EXECUTABLE}`);
    for (const c of CAPABILITIES)
      if (!p.capabilities.includes(c)) problems.push(`${at} lacks the capability ${c}`);
    if (p.languages[0] !== 'en-US')
      problems.push(`${at} the default language is ${p.languages[0] ?? 'missing'}, not en-US`);
    if (!p.files.includes('resources.pri'))
      problems.push(`${at} has no resources.pri (MakePri: the logos' sizes)`);
  }
  return problems;
}

// --- Requests and responses ----------------------------------------------------------------

export class StoreError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): string | null =>
  value === null || value === undefined || value === '' ? null : String(value);
const chars = (t: string) => [...t].length;

/**
 * The message of an error response: the API's `{code, message}` (`{statusCode, message}`),
 * Entra's `{error, error_description}`, a submission's `{errors: [{code, details}]}`; else the
 * body itself.
 */
export function errorMessage(status: number, body: unknown): string {
  const messages = (value: unknown): string[] => {
    if (typeof value === 'string') return value === '' ? [] : [value];
    if (Array.isArray(value)) return value.flatMap(messages);
    if (!isObject(value)) return [];
    const code = text(value.code);
    const own = [value.message, value.error_description, value.details]
      .map(text)
      .filter((m): m is string => m !== null)
      .map((m) => (code === null ? m : `${code}: ${m}`));
    if (own.length === 0 && typeof value.error === 'string') own.push(value.error);
    if (own.length === 0 && code !== null) own.push(code);
    return [...own, ...messages(value.errors), ...messages(value.data)];
  };
  const found = messages(body);
  if (found.length > 0) return [...new Set(found)].join('; ').slice(0, 2000);
  if (isObject(body) || Array.isArray(body)) return JSON.stringify(body).slice(0, 1000);
  return `HTTP ${status}`;
}

/** What to do about an error, for the ones that need the owner. */
function hint(status: number, what: string): string {
  if (what === 'accessToken')
    return ' (MS_STORE_TENANT_ID, MS_STORE_CLIENT_ID, MS_STORE_CLIENT_SECRET: the Microsoft Entra application of Partner Center, and its client secret, which expires)';
  if (status === 401 || status === 403)
    return ' (the Entra application has to be added in Partner Center, Account settings, User management, Microsoft Entra applications, with the Manager role; MS_STORE_APP_ID an app of the same account)';
  if (status === 404) return ' (MS_STORE_APP_ID: no such app in the account)';
  if (status === 409)
    return " (the app has a submission in progress, or its first submission wasn't made in Partner Center yet)";
  return '';
}

export interface Connection {
  fetch: typeof fetch;
  api: string;
  appId: string;
  token: string;
  /** Waits between the commit's status checks (the tests don't). */
  sleep: (ms: number) => Promise<void>;
}

/** A call of the API (a path below the app) with the access token. */
export async function call(
  c: Connection,
  what: string,
  method: 'GET' | 'POST' | 'PUT' | 'DELETE',
  sub: string,
  json?: unknown,
): Promise<unknown> {
  const headers: Record<string, string> = {
    Authorization: `Bearer ${c.token}`,
    Accept: 'application/json',
  };
  if (json !== undefined) headers['Content-Type'] = 'application/json';
  const url = `${c.api}/applications/${encodeURIComponent(c.appId)}${sub}`;
  const response = await c.fetch(url, {
    method,
    headers,
    ...(json === undefined ? {} : { body: JSON.stringify(json) }),
  });
  const body = await parse(response);
  if (!response.ok)
    throw new StoreError(
      `${what}: HTTP ${response.status}: ${errorMessage(response.status, body)}${hint(response.status, what)}`,
      response.status,
    );
  return body;
}

async function parse(response: Response): Promise<unknown> {
  const raw = await response.text();
  if (raw === '') return null;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/** An access token for the Entra application (client credentials, valid for an hour). */
export async function accessToken(
  f: typeof fetch,
  url: string,
  clientId: string,
  clientSecret: string,
): Promise<string> {
  const response = await f(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/json' },
    body: new URLSearchParams({
      grant_type: 'client_credentials',
      client_id: clientId,
      client_secret: clientSecret,
      resource: RESOURCE,
    }).toString(),
  });
  const body = await parse(response);
  if (!response.ok)
    throw new StoreError(
      `accessToken: HTTP ${response.status}: ${errorMessage(response.status, body)}${hint(response.status, 'accessToken')}`,
      response.status,
    );
  const token = isObject(body) ? body.access_token : null;
  if (typeof token !== 'string' || token === '')
    throw new StoreError('accessToken: no access_token in the answer', response.status);
  return token;
}

/** Uploads the zip to the submission's file upload URL (an Azure Storage blob's SAS URL). */
export async function uploadZip(f: typeof fetch, url: string, zip: string) {
  const response = await f(url, {
    method: 'PUT',
    headers: {
      'x-ms-blob-type': 'BlockBlob',
      'x-ms-version': BLOB_VERSION,
      'Content-Type': 'application/zip',
    },
    body: await fs.openAsBlob(zip, { type: 'application/zip' }),
  });
  if (!response.ok) {
    const body = await response.text();
    const code = /<Code>([^<]*)<\/Code>/.exec(body)?.[1];
    throw new StoreError(
      `upload: HTTP ${response.status}${code === undefined ? '' : `: ${code}`} (the submission's upload URL expires; run this again)`,
      response.status,
    );
  }
}

// --- The submission ------------------------------------------------------------------------

export type Log = (line: string) => void;

export interface App {
  id: string;
  primaryName: string;
  identityName: string | null;
  publisher: string | null;
  pending: string | null;
  published: string | null;
}

export function appOf(body: unknown): App {
  const b = isObject(body) ? body : {};
  const ref = (value: unknown) => (isObject(value) ? text(value.id) : null);
  return {
    id: text(b.id) ?? '',
    primaryName: text(b.primaryName) ?? '',
    identityName: text(b.packageIdentityName),
    publisher: text(b.publisherName),
    pending: ref(b.pendingApplicationSubmission),
    published: ref(b.lastPublishedApplicationSubmission),
  };
}

export type Submission = Record<string, unknown> & { id: string; status: string };

function submissionOf(body: unknown, what: string): Submission {
  if (!isObject(body) || text(body.id) === null)
    throw new StoreError(`${what}: no submission in the answer`, 200);
  return { ...body, id: String(body.id), status: text(body.status) ?? '' } as Submission;
}

/** A submission still being edited: its packages, texts and images can change. */
export const DRAFT = 'PendingCommit';

/**
 * The submission to change: the app's pending one if it is a draft, else a new one (a copy of
 * the published one). Refuses one in certification or being published.
 */
export async function openSubmission(c: Connection, app: App, log: Log): Promise<Submission> {
  if (app.pending !== null) {
    const pending = submissionOf(
      await call(c, 'getSubmission', 'GET', `/submissions/${app.pending}`),
      'getSubmission',
    );
    if (pending.status !== DRAFT)
      throw new Error(
        `the app's submission ${pending.id} is ${pending.status}, not a draft: it is in certification or being published. Wait for Microsoft (Partner Center shows its progress), then run this again`,
      );
    log(`The draft submission ${pending.id}`);
    return pending;
  }
  if (app.published === null)
    throw new Error(
      'the app has no published submission: the first one is made in Partner Center (docs/stores.md, Microsoft Store), the API only makes the ones after it',
    );
  const created = submissionOf(
    await call(c, 'createSubmission', 'POST', '/submissions'),
    'createSubmission',
  );
  log(`A new submission ${created.id}`);
  return created;
}

/** The submission without what the API sets itself, for its PUT (as the reference's example). */
function writable(s: Submission): Record<string, unknown> {
  const { id: _, status: __, statusDetails: ___, fileUploadUrl: ____, ...rest } = s;
  return rest;
}

/** The parts of a version, compared as numbers. */
export function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

type Package = Record<string, unknown>;
const packagesOf = (s: Record<string, unknown>): Package[] =>
  Array.isArray(s.applicationPackages) ? s.applicationPackages.filter(isObject) : [];

/**
 * The published version, from the published submission's packages; refuses a newer one than
 * `version`, and says when it is this one.
 */
export function publishedState(
  published: Record<string, unknown>,
  version: string,
): 'older' | 'same' {
  const v = msixVersion(version);
  let same = false;
  for (const p of packagesOf(published)) {
    const pv = text(p.version);
    if (pv === null) continue;
    const d = compareVersions(pv, v);
    if (d > 0)
      throw new Error(`the Store has a newer version: ${text(p.fileName) ?? 'a package'} is ${pv}`);
    if (d === 0) same = true;
  }
  return same ? 'same' : 'older';
}

/** The submission's packages with the bundle as its only one: the others are deleted. */
export function replacePackages(packages: Package[], fileName: string): Package[] {
  return [
    ...packages
      .filter((p) => !(p.fileName === fileName && p.fileStatus === 'PendingUpload'))
      .map((p) => ({ ...p, fileStatus: 'PendingDelete' })),
    {
      fileName,
      fileStatus: 'PendingUpload',
      minimumDirectXVersion: 'None',
      minimumSystemRam: 'None',
    },
  ];
}

/** The Store's limits of a listing's texts. */
export const LIMITS = {
  title: 256,
  description: 10000,
  shortDescription: 1000,
  releaseNotes: 1500,
  features: { count: 20, length: 200 },
} as const;

/** The text of every language's "what's new": the release notes, if the Store may show them. */
export function releaseNotes(notes: string, version: string): { text: string; note?: string } {
  const stores = otherStores(notes);
  if (stores.length > 0)
    return {
      text: `Hashi ${version}`,
      note: `The release notes name ${stores.join(', ')}: the Microsoft Store's "what's new" is "Hashi ${version}"`,
    };
  if (chars(notes) > LIMITS.releaseNotes)
    throw new Error(
      `the release notes have ${chars(notes)} characters, the Store takes ${LIMITS.releaseNotes}`,
    );
  return { text: notes === '' ? `Hashi ${version}` : notes };
}

type Listings = Record<string, Record<string, unknown>>;
const listingsOf = (s: Record<string, unknown>): Listings =>
  isObject(s.listings)
    ? Object.fromEntries(
        Object.entries(s.listings).filter((e): e is [string, Record<string, unknown>] =>
          isObject(e[1]),
        ),
      )
    : {};
const baseOf = (l: Record<string, unknown>) => (isObject(l.baseListing) ? l.baseListing : {});

/** Commits the submission and waits until Microsoft took it (or refused it). Its status. */
async function commit(c: Connection, id: string, log: Log): Promise<string> {
  await call(c, 'commitSubmission', 'POST', `/submissions/${id}/commit`);
  log('Committed, waiting for Microsoft to take it');
  // Pre-processing the packages takes minutes
  for (let i = 0; i < 40; i++) {
    const status = await call(c, 'getSubmissionStatus', 'GET', `/submissions/${id}/status`);
    const s = isObject(status) ? (text(status.status) ?? '') : '';
    if (s === 'CommitFailed') {
      const details = isObject(status) ? status.statusDetails : null;
      throw new Error(
        `Microsoft refused the submission ${id}: ${errorMessage(200, details)} (fix it in Partner Center, or delete the submission there and run this again)`,
      );
    }
    if (s !== 'CommitStarted') return s;
    await c.sleep(30_000);
  }
  return 'CommitStarted';
}

export interface ReleaseOptions {
  version: string;
  bundle: string;
  info: BundleInfo;
  releaseNotes: string;
  submit: boolean;
  /** Where the upload's zip is written. */
  workDir: string;
}

/** The release: the bundle as the submission's package, committed with `submit`. Its summary. */
export async function release(c: Connection, o: ReleaseOptions, log: Log): Promise<string[]> {
  const app = appOf(await call(c, 'getApp', 'GET', ''));
  log(`The app ${app.id}, "${app.primaryName}"`);
  if (app.identityName !== null && app.publisher !== null) {
    const problems = bundleProblems(o.info, {
      name: app.identityName,
      publisher: app.publisher,
      version: o.version,
    });
    if (problems.length > 0)
      throw new Error(
        `the bundle doesn't fit the Store's app: ${problems.join('; ')} (MS_STORE_IDENTITY_NAME and MS_STORE_PUBLISHER as Partner Center's Product identity has them, then build it again: gh workflow run app-release.yml --ref app-v${o.version} -f desktop=true)`,
      );
  }
  if (app.published !== null) {
    const published = await call(c, 'getSubmission', 'GET', `/submissions/${app.published}`);
    if (publishedState(isObject(published) ? published : {}, o.version) === 'same')
      return [`Microsoft Store: ${o.version} is published already, nothing to do`];
  }
  const submission = await openSubmission(c, app, log);
  const fileName = path.basename(o.bundle);
  const listings = listingsOf(submission);
  for (const listing of Object.values(listings))
    listing.baseListing = { ...baseOf(listing), releaseNotes: o.releaseNotes };
  const changed: Submission = {
    ...submission,
    applicationPackages: replacePackages(packagesOf(submission), fileName),
    listings,
  };
  const updated = submissionOf(
    await call(c, 'updateSubmission', 'PUT', `/submissions/${submission.id}`, writable(changed)),
    'updateSubmission',
  );
  const uploadUrl = text(updated.fileUploadUrl) ?? text(submission.fileUploadUrl);
  if (uploadUrl === null) throw new StoreError('the submission has no fileUploadUrl', 200);
  const zip = path.join(o.workDir, 'msstore-upload.zip');
  writeZip(zip, [[fileName, o.bundle]]);
  try {
    await uploadZip(c.fetch, uploadUrl, zip);
  } finally {
    fs.rmSync(zip, { force: true });
  }
  log(`Uploaded ${fileName} (${(fs.statSync(o.bundle).size / 1048576).toFixed(1)} MB)`);
  const summary = [
    `Microsoft Store: ${o.version} in the submission ${submission.id}, the package ${fileName} (${o.info.packages.map((p) => p.arch).join(', ')}), "what's new" in ${Object.keys(listings).length} language${Object.keys(listings).length === 1 ? '' : 's'}`,
  ];
  if (o.submit) {
    const status = await commit(c, submission.id, log);
    summary.push(
      status === 'CommitStarted'
        ? 'Committed; Microsoft is still processing it (Partner Center shows the progress). Certification follows, then it goes live.'
        : `Submitted for certification (${status}). It goes live after it, usually within three working days.`,
    );
  } else {
    summary.push(
      'Not submitted (MS_STORE_SUBMIT is false): submit it in Partner Center (the app, the submission, "Submit to the Store").',
    );
  }
  return summary;
}

// --- Listing -------------------------------------------------------------------------------

/**
 * The listing's languages: the Store's code (lower case, as the API's listings have them) and
 * the Google Play locale whose texts and screenshots it gets. Each language of the app, and
 * Play's European Portuguese and Latin American Spanish.
 */
export const MICROSOFT: readonly { code: string; play: string }[] = [
  { code: 'en-us', play: 'en-US' },
  { code: 'de-de', play: 'de-DE' },
  { code: 'fr-fr', play: 'fr-FR' },
  { code: 'it-it', play: 'it-IT' },
  { code: 'es-es', play: 'es-ES' },
  { code: 'es-mx', play: 'es-419' },
  { code: 'pt-br', play: 'pt-BR' },
  { code: 'pt-pt', play: 'pt-PT' },
  { code: 'nl-nl', play: 'nl-NL' },
  { code: 'pl-pl', play: 'pl-PL' },
  { code: 'tr-tr', play: 'tr-TR' },
  { code: 'ru-ru', play: 'ru-RU' },
  { code: 'uk-ua', play: 'uk' },
  { code: 'ar-sa', play: 'ar' },
  { code: 'hi-in', play: 'hi-IN' },
  { code: 'id-id', play: 'id' },
  { code: 'vi-vn', play: 'vi' },
  { code: 'th-th', play: 'th' },
  { code: 'ja-jp', play: 'ja-JP' },
  { code: 'ko-kr', play: 'ko-KR' },
  { code: 'zh-cn', play: 'zh-CN' },
  { code: 'zh-tw', play: 'zh-TW' },
];
export const DEFAULT_LANGUAGE = 'en-us';

/** The frame of the store images whose screenshots go to the Store: the desktop's, 1920×1080. */
export const SCREENSHOT_FRAME = 'desktop';
/** Desktop screenshots: 1 to 10 PNGs, at least 1366×768, at most 50 MB. */
export const SCREENSHOTS = { min: 1, max: 10, width: 1366, height: 768, bytes: 50 * 1048576 };

export interface ListingText {
  code: string;
  play: string;
  description: string;
  shortDescription: string;
  features: string[];
}

/** The texts of each language, from Google Play's listing. */
export function listingTexts(metadata = path.join(ROOT, PLAY_METADATA)): ListingText[] {
  const read = (locale: string, file: string) =>
    fs.readFileSync(path.join(metadata, locale, file), 'utf8').trim();
  return MICROSOFT.map(({ code, play }) => {
    const description = read(play, 'full_description.txt');
    return {
      code,
      play,
      description,
      shortDescription: read(play, 'short_description.txt'),
      features: featureBullets(description),
    };
  });
}

/** A text's problems with the Store's limits (and other stores' names). */
export function textProblems(t: ListingText): string[] {
  const problems: string[] = [];
  const at = `${t.code} (${t.play})`;
  const stores = otherStores([t.description, t.shortDescription, ...t.features].join('\n'));
  if (stores.length > 0) problems.push(`${at}: names ${stores.join(', ')}`);
  if (t.description === '') problems.push(`${at}: no description`);
  if (chars(t.description) > LIMITS.description)
    problems.push(
      `${at}: the description has ${chars(t.description)} characters, the Store takes ${LIMITS.description}`,
    );
  if (chars(t.shortDescription) > LIMITS.shortDescription)
    problems.push(
      `${at}: the short description has ${chars(t.shortDescription)} characters, the Store takes ${LIMITS.shortDescription}`,
    );
  if (t.features.length > LIMITS.features.count)
    problems.push(`${at}: ${t.features.length} features, the Store takes ${LIMITS.features.count}`);
  for (const f of t.features)
    if (chars(f) > LIMITS.features.length)
      problems.push(
        `${at}: a feature has ${chars(f)} characters, the Store takes ${LIMITS.features.length}`,
      );
  return problems;
}

export interface ListingPlan {
  texts: ListingText[];
  /** Languages left as the Store has them, and why. */
  skipped: string[];
}

/** Which languages get their texts: those within the Store's limits; the default one has to. */
export function planListing(texts: ListingText[]): ListingPlan {
  const plan: ListingPlan = { texts: [], skipped: [] };
  for (const t of texts) {
    const problems = textProblems(t);
    if (t.code === DEFAULT_LANGUAGE && problems.length > 0)
      throw new Error(`the default language's texts: ${problems.join('; ')}`);
    if (problems.length > 0) plan.skipped.push(`${problems.join('; ')}: kept as it is`);
    else plan.texts.push(t);
  }
  return plan;
}

/** An image's problems with the Store's rules for desktop screenshots. */
export function screenshotProblems(file: string): string[] {
  const { width, height } = pngInfo(file);
  const problems: string[] = [];
  const at = path.relative(ROOT, file) || file;
  if (Math.max(width, height) < SCREENSHOTS.width || Math.min(width, height) < SCREENSHOTS.height)
    problems.push(
      `${at} is ${width}×${height}, the Store takes ${SCREENSHOTS.width}×${SCREENSHOTS.height} or larger`,
    );
  if (fs.statSync(file).size > SCREENSHOTS.bytes) problems.push(`${at} is over 50 MB`);
  return problems;
}

/** Each language's screenshots from an images folder, checked. */
export function listingImages(images: string): Map<string, string[]> {
  const problems: string[] = [];
  const result = new Map<string, string[]>();
  for (const { code, play } of MICROSOFT) {
    const site = PLAY.find((p) => p.locale === play)?.site ?? play;
    const dir = path.join(images, site, SCREENSHOT_FRAME);
    const files = fs.existsSync(dir)
      ? fs
          .readdirSync(dir)
          .filter((f) => f.endsWith('.png'))
          .sort((a, b) => Number.parseInt(a, 10) - Number.parseInt(b, 10))
          .map((f) => path.join(dir, f))
      : [];
    if (files.length < SCREENSHOTS.min || files.length > SCREENSHOTS.max)
      problems.push(
        `${code}: ${files.length} screenshots in ${path.relative(ROOT, dir) || dir}, the Store takes ${SCREENSHOTS.min} to ${SCREENSHOTS.max}`,
      );
    for (const f of files) problems.push(...screenshotProblems(f));
    result.set(code, files);
  }
  if (problems.length > 0) throw new Error(`the images have problems:\n${problems.join('\n')}`);
  return result;
}

/**
 * A language's listing with these texts (and screenshots: `shots`, their names in the upload),
 * on what the Store has: its title (a new language gets the app's name), keywords, release
 * notes and other images stay. The Store's screenshots are deleted for the new ones.
 */
export function listingWith(
  existing: Record<string, unknown> | undefined,
  t: ListingText,
  title: string,
  shots: string[] | null,
): Record<string, unknown> {
  const base = baseOf(existing ?? {});
  const changed: Record<string, unknown> = {
    ...base,
    title: text(base.title) ?? title,
    description: t.description,
    shortDescription: t.shortDescription,
    features: t.features,
  };
  if (shots !== null) {
    const images = Array.isArray(base.images) ? base.images.filter(isObject) : [];
    changed.images = [
      ...images
        .filter((i) => !(i.imageType === 'Screenshot' && i.fileStatus === 'PendingUpload'))
        .map((i) => (i.imageType === 'Screenshot' ? { ...i, fileStatus: 'PendingDelete' } : i)),
      ...shots.map((fileName) => ({
        fileName,
        fileStatus: 'PendingUpload',
        imageType: 'Screenshot',
        description: '',
      })),
    ];
  }
  return { ...(existing ?? {}), baseListing: changed };
}

/** The listing into the next submission: texts, and the screenshots when there are. */
export async function listing(
  c: Connection,
  texts: ListingText[],
  images: Map<string, string[]> | null,
  workDir: string,
  log: Log,
): Promise<string[]> {
  const app = appOf(await call(c, 'getApp', 'GET', ''));
  const plan = planListing(texts);
  const submission = await openSubmission(c, app, log);
  const listings = listingsOf(submission);
  const files: [string, string][] = [];
  for (const t of plan.texts) {
    let shots: string[] | null = null;
    if (images !== null) {
      shots = [];
      for (const [i, file] of (images.get(t.code) ?? []).entries()) {
        const name = `${t.code}/${i + 1}-${path.basename(file)}`;
        files.push([name, file]);
        shots.push(name);
      }
    }
    listings[t.code] = listingWith(listings[t.code], t, app.primaryName, shots);
    log(`${t.code}: the texts${shots === null ? '' : ` and ${shots.length} screenshots`}`);
  }
  const updated = submissionOf(
    await call(
      c,
      'updateSubmission',
      'PUT',
      `/submissions/${submission.id}`,
      writable({ ...submission, listings }),
    ),
    'updateSubmission',
  );
  if (files.length > 0) {
    const uploadUrl = text(updated.fileUploadUrl) ?? text(submission.fileUploadUrl);
    if (uploadUrl === null) throw new StoreError('the submission has no fileUploadUrl', 200);
    const zip = path.join(workDir, 'msstore-listing.zip');
    writeZip(zip, files);
    try {
      await uploadZip(c.fetch, uploadUrl, zip);
    } finally {
      fs.rmSync(zip, { force: true });
    }
    log(`Uploaded ${files.length} screenshots`);
  }
  return [
    `Microsoft Store: the listing in ${plan.texts.length} languages (${plan.texts.map((t) => t.code).join(', ')})${images === null ? ', texts only' : ', with the desktop screenshots'}, in the submission ${submission.id} (not submitted: the next release submits it, or Partner Center)`,
    ...plan.skipped.map((s) => `Not changed: ${s}`),
  ];
}

// --- Command line --------------------------------------------------------------------------

const USAGE = `usage: node scripts/microsoft-store.ts layout --version <x.y.z> --arch <x64|arm64> --exe <hashi.exe> --out <dir>
       node scripts/microsoft-store.ts release --version <x.y.z> [--whatsnew <file>] [--submit] [--dry-run] <msixbundle>
       node scripts/microsoft-store.ts listing [--images <dir>] [--dry-run]`;

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

/** The Entra application and the app from the environment; checked unless a dry run. */
export function environment(env: NodeJS.ProcessEnv, dryRun: boolean) {
  const value = (name: string) => env[name]?.trim() ?? '';
  const tenant = value('MS_STORE_TENANT_ID');
  const clientId = value('MS_STORE_CLIENT_ID');
  const clientSecret = env.MS_STORE_CLIENT_SECRET ?? '';
  const appId = value('MS_STORE_APP_ID');
  const missing = [
    ['MS_STORE_TENANT_ID', tenant],
    ['MS_STORE_CLIENT_ID', clientId],
    ['MS_STORE_CLIENT_SECRET', clientSecret],
    ['MS_STORE_APP_ID', appId],
  ]
    .filter(([, v]) => v === '')
    .map(([name]) => name);
  if (missing.length > 0 && !dryRun) throw new Error(`${missing.join(', ')} not set`);
  const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  if (tenant !== '' && !guid.test(tenant))
    throw new Error("MS_STORE_TENANT_ID isn't a tenant ID (a GUID, Partner Center's Tenant ID)");
  if (clientId !== '' && !guid.test(clientId))
    throw new Error("MS_STORE_CLIENT_ID isn't a client ID (a GUID, the Entra application's)");
  if (appId !== '' && !/^9[0-9A-Z]{11}$/.test(appId))
    throw new Error("MS_STORE_APP_ID isn't a Store ID (9 and 11 letters or digits)");
  return { tenant, clientId, clientSecret, appId };
}

async function connect(env: NodeJS.ProcessEnv): Promise<Connection> {
  const { tenant, clientId, clientSecret, appId } = environment(env, false);
  const token = await accessToken(
    fetch,
    env.MS_STORE_TOKEN_URL ?? tokenUrl(tenant),
    clientId,
    clientSecret,
  );
  return {
    fetch,
    api: env.MS_STORE_API ?? API,
    appId,
    token,
    sleep: (ms) =>
      new Promise((resolve) => {
        setTimeout(resolve, ms);
      }),
  };
}

async function main(args: string[], env: NodeJS.ProcessEnv): Promise<string[]> {
  const command = args.shift();
  const flag = (name: string) => {
    const i = args.indexOf(name);
    if (i !== -1) args.splice(i, 1);
    return i !== -1;
  };
  const option = (name: string) => {
    const i = args.indexOf(name);
    if (i === -1) return undefined;
    const value = args[i + 1];
    if (value === undefined) usage();
    args.splice(i, 2);
    return value;
  };
  const dryRun = flag('--dry-run');
  const log: Log = (line) => console.log(line);
  // The upload's zip, deleted after it
  const workDir = env.RUNNER_TEMP ?? os.tmpdir();

  if (command === 'layout') {
    const version = option('--version');
    const arch = option('--arch');
    const exe = option('--exe');
    const out = option('--out');
    if (version === undefined || exe === undefined || out === undefined || args.length > 0) usage();
    if (!ARCHS.includes(arch as Arch)) usage();
    const identity = identityOf(env);
    const files = layout({ ...identity, version, arch: arch as Arch, exe, out });
    return [
      `${out}: the ${arch} package of ${version} (${msixVersion(version)}), "${identity.name}" by "${identity.publisher}", ${files.length} files${identity.placeholder ? " (a placeholder identity: MS_STORE_IDENTITY_NAME and MS_STORE_PUBLISHER aren't set, so the Store won't take it)" : ''}`,
    ];
  }

  if (command === 'release') {
    const submit = flag('--submit');
    const version = option('--version');
    const whatsnew = option('--whatsnew');
    const bundle = args[0];
    if (version === undefined || bundle === undefined || args.length !== 1) usage();
    const info = bundleInfo(fs.readFileSync(bundle));
    const identity = identityOf(env);
    const problems = bundleProblems(info, { ...identity, version });
    if (problems.length > 0) throw new Error(`the bundle has problems:\n${problems.join('\n')}`);
    const notes = releaseNotes(
      whatsnew === undefined ? '' : fs.readFileSync(whatsnew, 'utf8').trim(),
      version,
    );
    const lines = notes.note === undefined ? [] : [notes.note];
    const checked = `${path.basename(bundle)} (${(fs.statSync(bundle).size / 1048576).toFixed(1)} MB): "${info.name}" by "${info.publisher}" ${info.version}, ${info.packages.map((p) => p.arch).join(' and ')}, full trust, ${info.packages[0]?.languages.length ?? 0} languages`;
    if (identity.placeholder && !dryRun)
      throw new Error(
        `${checked} has the placeholder identity: set MS_STORE_IDENTITY_NAME and MS_STORE_PUBLISHER (docs/stores.md, Microsoft Store) and build it again`,
      );
    environment(env, dryRun);
    if (dryRun) {
      return [
        ...lines,
        `Microsoft Store, dry run: ${checked}. Would replace the packages of the app's next submission, "what's new" "${notes.text.slice(0, 80)}${notes.text.length > 80 ? '...' : ''}"${submit ? ', then commit it for certification' : ', not submitted'}`,
      ];
    }
    const c = await connect(env);
    return lines.concat(
      await release(c, { version, bundle, info, releaseNotes: notes.text, submit, workDir }, log),
    );
  }

  if (command === 'listing') {
    const imagesDir = option('--images');
    if (args.length > 0) usage();
    const texts = listingTexts();
    const problems = texts.flatMap(textProblems);
    // The default language's problems stop it here already
    planListing(texts);
    const images = imagesDir === undefined ? null : listingImages(imagesDir);
    environment(env, dryRun);
    if (dryRun) {
      return [
        `Microsoft Store, dry run: the listing in ${texts.length} languages${images === null ? ', texts only' : `, with ${[...images.values()].reduce((n, s) => n + s.length, 0)} desktop screenshots`}`,
        ...problems.map((p) => `Would keep the Store's text: ${p}`),
      ];
    }
    const c = await connect(env);
    return await listing(c, texts, images, workDir, log);
  }
  usage();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const summary = await main(process.argv.slice(2), process.env);
    for (const line of summary) console.log(line);
    if (process.env.GITHUB_STEP_SUMMARY)
      fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary.join('\n\n')}\n`);
  } catch (e) {
    console.log(`::error::${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
