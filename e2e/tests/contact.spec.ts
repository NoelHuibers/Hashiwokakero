// The feedback form: what it refuses, a message that arrives, and one opened from the header's
// help with the page it came from. Without SMTP (as here) messages are only stored, the admin
// dashboard lists them.
import { allow, expect, signIn, test, visit } from '../fixtures';

test('the feedback form checks the message and sends it', async ({ page, problems }) => {
  await visit(page, '/kontakt');
  const message = page.getByLabel('Message');
  const send = page.getByRole('button', { name: /^Send/ });
  // Opened without a type: something else
  await expect(page.getByRole('radio', { name: 'Something else' })).toHaveAttribute('aria-checked', 'true');

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

test('a bug report from the help menu brings its page along', async ({ page, context }) => {
  await signIn(context, 'aiko');
  await visit(page, '/practice');
  const help = page.locator('header').getByRole('button', { name: 'Help' });
  await help.click();
  await expect(help).toHaveAttribute('aria-expanded', 'true');
  await page.locator('header').getByRole('link', { name: 'Report a bug' }).click();
  await expect(page).toHaveURL(/\/kontakt\?type=bug&from=\/practice$/);
  // The menu closed on the way
  await expect(page.locator('header').getByRole('link', { name: 'Report a bug' })).toHaveCount(0);

  const bug = page.getByRole('radio', { name: 'Bug' });
  await expect(bug).toHaveAttribute('aria-checked', 'true');
  const message = page.getByLabel('Message');
  await expect(message).toHaveAttribute('placeholder', /what happened/);

  // What goes along is listed before sending
  const sent = page.getByTestId('feedback-context');
  await sent.locator('summary').click();
  for (const [name, value] of [
    ['Version', 'Website'],
    ['Platform', 'Web'],
    ['Language', 'en'],
    ['Page', '/practice'],
  ]) {
    await expect(sent.locator('dt', { hasText: new RegExp(`^${name}$`) }).locator('xpath=following-sibling::dd[1]')).toHaveText(value);
  }
  await expect(sent.locator('dt', { hasText: /^Player$/ }).locator('xpath=following-sibling::dd[1]')).toHaveText(/^#\d+$/);
  await expect(sent.locator('dt', { hasText: /^Screen$/ }).locator('xpath=following-sibling::dd[1]')).toHaveText(/^\d+×\d+ @[\d.]+x$/);

  const text = `The hint on the practice board did nothing ${Date.now()}`;
  await message.fill(text);
  await page.getByRole('button', { name: /^Send/ }).click();
  await expect(page.getByText('Thank you!')).toBeVisible();

  // In the admins' inbox: a new bug, with where it came from
  await signIn(context, 'admin');
  await visit(page, '/admin?tab=messages');
  const article = page.locator('article').filter({ hasText: text });
  await expect(article).toHaveAttribute('data-status', 'new');
  await expect(article).toContainText('Bug');
  await expect(article.getByTestId('message-context')).toContainText('website · Web');
  await expect(article.getByTestId('message-context')).toContainText('from /practice');
  // Answered, it leaves the new ones
  await article.getByRole('button', { name: 'Answered' }).click();
  await expect(article).toHaveCount(0);
  await page.getByLabel('Status').selectOption('answered');
  await expect(page.locator('article').filter({ hasText: text })).toHaveAttribute('data-status', 'answered');
});
