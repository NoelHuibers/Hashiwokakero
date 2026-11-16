// Makes a key pair for Web Push (VAPID, RFC 8292) and prints it as the server reads it
// (src/server/notify/web_push.rs): the public key as the uncompressed P-256 point, the private
// key as its scalar, both base64url. `pnpm run vapid-keys`, then for example
// `fly secrets set VAPID_PUBLIC_KEY=… VAPID_PRIVATE_KEY=…`.
//
// A new pair invalidates every browser's subscription: they were made for the old public key,
// and push services refuse messages signed with another one. Keep the pair.
import { generateKeyPairSync } from 'node:crypto';

const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
const jwk = privateKey.export({ format: 'jwk' });
if (!(jwk.d && jwk.x && jwk.y)) throw new Error('the key has no P-256 coordinates');
const point = Buffer.concat([
  Buffer.from([4]),
  Buffer.from(jwk.x, 'base64url'),
  Buffer.from(jwk.y, 'base64url'),
]);
console.log(`VAPID_PUBLIC_KEY=${point.toString('base64url')}`);
console.log(`VAPID_PRIVATE_KEY=${jwk.d}`);
