// node --test scripts/tests/: the checks of the foss build's APKs (scripts/foss-apk.ts)
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  DEPENDENCY_INFO_BLOCK,
  fossVersionCode,
  proprietaryNames,
  signingBlockIds,
} from '../foss-apk.ts';

test("each ABI's versionCode: the version's times ten plus its digit", () => {
  // 1.2.3 is 1002003 on Play
  assert.equal(fossVersionCode(1_002_003, 'armeabi-v7a'), 10_020_031);
  assert.equal(fossVersionCode(1_002_003, 'arm64-v8a'), 10_020_032);
  assert.equal(fossVersionCode(1_002_003, 'x86_64'), 10_020_034);
  // Newer versions above older ones on every ABI, 64-bit above 32-bit within a version
  assert.ok(fossVersionCode(1_002_004, 'armeabi-v7a') > fossVersionCode(1_002_003, 'x86_64'));
  assert.ok(fossVersionCode(1, 'arm64-v8a') > fossVersionCode(1, 'armeabi-v7a'));
  // Play allows versionCodes up to 2100000000: the foss build's fit up to version 209.999.999
  assert.ok(fossVersionCode(209_999_999, 'x86_64') <= 2_100_000_000);
  assert.throws(() => fossVersionCode(1, 'mips'), /no such ABI/);
});

test('finds proprietary libraries by their package names, also as strings of renamed code', () => {
  assert.deepEqual(
    proprietaryNames(
      'Lcom/google/android/gms/games/PlayGames; com.google.firebase.messaging x com.google.android.play.core.review',
    ),
    ['com.google.android.play.core', 'com.google.firebase', 'com/google/android/gms'],
  );
  assert.deepEqual(proprietaryNames(Buffer.from('\0com.android.billingclient.api\0')), [
    'com.android.billingclient',
  ]);
  // The photo picker's intents, which AndroidX Activity sends, aren't Play services' code
  assert.deepEqual(
    proprietaryNames(
      '\0com.google.android.gms.provider.action.PICK_IMAGES\0com.google.android.gms.provider.extra.PICK_IMAGES_MAX\0',
    ),
    [],
  );
  assert.deepEqual(proprietaryNames('com.google.android.gms.provider.FooProvider'), [
    'com.google.android.gms',
  ]);
  // Material, AndroidX and the Play Store's package name (an intent target, not code) are fine
  assert.deepEqual(
    proprietaryNames('Lcom/google/android/material/R; androidx.core com.android.vending'),
    [],
  );
});

/** A ZIP file's tail with an APK signing block of entries with these IDs before its central directory. */
function apk(ids: number[] | null): Buffer {
  const data = Buffer.from('local file headers and data');
  const pairs = (ids ?? []).map((id) => {
    const pair = Buffer.alloc(8 + 4 + 3);
    pair.writeBigUInt64LE(7n, 0);
    pair.writeUInt32LE(id, 8);
    return pair;
  });
  const body = Buffer.concat(pairs);
  const size = BigInt(body.length + 8 + 16);
  const head = Buffer.alloc(8);
  head.writeBigUInt64LE(size);
  const tail = Buffer.alloc(8);
  tail.writeBigUInt64LE(size);
  const block =
    ids === null
      ? Buffer.alloc(0)
      : Buffer.concat([head, body, tail, Buffer.from('APK Sig Block 42')]);
  const centralDirectory = Buffer.from('central directory');
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt32LE(data.length + block.length, 16);
  return Buffer.concat([data, block, centralDirectory, eocd]);
}

test("reads the IDs of an APK's signing block", () => {
  const v2 = 0x7109871a;
  const v3 = 0xf05368c0;
  assert.deepEqual(signingBlockIds(apk([v2, v3])), [v2, v3]);
  assert.ok(signingBlockIds(apk([v2, DEPENDENCY_INFO_BLOCK, v3])).includes(DEPENDENCY_INFO_BLOCK));
  assert.deepEqual(signingBlockIds(apk([])), []);
  // An unsigned (v1 only) APK has none
  assert.deepEqual(signingBlockIds(apk(null)), []);
  assert.throws(
    () => signingBlockIds(Buffer.from('not a zip file at all, no records')),
    /not a ZIP/,
  );
});
