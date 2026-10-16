// Offline production-component inputs only; no adapter/runtime/backend verification.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const { webkit } = await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE ?? '/tmp/living-space-browser/node_modules/playwright/index.mjs').href);
const out = resolve(homedir(), 'living-space-shots');
const check = process.argv.includes('--check');
if (!check && !process.argv.includes('--production-frozen')) throw Error('Capture requires explicit --production-frozen; use --check for selector checks without images.');
await mkdir(out, { recursive: true });
const root = resolve(import.meta.dirname, '../../node_modules/.cache/living-space-fixture');
const assets = Object.fromEntries(await Promise.all(['index.html','living-space.js','living-space.css'].map(async name => [name,await readFile(resolve(root,name))])));
async function load(page, theme) {
  await page.goto(`https://living-space-fixture.invalid/?theme=${theme}`);
  await page.getByTestId('living-space-home').waitFor();
  await page.evaluate(() => document.fonts.ready);
  await page.locator('.zl-cover img').evaluate(async image => { if (!image.complete) await new Promise((resolve,reject) => { image.onload=resolve; image.onerror=reject; }); });
}
async function section(page, name) { await page.getByRole('heading', {name,exact:true}).evaluate(el => el.scrollIntoView({block:'start'})); }
async function metrics(page) {
  return page.evaluate(() => {
    const bounds = element => { if (!element) throw Error('Missing Home measurement target'); const r=element.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height}; };
    const heading = name => [...document.querySelectorAll('h1,h2')].find(el => el.textContent.trim()===name);
    return {evidence:'Offline production-component fixture; raw DOM bounds, not native pixel-parity or adapter/runtime verification',coordinateSpace:'Browser viewport CSS pixels; no normalization or synthetic OS inset',browserOSsafeArea:0,fixtureBanner:{present:!!document.querySelector('.fixture-label'),...(document.querySelector('.fixture-label') ? bounds(document.querySelector('.fixture-label')) : {height:0})},viewport:{width:innerWidth,height:innerHeight},navigation:bounds(document.querySelector('.zl-navigation')),sidebar:bounds(document.querySelector('.zl-navigation')),title:bounds(heading('Living Space')),announcementsHeading:bounds(heading('Pinned announcements')),pageHeading:bounds(heading('Live page')),pageLink:bounds(document.querySelector('.zl-home .zl-row')),eventsHeading:bounds(heading('Events')),filesHeading:bounds(heading('Files')),tabs:[...document.querySelectorAll('.zl-navigation button')].map(el=>({label:el.textContent.trim(),...bounds(el)}))};
  });
}
const browser=await webkit.launch({headless:true});
try {
  for (const [viewport,width,height] of [['web-narrow',440,956],['web-wide',1194,834]]) for (const theme of ['light','dark']) {
    const page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'});
    await page.clock.setFixedTime(new Date('2026-10-16T12:00:00Z'));
    const errors=[]; page.on('pageerror',()=>errors.push('Fixture render error'));
    await page.route('**/*',async route=>{
      const url=new URL(route.request().url());
      if(url.origin!=='https://living-space-fixture.invalid') return route.abort();
      const name=url.pathname==='/'?'index.html':url.pathname.slice(1);
      if(!Object.hasOwn(assets,name)) return route.abort();
      await route.fulfill({status:200,contentType:name.endsWith('.css')?'text/css':name.endsWith('.js')?'text/javascript':'text/html',body:assets[name]});
    });
    const shot=async screen=>{if(!check) await page.screenshot({path:resolve(out,`${viewport}-${theme}-${screen}.png`)});};
    await load(page,theme); const homeMetrics=await metrics(page);
    if(!check) await writeFile(resolve(out,`${viewport}-${theme}-home-metrics.json`),JSON.stringify(homeMetrics,null,2));
    await shot('home');
    await page.getByRole('button',{name:'View announcements',exact:true}).click();
    await page.getByRole('dialog',{name:'Announcements',exact:true}).waitFor();await shot('announcements');
    await page.getByRole('button',{name:'Done',exact:true}).click();
    await page.getByRole('button',{name:'Notifications',exact:true}).click();await section(page,'Notifications');
    await page.getByRole('heading',{name:'Join request',exact:true}).waitFor();
    await page.getByText('Taylor',{exact:true}).waitFor();
    await page.getByRole('button',{name:'Approve',exact:true}).waitFor();
    await page.getByRole('button',{name:'Decline',exact:true}).waitFor();
    if(await page.getByText('No pending requests.',{exact:true}).count()) throw Error('Notifications capture must contain a pending join request');
    await shot('notifications');
    await page.getByRole('button',{name:'Spaces',exact:true}).click();
    await page.locator('.zl-scroll').getByRole('button',{name:'Living Space',exact:true}).click();
    await page.locator('.zl-row').filter({hasText:'Welcome to our Space'}).click();
    await page.getByRole('heading',{name:'Welcome to our Space',exact:true}).first().waitFor();await shot('page');
    await page.getByRole('button',{name:'Back to Home',exact:true}).click();
    await page.getByRole('button',{name:'Hackathon',exact:true}).click();await section(page,'Build together');await shot('hackathon');
    await section(page,'Find teammates');await shot('teams');
    await section(page,'Project');await shot('submission');
    await section(page,'Winners');await shot('results');
    await page.getByRole('button',{name:'Spaces',exact:true}).click();
    await page.locator('.zl-scroll').getByRole('button',{name:'Living Space',exact:true}).click();
    await page.getByRole('button',{name:'Memberships',exact:true}).click();
    await page.getByText('Manage memberships',{exact:true}).click();
    await page.getByRole('button',{name:'Create membership',exact:true}).waitFor();
    if(!await page.locator('.zh-detail details').evaluate(el=>el.open)) throw Error('Membership capture must show expanded admin controls');
    await section(page,'Memberships');await shot('tiers');
    if(errors.length) throw Error('Fixture rendering failed');
    await page.close();
  }
} finally {await browser.close();}
console.log(check?'Passed 36 offline fixture state/viewport/theme selector checks; no images captured.':'Saved 36 offline fixture screenshots and 4 real-DOM Home metrics files; no adapter/runtime/backend verification.');
