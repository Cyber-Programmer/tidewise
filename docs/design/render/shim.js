// Minimal stand-in for the design canvas runtime: enough to render .dc.html artboards statically.
(function () {
  function lookup(path, ctx) {
    path = path.trim();
    if (path === 'true') return true;
    if (path === 'false') return false;
    if (/^-?\d+(\.\d+)?$/.test(path)) return Number(path);
    var parts = path.split('.');
    var v = ctx;
    for (var i = 0; i < parts.length; i++) { if (v == null) return undefined; v = v[parts[i]]; }
    return v;
  }
  var WHOLE = /^\s*\{\{([^}]+)\}\}\s*$/;
  function interp(s, ctx) {
    return s.replace(/\{\{([^}]+)\}\}/g, function (_, p) { var v = lookup(p, ctx); return v == null ? '' : String(v); });
  }
  function walk(src, ctx, out) {
    if (src.nodeType === 3) { out.appendChild(document.createTextNode(interp(src.nodeValue, ctx))); return; }
    if (src.nodeType !== 1) return;
    var tag = src.tagName.toLowerCase();
    if (tag === 'sc-for') {
      var list = lookup(src.getAttribute('list').replace(/[{}]/g, ''), ctx) || [];
      var as = src.getAttribute('as') || 'item';
      list.forEach(function (it, idx) {
        var c = Object.create(ctx); c[as] = it; c.$index = idx;
        src.childNodes.forEach(function (ch) { walk(ch, c, out); });
      });
      return;
    }
    if (tag === 'sc-if') {
      if (lookup(src.getAttribute('value').replace(/[{}]/g, ''), ctx)) src.childNodes.forEach(function (ch) { walk(ch, ctx, out); });
      return;
    }
    var el = src.namespaceURI === 'http://www.w3.org/2000/svg' ? document.createElementNS(src.namespaceURI, src.tagName) : document.createElement(src.tagName);
    Array.from(src.attributes).forEach(function (a) {
      var m = a.value.match(WHOLE);
      if (/^on[a-z]+$/.test(a.name)) {
        if (m) { var fn = lookup(m[1], ctx); if (typeof fn === 'function') el.addEventListener(a.name.slice(2), fn); }
        return;
      }
      if (a.name === 'checked') { var cv = m ? lookup(m[1], ctx) : true; if (cv) el.setAttribute('checked', ''); el.checked = !!cv; return; }
      if (a.name.indexOf('hint-') === 0) return;
      var val = m ? lookup(m[1], ctx) : interp(a.value, ctx);
      if (val === false || val == null) return;
      el.setAttribute(a.name, val === true ? '' : String(val));
    });
    var kids = src.content ? src.content.childNodes : src.childNodes;
    kids.forEach(function (ch) { walk(ch, ctx, el); });
    out.appendChild(el);
  }
  window.DCLogic = class { constructor(props) { this.props = props || {}; this.state = {}; }
    setState(s) { Object.assign(this.state, typeof s === 'function' ? s(this.state) : s); window.__dcRender(); }
    forceUpdate() { window.__dcRender(); } };
  document.addEventListener('DOMContentLoaded', function () {
    var host = document.querySelector('x-dc');
    var helmet = host.querySelector('helmet');
    if (helmet) { Array.from(helmet.children).forEach(function (n) { document.head.appendChild(n); }); helmet.remove(); }
    var tpl = document.createElement('div');
    while (host.firstChild) tpl.appendChild(host.firstChild);
    var script = document.querySelector('script[data-dc-script]');
    var props = {};
    try { var dp = JSON.parse(script.getAttribute('data-props') || '{}'); Object.keys(dp).forEach(function (k) { if (dp[k] && 'default' in dp[k]) props[k] = dp[k].default; }); } catch (e) {}
    var Comp = new Function(script.textContent + '\nreturn Component;')();
    var comp = new Comp(props);
    if (location.hash.length > 1) { try { Object.assign(comp.state, JSON.parse(decodeURIComponent(location.hash.slice(1)))); } catch (e) {} }
    window.__dcRender = function () {
      var vals = comp.renderVals() || {};
      host.innerHTML = '';
      tpl.childNodes.forEach(function (ch) { walk(ch, vals, host); });
    };
    window.__dcRender();
    window.__dcComp = comp;
    document.body.setAttribute('data-rendered', '1');
  });
})();
