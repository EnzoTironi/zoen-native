import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium } from 'playwright-core';
import { deleteForEveryone, waitForDeletedMessage } from './delete-message.mjs';
import { journeyEvidence } from './evidence.mjs';

// Actual browser selector/dialog/evidence tests on controlled DOM fixtures.
// These do not replace the separate real relay/MLS two-person journey.
let browser;
before(async () => {
  browser = await chromium.launch({ executablePath: process.env.CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox'] });
});
after(async () => { await browser?.close(); });

const html = `<div class="zm-app"><main class="zm-main">
  <article data-testid="message" id="message-other"><div class="zm-deleted"><span class="zm-text">Mensagem apagada</span></div></article>
  <article data-testid="message" id="message-target"><div class="zm-bubble"><span class="zm-text">mensagem errada</span></div>
    <button aria-label="Mais ações">⋯</button><button role="menuitem">Apagar para todos</button>
  </article></main></div>`;

test('another deleted row cannot satisfy deletion of the intended message', async () => {
  const page = await browser.newPage();
  try {
    await page.setContent(html);
    await assert.rejects(waitForDeletedMessage(page, 'message-target', 200), /Timeout/);
    await page.locator('#message-target .zm-bubble').evaluate((el) => {
      el.classList.add('zm-deleted'); el.querySelector('.zm-text').textContent = 'Mensagem apagada';
    });
    await waitForDeletedMessage(page, 'message-target', 200);
    await page.locator('#message-target').evaluate((el) => el.remove());
    await assert.rejects(waitForDeletedMessage(page, 'message-target', 200), /Timeout/);
  } finally { await page.close(); }
});

test('delete confirms the exact dialog and both message projections before passing', async () => {
  const sender = await browser.newPage();
  const receiver = await browser.newPage();
  const stages = [];
  try {
    await Promise.all([sender, receiver].map((page) => page.setContent(html)));
    await sender.exposeFunction('deliverDelete', async () => {
      await receiver.locator('#message-target .zm-bubble').evaluate((el) => {
        el.classList.add('zm-deleted'); el.querySelector('.zm-text').textContent = 'Mensagem apagada';
      });
    });
    await sender.getByRole('menuitem').evaluate((button) => {
      button.onclick = () => {
        if (!window.confirm('Apagar esta mensagem para todos?')) return;
        const bubble = document.querySelector('#message-target .zm-bubble');
        bubble.classList.add('zm-deleted'); bubble.querySelector('.zm-text').textContent = 'Mensagem apagada';
        void window.deliverDelete();
      };
    });
    await deleteForEveryone(sender, receiver, 'mensagem errada', async (stage, details) => { stages.push({ stage, details }); });
    assert.deepEqual(stages.map(({ stage }) => stage), ['delete-clicked', 'delete-confirmed']);
    assert.equal(stages[0].details.messageId, 'message-target');
    assert.deepEqual(stages[0].details.confirmation, { type: 'confirm', message: 'Apagar esta mensagem para todos?' });
  } finally { await Promise.all([sender.close(), receiver.close()]); }
});

test('failure evidence preserves both DOM projections and transient browser errors', async () => {
  const page = await browser.newPage();
  const directory = await mkdtemp(join(tmpdir(), 'zoen-delete-evidence-'));
  try {
    await page.setContent(html);
    const capture = journeyEvidence([{ name: 'Ana', page }, { name: 'Bruno' }], directory, ['Ana: synthetic refusal']);
    await capture('delete-failed', { messageId: 'message-target' });
    const saved = JSON.parse(await readFile(join(directory, 'delete-failed.json'), 'utf8'));
    assert.deepEqual(saved.errors, ['Ana: synthetic refusal']);
    assert.equal(saved.pages[0].state[0].messages.find((message) => message.id === 'message-target').deleted, false);
    assert.deepEqual(saved.pages[0].captureErrors, []);
    assert.equal(saved.pages[1].unavailable, true);
    assert.match(await readFile(join(directory, 'delete-failed-ana.html'), 'utf8'), /mensagem errada/);
    assert.ok((await readFile(join(directory, 'delete-failed-ana.png'))).length > 0);
  } finally { await page.close(); await rm(directory, { recursive: true, force: true }); }
});
