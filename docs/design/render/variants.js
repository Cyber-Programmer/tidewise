// node variants.js <workDir> <outDir> — renders extra states of M1 screens
const { chromium } = require('playwright');
const path = require('path');
const [work, out] = process.argv.slice(2);
const variants = [
  ['Performance', 'details', { details: true }],
  ['Performance', 'memory', { metric: 'mem' }],
  ['Performance', 'memory-details', { metric: 'mem', details: true }],
  ['Performance', 'network', { metric: 'net' }],
  ['Performance', 'disk', { metric: 'disk' }],
  ['Processes', 'details', { details: true }],
  ['Processes', 'apps', { filter: 'apps' }],
  ['Processes', 'protected-selected', { selected: 'plasma' }],
];
(async () => {
  const browser = await chromium.launch();
  for (const [name, label, state] of variants) {
    const page = await browser.newPage({ viewport: { width: 1360, height: 960 } });
    await page.goto('file://' + path.join(work, name + '.dc.html') + '#' + encodeURIComponent(JSON.stringify(state)));
    await page.waitForSelector('body[data-rendered]');
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(150);
    await page.screenshot({ path: path.join(out, `${name}--${label}.png`), fullPage: true });
    console.log(name, label, 'ok');
    await page.close();
  }
  await browser.close();
})();
