import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

// Only synthetic journey pages are captured. Never export storage, keys or cookies.
export function journeyEvidence(people, directory, errors) {
  return async (stage, details = {}) => {
    await mkdir(directory, { recursive: true });
    const pages = await Promise.all(people.map(async ({ name, page }) => {
      if (!page || page.isClosed()) return { name, unavailable: true };
      const prefix = join(directory, `${stage}-${name.toLowerCase()}`);
      const captures = await Promise.allSettled([
        page.screenshot({ path: `${prefix}.png`, timeout: 5000 }),
        page.content().then((html) => writeFile(`${prefix}.html`, html)),
        page.locator('.zm-app').evaluateAll((apps) => apps.map((app) => ({
          selectedSpace: app.querySelector('.zm-chat.zm-active')?.getAttribute('data-space-id'),
          status: [...app.querySelectorAll('[role=status], [role=alert]')].map((el) => el.textContent),
          messages: [...app.querySelectorAll('[data-testid=message]')].map((row) => ({
            id: row.id,
            thread: !!row.closest('.zm-thread'),
            deleted: !!row.querySelector('.zm-deleted'),
            text: row.querySelector('.zm-text')?.textContent,
            delivery: row.querySelector('.zm-ticks, .zm-failed')?.getAttribute('aria-label'),
          })),
        }))),
      ]);
      return {
        name, url: page.url(),
        state: captures[2].status === 'fulfilled' ? captures[2].value : null,
        captureErrors: captures.flatMap((result, index) => result.status === 'rejected'
          ? [{ capture: ['screenshot', 'dom', 'state'][index], error: String(result.reason) }] : []),
      };
    }));
    await writeFile(join(directory, `${stage}.json`), JSON.stringify({ details, errors, pages }, null, 2) + '\n');
  };
}
