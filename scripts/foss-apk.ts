// Checks the foss build's APKs (app/README.md, Android builds), as the release workflow does
// after building them:
//
//   node scripts/foss-apk.ts --build-tools <dir> --cert <sha256> --version <x.y.z> --code <n> <apk>...
//   node scripts/foss-apk.ts --build-tools <dir> --proprietary <apk>...
//
// `--proprietary` checks only the last part below, for any build (CI's debug build of foss).
// Each APK `hashi-<version>-foss-<abi>.apk` must be rs.hashi in the version, with the versionCode
// of its ABI (`fossVersionCode`), not debuggable, with the native code of its ABI alone, aligned
// for 16 KB pages, signed with the certificate `--cert` alone (SHA-256, the direct key), and
// without anything proprietary: no class or string of Google Play services, Firebase or the Play
// Core libraries in its code (by name, so R8's renaming doesn't hide them: their package names
// stay in the strings), nothing of them in the manifest, and no dependency list in the signing
// block (AGP encrypts it for Google; F-Droid refuses APKs with it).
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** The ABIs of the foss build and the last digit of their versionCodes (build.gradle.kts). */
export const ABI_DIGITS: Readonly<Record<string, number>> = {
  'armeabi-v7a': 1,
  'arm64-v8a': 2,
  x86: 3,
  x86_64: 4,
};

/** The versionCode of an ABI's foss APK: the version's times ten plus the ABI's digit. */
export function fossVersionCode(code: number, abi: string): number {
  const digit = ABI_DIGITS[abi];
  if (digit === undefined) throw new Error(`no such ABI: ${abi}`);
  return code * 10 + digit;
}

/** IDs of the APK signing block's entries. */
export const DEPENDENCY_INFO_BLOCK = 0x504b4453;

/**
 * The IDs of the entries of an APK's signing block (v2/v3 signatures, padding, AGP's dependency
 * list, ...), or none when it has no signing block.
 */
export function signingBlockIds(apk: Buffer): number[] {
  // The end of central directory record: its signature, then the central directory's offset at
  // +16. It is at the end, before a comment of at most 65535 bytes.
  let eocd = -1;
  for (let i = apk.length - 22; i >= Math.max(0, apk.length - 22 - 0xffff); i--) {
    if (apk.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error('not a ZIP file');
  const centralDirectory = apk.readUInt32LE(eocd + 16);
  // The signing block ends right before the central directory: its size, then the magic
  if (centralDirectory < 32) return [];
  if (apk.toString('latin1', centralDirectory - 16, centralDirectory) !== 'APK Sig Block 42')
    return [];
  const size = Number(apk.readBigUInt64LE(centralDirectory - 24));
  const start = centralDirectory - size - 8;
  if (start < 0 || Number(apk.readBigUInt64LE(start)) !== size)
    throw new Error('a broken signing block');
  const ids: number[] = [];
  let at = start + 8;
  while (at < centralDirectory - 24) {
    const length = Number(apk.readBigUInt64LE(at));
    if (length < 4 || at + 8 + length > centralDirectory - 24)
      throw new Error('a broken signing block');
    ids.push(apk.readUInt32LE(at + 8));
    at += 8 + length;
  }
  return ids;
}

/** Names of proprietary libraries, as class descriptors, package names or paths. */
const PROPRIETARY =
  /com[./]google[./](?:android[./]gms|firebase|android[./]play[./]core|android[./]datatransport)|com[./]android[./]billingclient/g;

/**
 * Names of Google Play services that aren't its code: the intents of the photo picker's backport,
 * which AndroidX Activity's `PickVisualMedia` sends (the system answers them without Play
 * services; R8 drops them from release builds where the app doesn't pick images).
 */
const NOT_CODE = /com\.google\.android\.gms\.provider\.(?:action|extra)\.[A-Z_]+/g;

/** The proprietary names in a text or a binary file (the strings of a DEX file). */
export function proprietaryNames(content: Buffer | string): string[] {
  const text = (typeof content === 'string' ? content : content.toString('latin1')).replace(
    NOT_CODE,
    '',
  );
  return [...new Set(text.match(PROPRIETARY) ?? [])].sort((a, b) => (a < b ? -1 : 1));
}

function run(tool: string, args: string[]): string {
  return execFileSync(tool, args, { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
}

/** The problems of one foss APK, none when it is fine. */
export function checkApk(
  apk: string,
  {
    buildTools,
    cert,
    version,
    code,
  }: { buildTools: string; cert: string; version: string; code: number },
): string[] {
  const name = path.basename(apk);
  const abi = new RegExp(`^hashi-${version.replaceAll('.', '\\.')}-foss-(.+)\\.apk$`).exec(
    name,
  )?.[1];
  if (abi === undefined || ABI_DIGITS[abi] === undefined)
    return [`${name}: not hashi-${version}-foss-<abi>.apk`];
  const problems: string[] = [];
  const problem = (text: string) => problems.push(`${name}: ${text}`);
  const tool = (t: string) => path.join(buildTools, t);

  const badging = run(tool('aapt2'), ['dump', 'badging', apk]);
  const field = (re: RegExp) => re.exec(badging)?.[1];
  if (field(/^package: name='([^']*)'/m) !== 'rs.hashi')
    problem(`package ${field(/^package: name='([^']*)'/m)}, not rs.hashi`);
  const expectedCode = fossVersionCode(code, abi);
  if (field(/versionCode='(\d+)'/) !== String(expectedCode))
    problem(`versionCode ${field(/versionCode='(\d+)'/)}, not ${expectedCode}`);
  if (field(/versionName='([^']*)'/) !== version)
    problem(`versionName ${field(/versionName='([^']*)'/)}, not ${version}`);
  if (/^application-debuggable/m.test(badging)) problem('debuggable');
  const native = field(/^native-code: (.*)$/m);
  if (native !== `'${abi}'`) problem(`native code ${native}, not '${abi}' alone`);

  try {
    const signers = run(tool('apksigner'), ['verify', '--print-certs', apk]);
    const digests = [...signers.matchAll(/^Signer #\d+ certificate SHA-256 digest: (\w+)$/gm)].map(
      (m) => m[1],
    );
    if (digests.length !== 1 || digests[0] !== cert.toLowerCase())
      problem(`signed by ${digests.join(', ') || 'nobody'}, not by ${cert} alone`);
  } catch {
    problem("the signature doesn't verify");
  }
  try {
    run(tool('zipalign'), ['-c', '-P', '16', '4', apk]);
  } catch {
    problem("isn't aligned for 16 KB pages");
  }

  problems.push(...proprietaryProblems(apk, buildTools));
  return problems;
}

/**
 * What an APK has of proprietary code (any build, debug ones too): names of the libraries in its
 * code and its manifest, AGP's dependency list in its signing block.
 */
export function proprietaryProblems(apk: string, buildTools: string): string[] {
  const name = path.basename(apk);
  const problems: string[] = [];
  const files = run('unzip', ['-Z1', apk]).split('\n');
  const dexes = files.filter((f) => /^classes\d*\.dex$/.test(f));
  if (dexes.length === 0) problems.push(`${name}: no classes.dex`);
  for (const dex of dexes) {
    const found = proprietaryNames(
      execFileSync('unzip', ['-p', apk, dex], { maxBuffer: 256 * 1024 * 1024 }),
    );
    if (found.length > 0) problems.push(`${name}: ${dex} has ${found.join(', ')}`);
  }
  const manifest = proprietaryNames(
    run(path.join(buildTools, 'aapt2'), ['dump', 'xmltree', '--file', 'AndroidManifest.xml', apk]),
  );
  if (manifest.length > 0) problems.push(`${name}: the manifest has ${manifest.join(', ')}`);
  if (signingBlockIds(fs.readFileSync(apk)).includes(DEPENDENCY_INFO_BLOCK))
    problems.push(`${name}: has AGP's dependency list in its signing block (dependenciesInfo)`);
  return problems;
}

const USAGE = `usage: node scripts/foss-apk.ts --build-tools <dir> --cert <sha256> --version <x.y.z> --code <n> <apk>...
       node scripts/foss-apk.ts --build-tools <dir> --proprietary <apk>...   (any build: only what it has of proprietary code)`;

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const proprietary = args.includes('--proprietary');
  if (proprietary) args.splice(args.indexOf('--proprietary'), 1);
  const option = (flag: string) => {
    const i = args.indexOf(flag);
    const value = args[i + 1];
    if (i === -1 || value === undefined) {
      console.error(USAGE);
      process.exit(2);
    }
    args.splice(i, 2);
    return value;
  };
  const buildTools = option('--build-tools');
  const check = proprietary
    ? (apk: string) => proprietaryProblems(apk, buildTools)
    : (() => {
        const options = {
          buildTools,
          cert: option('--cert'),
          version: option('--version'),
          code: Number(option('--code')),
        };
        return (apk: string) => checkApk(apk, options);
      })();
  if (args.length === 0) {
    console.error(USAGE);
    process.exit(2);
  }
  const problems = args.flatMap(check);
  for (const problem of problems) console.log(`::error::${problem}`);
  if (problems.length > 0) process.exit(1);
  console.log(
    `${args.map((apk) => path.basename(apk)).join(', ')}: ${proprietary ? 'nothing proprietary' : 'fine'}`,
  );
}
