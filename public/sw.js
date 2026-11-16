// Hashi's service worker: notifications only (docs/notifications.md).
//
// Registered when a player turns notifications on in this browser (src/state/notify.rs), at a
// fixed address without a hash, so a new version replaces it in place. It shows what the server
// pushes (src/server/notify/web_push.rs: title, body, url, tag, already in the browser's
// language) and opens its page on a tap.
//
// Deliberately no fetch handler and no caches: every request goes to the network as if there were
// no service worker, so no page or asset can go stale behind it.

/// <reference lib="webworker" />

const sw = /** @type {ServiceWorkerGlobalScope} */ (/** @type {unknown} */ (self));

sw.addEventListener('install', () => {
  // Nothing to set up: a new version takes over at once
  sw.skipWaiting();
});

sw.addEventListener('activate', (event) => {
  event.waitUntil(sw.clients.claim());
});

/**
 * @typedef {{ title: string, body: string, url: string, tag: string }} Message
 */

/** @param {PushEvent} event */
function message(event) {
  try {
    const data = /** @type {Partial<Message>} */ (event.data?.json() ?? {});
    return {
      title: typeof data.title === 'string' ? data.title : 'Hashi',
      body: typeof data.body === 'string' ? data.body : '',
      url: typeof data.url === 'string' ? data.url : '/',
      tag: typeof data.tag === 'string' ? data.tag : '',
    };
  } catch {
    return { title: 'Hashi', body: '', url: '/', tag: '' };
  }
}

sw.addEventListener('push', (event) => {
  const { title, body, url, tag } = message(event);
  event.waitUntil(
    sw.registration.showNotification(title, {
      body,
      // The same tag replaces the notification before it (a shared puzzle's count of solvers)
      ...(tag && { tag }),
      icon: '/icon-192.png',
      badge: '/badge-96.png',
      data: { url },
    }),
  );
});

/**
 * Only the site's own pages, whatever a message says.
 * @param {unknown} url
 */
function target(url) {
  try {
    const parsed = new URL(typeof url === 'string' ? url : '/', sw.location.origin);
    return parsed.origin === sw.location.origin ? parsed.href : sw.location.origin;
  } catch {
    return sw.location.origin;
  }
}

/**
 * Brings an open tab of the site to the front and shows the page there, or opens a new one.
 * @param {string} url
 */
async function open(url) {
  const windows = await sw.clients.matchAll({ type: 'window', includeUncontrolled: true });
  const tab = windows.find((client) => new URL(client.url).origin === sw.location.origin);
  if (tab) {
    const focused = await tab.focus();
    // Fails for a tab this worker doesn't control (opened before it was registered): then it
    // stays on its page, and a new one opens the target
    try {
      await focused.navigate(url);
      return;
    } catch {
      // Below
    }
  }
  await sw.clients.openWindow(url);
}

sw.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const data = /** @type {{ url?: unknown } | null} */ (event.notification.data);
  event.waitUntil(open(target(data?.url)));
});
