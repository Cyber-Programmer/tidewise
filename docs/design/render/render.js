// node render.js <srcDir> <outDir>
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const [src, out] = process.argv.slice(2);
const work = path.join(__dirname, 'work');
fs.mkdirSync(work, { recursive: true }); fs.mkdirSync(out, { recursive: true });
const fontDir = path.join(__dirname, 'node_modules/geist/dist/fonts');
const fontCss = ['geist-sans/Geist-Variable.woff2', 'geist-mono/GeistMono-Variable.woff2'].map((f, i) =>
  `@font-face{font-family:'${i ? 'Geist Mono' : 'Geist'}';src:url('file://${path.join(fontDir, f)}') format('woff2');font-weight:100 900;}`).join('');
fs.copyFileSync(path.join(__dirname, 'shim.js'), path.join(work, 'support.js'));
(async () => {
  const browser = await chromium.launch();
  for (const f of fs.readdirSync(src).filter(f => f.endsWith('.dc.html'))) {
    let html = fs.readFileSync(path.join(src, f), 'utf8');
    html = html.replace(/<link[^>]+fonts\.googleapis[^>]*>/g, '').replace('</head>', `<style>${fontCss}</style></head>`);
    fs.writeFileSync(path.join(work, f), html);
    const m = html.match(/"\$preview":\{"width":(\d+),"height":(\d+)\}/);
    const w = m ? +m[1] : 1360, h = m ? +m[2] : 960;
    const page = await browser.newPage({ viewport: { width: w, height: h } });
    const errs = [];
    page.on('pageerror', e => errs.push(e.message));
    await page.goto('file://' + path.join(work, f));
    await page.waitForSelector('body[data-rendered]', { timeout: 5000 }).catch(() => errs.push('not rendered'));
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(150);
    await page.screenshot({ path: path.join(out, f.replace('.dc.html', '.png')), fullPage: true });
    console.log(f, w + 'x' + h, errs.length ? 'ERR ' + errs.join(' | ') : 'ok');
    await page.close();
  }
  await browser.close();
})();
