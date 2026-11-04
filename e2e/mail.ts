// The mails the server sent to players: with HASHI_MAIL_DIR (server.ts) it writes each one as a
// JSON file instead of sending it (src/server/mail.rs).
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { expect } from '@playwright/test';

import { MAIL_DIR } from './env.ts';

export interface Mail {
  to: string;
  subject: string;
  body: string;
}

/** The mails to `address` so far, oldest first. */
export function mailsTo(address: string): Mail[] {
  let names: string[];
  try {
    names = readdirSync(MAIL_DIR);
  } catch {
    return [];
  }
  return names
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => JSON.parse(readFileSync(path.join(MAIL_DIR, name), 'utf8')) as Mail)
    .filter((mail) => mail.to.toLowerCase() === address.toLowerCase());
}

/** Waits for mail number `n` (from 1) to `address` and returns it. */
export async function nthMailTo(address: string, n: number): Promise<Mail> {
  await expect.poll(() => mailsTo(address).length, { message: `mail ${n} to ${address}` }).toBe(n);
  const mail = mailsTo(address)[n - 1];
  if (!mail) throw new Error(`no mail ${n} to ${address}`);
  return mail;
}

/** The link in a mail as a path of this site: the mails link to https://hashi.rs. */
export function linkIn(mail: Mail): string {
  const link = /https:\/\/hashi\.rs(\/\S*#[0-9a-f]{64})/.exec(mail.body)?.[1];
  if (!link) throw new Error(`no link in ${mail.body}`);
  return link;
}
