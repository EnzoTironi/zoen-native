// Two people, each with a CLI "phone" that links a headless browser; they chat on the web.
import { chromium } from 'playwright-core';
import { spawn, execFileSync } from 'node:child_process';
const RELAY = process.env.RELAY ?? 'http://127.0.0.1:8790';
const WEB = process.env.WEB ?? 'http://127.0.0.1:4174';
const ZOEN = process.env.ZOEN_CLI ?? 'target/debug/zoen';
const SHOTS = process.env.ZOEN_SHOTS ?? '/tmp/zoen-web-shots';
import { mkdirSync } from 'node:fs'; mkdirSync(SHOTS, { recursive: true });
const run = (home, ...args) => execFileSync(ZOEN, ['--home', home, ...args], { encoding: 'utf8', timeout: 60000 });
const tag = Date.now().toString(36).slice(-5);
const people = [{ name: 'Ana', handle: `ana${tag}` }, { name: 'Bruno', handle: `bruno${tag}` }];
for (const p of people) { p.home = `/tmp/zw-${p.handle}`; run(p.home, 'init', '--name', p.name, '--handle', p.handle, '--relay', RELAY); }
const [ana, bruno] = people;
const browser = await chromium.launch({ executablePath: process.env.CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox', '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream', '--autoplay-policy=no-user-gesture-required'] });
const watchers = [];
const errors = [];
async function linkWeb(p) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 2, locale: 'pt-BR', permissions: ['microphone'] });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => errors.push(`${p.name}: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`${p.name}: ${m.text()}`); });
  await page.goto(`${WEB}/?relay=${encodeURIComponent(RELAY)}`);
  await page.getByRole('button', { name: 'Conectar este navegador' }).waitFor();
  if (p === ana) await page.screenshot({ path: `${SHOTS}/01-conectar.png` });
  await page.getByRole('button', { name: 'Conectar este navegador' }).click();
  await page.getByText('Usar código em texto').click();
  const code = (await page.locator('.zw-code').textContent()).trim();
  if (p === ana) await page.screenshot({ path: `${SHOTS}/02-codigo.png` });
  const out = run(p.home, 'link', code);
  console.log(p.name, 'link:', out.trim().split('\n').at(-1));
  await page.waitForSelector('.zm-app', { timeout: 60000 });
  p.page = page;
}
await linkWeb(ana); await linkWeb(bruno);
const watch = (p) => { p.watcher = spawn(ZOEN, ['--home', p.home, 'watch', '--for', '900'], { stdio: 'ignore' }); watchers.push(p.watcher); };
const offline = async (p, fn) => { p.watcher?.kill(); await new Promise((r) => setTimeout(r, 1500)); try { fn(); } finally { watch(p); } };
run(ana.home, 'dm', `@${bruno.handle}`, 'Oi Bruno! Tudo certo pra sexta?');
people.forEach(watch);
const ready = async (page) => { await page.locator('.zm-chat-open').first().click({ timeout: 90000 }); await page.waitForSelector('.zm-composer textarea:not([disabled])', { timeout: 90000 }); };
await Promise.all([ready(ana.page), ready(bruno.page)]);
const send = async (page, text) => { await page.locator('.zm-composer textarea').first().fill(text); await page.keyboard.press('Enter'); };
await bruno.page.locator('.zm-main .zm-text', { hasText: 'Oi Bruno! Tudo certo pra sexta?' }).waitFor();
await send(bruno.page, 'Tudo! Levo o bolo 🎂');
await ana.page.locator('.zm-main .zm-text', { hasText: 'Levo o bolo' }).waitFor({ timeout: 30000 });
await send(ana.page, 'Perfeito. Que horas você chega?');
// Ticks: Bruno has the chat open, so it should reach read (blue).
await ana.page.locator('.zm-own .zm-ticks-read').last().waitFor({ timeout: 30000 });
await ana.page.screenshot({ path: `${SHOTS}/03-ticks-lidos.png` });
// Reaction from Bruno on Ana's message.
const msg = bruno.page.locator('[data-testid=message]', { hasText: 'Que horas' });
await msg.hover(); await msg.getByRole('button', { name: 'Reagir' }).click();
await bruno.page.screenshot({ path: `${SHOTS}/04-reagir.png` });
await bruno.page.getByRole('dialog', { name: 'Escolher reação' }).getByRole('button', { name: '❤️', exact: true }).click();
await ana.page.locator('.zm-reactions button', { hasText: '❤️' }).waitFor({ timeout: 30000 });
// Inline reply.
const cake = ana.page.locator('[data-testid=message]', { hasText: 'Levo o bolo' });
await cake.hover(); await cake.getByRole('button', { name: 'Responder', exact: true }).click();
await send(ana.page, 'Você é demais');
await bruno.page.locator('.zm-quote', { hasText: 'Levo o bolo' }).waitFor({ timeout: 30000 });
// Edit own message.
const mine = ana.page.locator('[data-testid=message]', { hasText: 'Você é demais' });
await mine.hover(); await mine.getByRole('button', { name: 'Mais ações' }).click();
await ana.page.screenshot({ path: `${SHOTS}/05-menu-acoes.png` });
await ana.page.getByRole('menuitem', { name: 'Editar' }).click();
const editor = ana.page.getByRole('dialog', { name: 'Editar mensagem' });
await editor.getByRole('textbox').fill('Você é demais, Bruno!');
await editor.getByRole('button', { name: 'Salvar', exact: true }).click();
await bruno.page.locator('.zm-main .zm-text', { hasText: 'Você é demais, Bruno!' }).waitFor({ timeout: 30000 });
await bruno.page.locator('.zm-meta', { hasText: 'editada' }).waitFor();
// Thread.
const root = bruno.page.locator('[data-testid=message]', { hasText: 'Que horas' });
await root.hover(); await root.getByRole('button', { name: 'Abrir respostas', exact: true }).click();
await bruno.page.locator('.zm-thread textarea').fill('Umas 19h, pode ser?');
await bruno.page.keyboard.press('Enter');
await ana.page.locator('.zm-thread-summary', { hasText: '1 resposta' }).waitFor({ timeout: 30000 });
await bruno.page.screenshot({ path: `${SHOTS}/06-thread-painel.png` });
await ana.page.locator('.zm-thread-summary').click();
await ana.page.locator('.zm-thread textarea').fill('Fechado 👍');
await ana.page.keyboard.press('Enter');
await bruno.page.locator('.zm-thread', { hasText: 'Fechado' }).waitFor({ timeout: 30000 });
await ana.page.screenshot({ path: `${SHOTS}/07-thread-ana.png` });
// Delete for everyone.
await send(bruno.page, 'mensagem errada');
await ana.page.locator('.zm-main .zm-text', { hasText: 'mensagem errada' }).waitFor({ timeout: 30000 });
const wrong = bruno.page.locator('.zm-main [data-testid=message]', { hasText: 'mensagem errada' });
bruno.page.once('dialog', (d) => d.accept());
await wrong.hover(); await wrong.getByRole('button', { name: 'Mais ações' }).click();
await bruno.page.getByRole('menuitem', { name: 'Apagar para todos' }).click();
await ana.page.locator('.zm-main .zm-deleted').waitFor({ timeout: 30000 });
// A second chat, unread + pinned/muted/archived in the list.
await offline(bruno, () => { run(bruno.home, 'group', 'Churrasco de sexta', `@${ana.handle}`); run(bruno.home, 'send', 'Churrasco de sexta', `@${ana.name.toLowerCase()} traz o carvão?`); });
await ana.page.locator('.zm-chat-open', { hasText: 'Churrasco de sexta' }).waitFor({ timeout: 60000 });
await ana.page.locator('.zm-chat', { hasText: 'Churrasco' }).locator('.zm-badge').waitFor({ timeout: 60000 }).catch(() => {});
await ana.page.screenshot({ path: `${SHOTS}/08-lista-nao-lidas.png` });
const row = ana.page.locator('.zm-chat', { hasText: 'Bruno' }).first();
await row.hover(); await row.locator('.zm-chat-more').click(); await ana.page.getByRole('menuitem', { name: 'Fixar' }).click();
await row.hover(); await row.locator('.zm-chat-more').click(); await ana.page.getByRole('menuitem', { name: 'Silenciar' }).click();
await ana.page.screenshot({ path: `${SHOTS}/09-fixada-silenciada.png` });
// Forward.
const fw = ana.page.locator('.zm-main [data-testid=message]', { hasText: 'Levo o bolo' }).first();
await fw.hover(); await fw.getByRole('button', { name: 'Mais ações' }).click();
await ana.page.getByRole('menuitem', { name: 'Encaminhar' }).click();
await ana.page.screenshot({ path: `${SHOTS}/10-encaminhar.png` });
await ana.page.getByRole('dialog').getByRole('button', { name: /Churrasco/ }).click();
await ana.page.locator('.zm-forwarded').first().waitFor({ timeout: 30000 });
await ana.page.locator('.zm-main .zm-own .zm-ticks').last().waitFor({ timeout: 30000 }); await ana.page.waitForTimeout(500);
await ana.page.screenshot({ path: `${SHOTS}/11-encaminhada.png` });
const crow = ana.page.locator('.zm-chat', { hasText: 'Churrasco' }).first();
await crow.hover(); await crow.locator('.zm-chat-more').click(); await ana.page.getByRole('menuitem', { name: 'Arquivar' }).click();
await ana.page.screenshot({ path: `${SHOTS}/12-arquivadas.png` });
// Pick a chat by its exact title, never by preview text (a group preview can mention @ana).
const chat = (page, title) => page.locator('.zm-chat').filter({ has: page.locator('.zm-chat-top b').getByText(title, { exact: true }) });
await chat(bruno.page, `@${ana.handle}`).click();
await bruno.page.waitForTimeout(500); await bruno.page.screenshot({ path: `${SHOTS}/13-bruno-chat.png` });
// Empty composer shows the mic; typing swaps to send.
await bruno.page.locator('.zm-main .zm-composer textarea').fill('');
await bruno.page.locator('.zm-main .zm-composer').screenshot({ path: `${SHOTS}/14-composer-mic.png` });
await bruno.page.locator('.zm-main .zm-composer textarea').fill('Até sexta!');
await bruno.page.locator('.zm-main .zm-composer').screenshot({ path: `${SHOTS}/15-composer-enviar.png` });
// Media: photo, video, file and a voice note, end-to-end encrypted through the relay.
const fixtures = new URL('./fixtures/', import.meta.url).pathname;
await chat(ana.page, `@${bruno.handle}`).click();
const attach = async (page, file) => { await page.locator('.zm-main input[type=file]').setInputFiles(fixtures + file, { timeout: 15000 }); };
await attach(ana.page, 'praia.jpg');
await bruno.page.locator('.zm-main .zm-visual.zm-image img').last().waitFor({ timeout: 40000 });
await attach(ana.page, 'video.webm');
await bruno.page.locator('.zm-main .zm-visual.zm-video').last().waitFor({ timeout: 60000 });
await attach(ana.page, 'cardapio.pdf');
await bruno.page.locator('.zm-main .zm-file', { hasText: 'cardapio.pdf' }).waitFor({ timeout: 60000 });
await bruno.page.waitForTimeout(400);
await bruno.page.screenshot({ path: `${SHOTS}/16-midia-recebida.png` });
await bruno.page.locator('.zm-main .zm-visual.zm-image').last().click();
await bruno.page.locator('.zm-lightbox img').waitFor({ timeout: 30000 }); await bruno.page.waitForTimeout(300);
await bruno.page.screenshot({ path: `${SHOTS}/17-foto-aberta.png` });
await bruno.page.locator('.zm-lightbox button', { hasText: 'Fechar' }).click();
await bruno.page.locator('.zm-main .zm-visual.zm-video').last().click();
await bruno.page.locator('.zm-lightbox video').waitFor({ timeout: 30000 }); await bruno.page.waitForTimeout(1200);
await bruno.page.screenshot({ path: `${SHOTS}/18-video-tocando.png` });
await bruno.page.locator('.zm-lightbox button', { hasText: 'Fechar' }).click();
await bruno.page.locator('.zm-main .zm-composer textarea').fill('');
await bruno.page.locator('.zm-main').getByRole('button', { name: 'Gravar mensagem de voz' }).click();
await bruno.page.waitForTimeout(2500);
await bruno.page.screenshot({ path: `${SHOTS}/19-gravando-voz.png` });
await bruno.page.getByRole('button', { name: 'Enviar mensagem de voz' }).click();
const voice = ana.page.locator('.zm-main [data-testid=voice]').last();
await voice.waitFor({ timeout: 60000 });
await voice.getByRole('button', { name: 'Ouvir mensagem de voz' }).click({ timeout: 30000 });
await ana.page.waitForTimeout(1200);
await ana.page.screenshot({ path: `${SHOTS}/20-voz-tocando.png` });
await ana.page.locator('.zm-main .zm-own').last().screenshot({ path: `${SHOTS}/21-arquivo-enviado.png` });
const fatal = errors;
console.log('errors:', JSON.stringify(errors));
if (fatal.length) throw new Error(`Browser errors: ${fatal.join(' | ')}`);
watchers.forEach((w) => w.kill());
await browser.close();
console.log('E2E OK');
