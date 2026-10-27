// Performance screen. One `sample` call per second; paused while the window is hidden.
const HISTORY = 60, MB = 1e6, GB = 1e9;
const COLORS = { cpu: '#7AA7FF', mem: '#F2B84B', net: '#5DDCB0', disk: '#C3A2FF' };
const history = { cpu: [], mem: [], net: [], disk: [] };
let selected = 'cpu';
let last = null;

const $ = (id) => document.getElementById(id);

function push(series, v) { series.push(v); if (series.length > HISTORY) series.shift(); }
function rateScale(series, floor) { return Math.max(floor, ...series); }
function fmtRate(b) { return b >= MB ? [(b / MB).toFixed(1), 'MB/s'] : [(b / 1000).toFixed(0), 'KB/s']; }
function fmtBytes(b) { return b >= GB ? (b / GB).toFixed(1) + ' GB' : (b / MB).toFixed(0) + ' MB'; }
function mood(f) { return f < 0.25 ? 'Calm' : f < 0.7 ? 'Busy' : 'Very busy'; }

// Points for a line through values (0..1), right-aligned so a short history grows in from the right.
function points(values, w, h, pad, count) {
  const n = values.length;
  return values.map((v, i) => {
    const x = (count === HISTORY ? HISTORY - n + i : i) * w / ((count === HISTORY ? HISTORY : Math.max(n, 2)) - 1);
    const y = h - pad - Math.min(1, Math.max(0, v)) * (h - pad * 2);
    return x.toFixed(1) + ',' + y.toFixed(1);
  }).join(' ');
}

function metrics(s) {
  const memScale = Math.max(1, s.memory_total);
  const netScale = rateScale(history.net, MB), diskScale = rateScale(history.disk, MB);
  const tail = (a) => a.slice(-1)[0] || 0;
  const [nv, nu] = fmtRate(tail(history.net)), [dv, du] = fmtRate(tail(history.disk));
  const [na, nau] = fmtRate(netScale), [da, dau] = fmtRate(diskScale);
  return [
    { id: 'cpu', label: 'Processor', value: s.cpu_percent.toFixed(0), unit: '%', mood: mood(s.cpu_percent / 100), headline: 'How busy your processor is', axis: '100%', values: history.cpu.map((v) => v / 100) },
    { id: 'mem', label: 'Memory', value: (s.memory_used / GB).toFixed(1), unit: 'of ' + (memScale / GB).toFixed(0) + ' GB', mood: mood(s.memory_used / memScale), headline: 'How much memory apps use', axis: (memScale / GB).toFixed(0) + ' GB', values: history.mem.map((v) => v / memScale) },
    { id: 'net', label: 'Network', value: nv, unit: nu + ' down', mood: tail(history.net) > MB ? 'Downloading' : 'Quiet', headline: 'Data coming in', axis: na + ' ' + nau, values: history.net.map((v) => v / netScale) },
    { id: 'disk', label: 'Disk', value: dv, unit: du, mood: tail(history.disk) > 20 * MB ? 'Busy' : 'Quiet', headline: 'Reading and writing', axis: da + ' ' + dau, values: history.disk.map((v) => v / diskScale) },
  ];
}

function render() {
  if (!last) return;
  const list = metrics(last);
  const cards = $('cards');
  if (cards.children.length !== list.length) {
    cards.replaceChildren(...list.map((m) => {
      const b = document.createElement('button');
      b.type = 'button'; b.className = 'card'; b.setAttribute('role', 'tab'); b.dataset.id = m.id;
      b.addEventListener('click', () => { selected = m.id; render(); });
      return b;
    }));
  }
  list.forEach((m, i) => {
    const b = cards.children[i], color = COLORS[m.id], on = m.id === selected;
    b.setAttribute('aria-selected', on);
    b.style.borderColor = on ? color : '';
    b.innerHTML = `<span class="top"><span>${m.label}</span><span style="color:${color}">${m.mood}</span></span>` +
      `<span class="row"><span class="val">${m.value} <small>${m.unit}</small></span>` +
      `<svg viewBox="0 0 120 32" preserveAspectRatio="none" aria-hidden="true"><polyline fill="none" stroke="${color}" stroke-width="2" stroke-linejoin="round" points="${points(m.values.slice(-30), 120, 32, 2, 30)}"></polyline></svg></span>`;
  });
  const cur = list.find((m) => m.id === selected), color = COLORS[cur.id];
  $('panel').setAttribute('aria-label', cur.label);
  $('p-label').textContent = cur.label;
  $('p-headline').textContent = cur.headline;
  $('p-value').textContent = cur.value; $('p-value').style.color = color;
  $('p-unit').textContent = cur.unit;
  $('p-axis').textContent = cur.axis;
  const line = points(cur.values, 600, 200, 5, HISTORY);
  $('line').setAttribute('points', line); $('line').setAttribute('stroke', color);
  const x0 = (HISTORY - cur.values.length) * 600 / (HISTORY - 1);
  $('area').setAttribute('points', line ? `${x0.toFixed(1)},195 ${line} 600,195` : '');
  $('area').setAttribute('fill', color);

  $('proc-title').textContent = `All processes (${last.processes.length})`;
  $('procs').replaceChildren(...last.processes.map((p) => {
    const r = document.createElement('div');
    r.className = 'proc';
    r.innerHTML = '<span class="name"></span><span class="num pid"></span><span class="num cpu"></span><span class="num mem"></span>';
    r.children[0].textContent = p.name;
    r.children[1].textContent = p.pid;
    r.children[2].textContent = p.cpu_percent.toFixed(1) + '%';
    r.children[3].textContent = fmtBytes(p.memory_bytes);
    return r;
  }));
}

async function tick() {
  if (document.hidden) return;
  try {
    const s = await window.__TAURI__.core.invoke('sample');
    push(history.cpu, s.cpu_percent);
    push(history.mem, s.memory_used);
    push(history.net, s.net_down_per_sec || 0);
    push(history.disk, (s.disk_read_per_sec || 0) + (s.disk_write_per_sec || 0));
    last = s;
    // Keep the scroll position of the process list across redraws.
    const procs = $('procs'), top = procs.scrollTop;
    render();
    procs.scrollTop = top;
  } catch (e) {
    console.error('sample failed', e);
  }
}

tick();
setInterval(tick, 1000);
