// The contact form: what it refuses, and a message that arrives. Without SMTP (as here) the
// message is only stored, the admin dashboard lists it.
import { allow, expect, test, visit } from '../fixtures';

test('the contact form checks the message and sends it', async ({ page, problems }) => {
  await visit(page, '/kontakt');
  const message = page.getByLabel('Message');
  const send = page.getByRole('button', { name: /^Send/ });

  // Too short: the browser keeps the form
  await message.fill('Hi there');
  await send.click();
  expect(await message.evaluate((m: HTMLTextAreaElement) => m.validity.tooShort)).toBe(true);
  await expect(page.getByText('Thank you!')).toHaveCount(0);

  // Long enough for the browser, but only blanks: the server says what's wrong
  await message.fill(' '.repeat(12));
  await send.click();
  await expect(page.getByText('Please write between 10 and 5000 characters.')).toBeVisible();
  allow(problems, /\/api\/send_message/, /status of 4\d\d|status of 5\d\d/);

  await page.getByLabel(/^Name/).fill('Playwright');
  await page.getByLabel(/^Email/).fill('playwright@example.com');
  await message.fill('The bridges on the ladder puzzle are lovely.');
  await expect(page.getByText('44 / 5000')).toBeVisible();
  await send.click();
  await expect(page.getByText('Thank you!')).toBeVisible();
  await expect(page.getByText('Your message has arrived.', { exact: false })).toBeVisible();
  await page.getByRole('button', { name: 'Write another message' }).click();
  await expect(message).toHaveValue('');
});
