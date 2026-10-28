# Re-rendering the reference PNGs

Only needed when the design canvas changes and `../screens/*.dc.html` are re-exported.

```
cd docs/design/render
npm init -y && npm i playwright geist@1.7.0 && npx playwright install chromium
node render.js ../screens ../png          # every artboard
cp shim.js work/support.js && node variants.js "$PWD/work" "$PWD/../png"   # extra states
```

`shim.js` is a small stand-in for the canvas runtime (holes, sc-for, sc-if, click handlers); it's
enough for static renders, not a full implementation. Add new states to the list in `variants.js`.
