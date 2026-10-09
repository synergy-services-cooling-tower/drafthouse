// tab-timing.mjs - issue #151: the worst main-thread block and the worst frame of every tab switch,
// measured in a REAL browser on the built web plane (cockpit/pkg), the same way in every engine.
//
//   node cockpit/tools/tab-timing.mjs --browser safari  --runs 3 --out <file.json>   # the owner's surface
//   node cockpit/tools/tab-timing.mjs --browser firefox --runs 3 --out <file.json>   # headed Gecko
//   node cockpit/tools/tab-timing.mjs --browser chromium --binary <chrome> --runs 3 --out <file.json>
//
// Before it: `bash cockpit/tools/build-web.sh release` (the plane it serves). Options: --only Size,Curves
// (rows by name), --label <text>, --settle <ms> (default 3000), --budget <ms> (given overrides it; with
// none, the budget is DERIVED from the run - the worst main-thread block the idle windows show while the
// app does nothing is this machine's and browser's own floor, rounded up to the next 10 ms - and the
// derivation is printed with the summary), --viewport 1440x900, --binary <path> (the browser executable).
//
// Safari: `safaridriver` ships with macOS, but it refuses a session until remote automation is allowed:
// Safari > Settings > Advanced > "Show features for web developers", then Develop > "Allow Remote
// Automation" (once per machine; `safaridriver --enable` asks for an admin password instead). Without it
// this tool stops with the driver's own refusal and exit 4 - it never substitutes another engine.
// Firefox: the Playwright cache's Firefox build (~/Library/Caches/ms-playwright/firefox-*), driven over
// WebDriver BiDi, in a normal window. Chromium: any Chrome/Chromium binary, over the DevTools protocol.
//
// What it measures, per row, in a FRESH page load (every heavy result is computed again, as on a first
// visit), after the app reports `#viz-root[data-load=ready]`:
//   warmup  the row's setup commands (Compare: two saved variants opened) and the home screen, settled;
//   idle    3 s with no input: the page's own frame cadence on this machine (the floor);
//   cold    the switch, through the app's own command channel (`window.__viz.dispatch`, the call the nav
//           makes), recorded until the target screen reports nothing left to compute
//           (`data-screen-busy=0`) and stays so for the settle window;
//   back    to the home screen;
//   warm    the same switch again (nothing it depends on changed).
//
// The numbers, all from an in-page recorder that needs nothing engine-specific:
//   worst block  the longest stretch in which the page's main thread ran nothing else: a zero-delay
//                timer re-arms itself, and the largest gap between two of its callbacks is the longest
//                task (plus the timer's own <= 4 ms clamp). This is "worst long task" here. It works in
//                Safari and Firefox, which do not implement the Long Tasks API; where the browser has
//                that API (Chromium) its own longest `longtask` entry is recorded beside it.
//   worst frame  the largest gap between consecutive requestAnimationFrame callbacks - what the user
//                sees freeze. On a 60 Hz display the floor is ~16.7 ms.
//   app frames   how many frames the app itself drew in the window (its `data-frames` counter).
//   settled      when the target screen last turned not-busy, from the switch.
// Row "Compare open": the comparison's file open (two saved variants), from the Instrument. Row "Motion":
// the motion toggle (`motion`), twice. Row "Still": the still/motion control's own effect on the app
// (`freeze:1`, then `freeze:0`).
//
// Machine and load: the machine (model, chip, cores, memory, macOS) once; before and after every row the
// 1/5/15-minute load average and the number of rustc/cargo processes running (the host is shared).
// The served wasm must be byte-identical to cockpit/pkg on disk (sha256 + byte compare), or it refuses.
// Output: one JSON with every row's numbers, and the summary table on stdout.
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

if (typeof WebSocket === 'undefined') {
  console.error('tab-timing: no global WebSocket in this node (use node 22+)');
  process.exit(2);
}

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i === -1 ? fallback : args[i + 1];
};
const BROWSER = opt('--browser', null);
if (!['safari', 'firefox', 'chromium'].includes(BROWSER)) {
  console.error('usage: node cockpit/tools/tab-timing.mjs --browser safari|firefox|chromium [--runs 3] [--out file.json] [--only Size,Curves] [--binary path]');
  process.exit(2);
}
const LABEL = opt('--label', BROWSER);
const RUNS = Number(opt('--runs', '3'));
const OUT = resolve(opt('--out', join(tmpdir(), `tab-timing-${LABEL}.json`)));
const [VW, VH] = opt('--viewport', '1440x900').split('x').map(Number);
const ONLY = opt('--only', null);
const SETTLE_MS = Number(opt('--settle', '3000'));
const IDLE_MS = 3000;
const SWITCH_TIMEOUT_MS = Number(opt('--switch-timeout', '600000'));
const BUDGET_OPT = opt('--budget', null); // null = derive it from this run (below)
const OWNER_SURFACE = BROWSER === 'safari';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const step = (what) => process.stderr.write(`tab-timing: ${what}\n`);
const sh = (cmd, argv) => {
  try {
    return execFileSync(cmd, argv, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
  } catch {
    return null;
  }
};
const hostLoad = () => {
  const up = sh('uptime', []) || '';
  const m = up.match(/load averages?:\s*([\d.]+)[ ,]+([\d.]+)[ ,]+([\d.]+)/);
  const procs = (sh('ps', ['-A', '-o', 'comm=']) || '').split('\n');
  return {
    at: new Date().toISOString(),
    load_1_5_15: m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null,
    rustc_processes: procs.filter((p) => /(^|\/)rustc$/.test(p.trim())).length,
    cargo_processes: procs.filter((p) => /(^|\/)cargo$/.test(p.trim())).length,
  };
};
const freePort = () =>
  new Promise((ok) => {
    const s = createServer();
    s.listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => ok(port));
    });
  });
// a path is recorded relative to the browser cache or the home directory, never absolute
const shortPath = (p) => (p ? p.replace(homedir(), '~') : p);

// ------------------------------------------------------------------------------- the rows
const variantText = (f) => readFileSync(join(root, 'cockpit', 'assets', 'variants', f), 'utf8');
const ROWS = [
  { tab: 'Instrument', slug: 'instrument', home: 'screen:water', go: ['screen:instrument'] },
  { tab: 'Size', slug: 'size', go: ['screen:size'] },
  { tab: 'Curves', slug: 'curves', go: ['screen:curves'] },
  { tab: 'Water', slug: 'water', go: ['screen:water'] },
  { tab: 'Rate', slug: 'rate', go: ['screen:rate'] },
  {
    tab: 'Compare',
    slug: 'compare',
    pre: ['a-base.drafthouse', 'b-fan-faster.drafthouse'].map((f) => `compare:open:${f}:${variantText(f)}`),
    go: ['screen:compare'],
  },
  // the file open itself, in the order the page's "compare files" picker sends it: the screen, then
  // one `compare:open` per picked file
  {
    tab: 'Compare open',
    slug: 'compare',
    go: ['screen:compare', ...['a-base.drafthouse', 'b-fan-faster.drafthouse'].map((f) => `compare:open:${f}:${variantText(f)}`)],
    back: ['compare:clear', 'screen:instrument'],
  },
  { tab: 'Crossflow', slug: 'crossflow', go: ['tower:crossflow', 'screen:instrument'], back: ['tower:counterflow', 'screen:instrument'] },
  { tab: 'Report', slug: 'report', go: ['page:4', 'screen:report'] },
  { tab: 'Motion', slug: 'instrument', go: ['motion'], back: ['motion'] },
  { tab: 'Still', slug: 'instrument', go: ['freeze:1'], back: ['freeze:0'] },
].filter((t) => !ONLY || ONLY.split(',').includes(t.tab));

// ------------------------------------------------------------------------------- the in-page recorder
// Installed once per page load. Everything is measured inside the page; the driver only starts a window
// and polls for its result, so the same code runs in every browser.
const INSTALL = `(() => {
  if (window.__tt) return 'installed';
  const root = document.getElementById('viz-root');
  const rec = { frames: [], beats: [], lt: [], loaf: [], marks: [], errors: [], result: null };
  window.__tt = rec;
  // the page's own errors (a panic reaches console.error through the panic hook): kept, so a row the
  // app stopped answering in says why instead of only timing out
  const err0 = console.error.bind(console);
  console.error = (...a) => { rec.errors.push(a.map(String).join(' ').slice(0, 2000)); err0(...a); };
  window.addEventListener('error', (e) => rec.errors.push(String(e.message).slice(0, 2000)));
  const tick = (t) => { rec.frames.push(t); requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  const beat = () => { rec.beats.push(performance.now()); setTimeout(beat, 0); };
  setTimeout(beat, 0);
  const types = (window.PerformanceObserver && PerformanceObserver.supportedEntryTypes) || [];
  rec.lt_ok = types.includes('longtask');
  rec.loaf_ok = types.includes('long-animation-frame');
  if (rec.lt_ok) new PerformanceObserver((l) => { for (const e of l.getEntries()) rec.lt.push([e.startTime, e.duration]); }).observe({ type: 'longtask' });
  if (rec.loaf_ok) new PerformanceObserver((l) => { for (const e of l.getEntries()) rec.loaf.push([e.startTime, e.duration]); }).observe({ type: 'long-animation-frame' });
  new MutationObserver(() => rec.marks.push([performance.now(), root.dataset.screen || '', root.dataset.screenBusy || '']))
    .observe(root, { attributes: true, attributeFilter: ['data-screen', 'data-screen-busy'] });
  const r1 = (x) => (x == null ? null : Math.round(x * 10) / 10);
  const pct = (xs, p) => { if (!xs.length) return null; const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))]; };
  const gapsAfter = (ts, t0) => { const g = []; let prev = t0; for (const t of ts) { if (t <= t0) { prev = t; continue; } g.push(t - prev); prev = t; } return g; };
  const reset = () => { rec.result = null; rec.frames.length = 0; rec.beats.length = 0; rec.lt.length = 0; rec.loaf.length = 0; rec.marks.length = 0; };
  const stats = (t0, f0, slug, timedOut) => {
    const now = performance.now();
    // the gap still open at the end of the window counts too
    const fg = gapsAfter(rec.frames.concat([now]), t0);
    const bg = gapsAfter(rec.beats.concat([now]), t0);
    let readyAt = null;
    for (const [t, s, b] of rec.marks) { if (s === slug && b === '0') { if (readyAt === null) readyAt = t; } else readyAt = null; }
    const lts = rec.lt.filter(([s, d]) => s + d > t0).map(([, d]) => d);
    const loafs = rec.loaf.filter(([s, d]) => s + d > t0).map(([, d]) => d);
    return {
      timed_out: timedOut,
      window_ms: r1(now - t0),
      app_frames: Number(root.dataset.frames || 0) - f0,
      frames: fg.length,
      worst_frame_ms: r1(fg.length ? Math.max(...fg) : null),
      p50_frame_ms: r1(pct(fg, 50)),
      frames_over_50ms: fg.filter((g) => g > 50).length,
      worst_block_ms: r1(bg.length ? Math.max(...bg) : null),
      blocks_over_50ms: bg.filter((g) => g > 50).map(r1),
      longtask_api: rec.lt_ok,
      longest_longtask_ms: rec.lt_ok ? r1(lts.length ? Math.max(...lts) : 0) : null,
      longest_loaf_ms: rec.loaf_ok ? r1(loafs.length ? Math.max(...loafs) : 0) : null,
      settled_after_ms: slug && readyAt !== null ? r1(Math.max(0, readyAt - t0)) : null,
      visibility: document.visibilityState,
      focused: document.hasFocus(),
      page_errors: rec.errors.slice(-5),
    };
  };
  // dispatch the commands, then hold until the screen \`slug\` is up and not busy for \`settle\` ms
  rec.run = (cmds, slug, settle, timeout) => {
    reset();
    const f0 = Number(root.dataset.frames || 0);
    const t0 = performance.now();
    // the state the window starts in, so a command that leaves the screen where it is (Motion, Still)
    // reads as settled from the start rather than never
    rec.marks.push([t0, root.dataset.screen || '', root.dataset.screenBusy || '']);
    for (const c of cmds) window.__viz.dispatch(c);
    let since = null;
    const check = () => {
      const now = performance.now();
      const done = (root.dataset.screen || '') === slug && root.dataset.screenBusy === '0';
      if (done) { if (since === null) since = now; } else since = null;
      const out = now - t0 > timeout;
      if ((since !== null && now - since >= settle) || out) { rec.result = stats(t0, f0, slug, out); return; }
      setTimeout(check, 100);
    };
    setTimeout(check, 100);
    return 'started';
  };
  rec.idle = (ms) => {
    reset();
    const f0 = Number(root.dataset.frames || 0);
    const t0 = performance.now();
    setTimeout(() => { rec.result = stats(t0, f0, null, false); }, ms);
    return 'started';
  };
  return 'installed';
})()`;
const MARKERS = `JSON.stringify((() => { const r = document.getElementById('viz-root');
  return r ? { load: r.dataset.load || '', screen: r.dataset.screen || '' } : { load: '', screen: '' }; })())`;

// ------------------------------------------------------------------------------- the drivers
// Each driver: { name, version, user_agent, headless, binary, navigate(url), eval(expr) -> value, stop() }.
// `eval` takes an expression and returns its value; every expression here is synchronous.

async function waitFor(fn, ms, what) {
  const until = Date.now() + ms;
  for (;;) {
    const v = await fn().catch(() => null);
    if (v) return v;
    if (Date.now() > until) throw new Error(`timed out waiting for ${what}`);
    await sleep(150);
  }
}

function rpc(ws) {
  let seq = 0;
  const pending = new Map();
  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  });
  // every call is answered or refused within a minute: a browser that stops answering (a page
  // navigation the driver never reports complete) fails the row instead of hanging the run
  return (method, params = {}) =>
    new Promise((ok, bad) => {
      const id = ++seq;
      const timer = setTimeout(() => {
        pending.delete(id);
        bad(new Error(`${method}: no answer in 60 s`));
      }, 60000);
      pending.set(id, (msg) => {
        clearTimeout(timer);
        ok(msg);
      });
      ws.send(JSON.stringify({ id, method, params }));
    });
}
const connect = async (url) => {
  const ws = new WebSocket(url);
  await new Promise((ok, bad) => {
    ws.addEventListener('open', ok, { once: true });
    ws.addEventListener('error', () => bad(new Error(`could not connect to ${url}`)), { once: true });
  });
  return ws;
};

async function safari() {
  const bin = opt('--binary', '/usr/bin/safaridriver');
  const port = await freePort();
  const proc = spawn(bin, ['-p', String(port)], { stdio: 'ignore' });
  const base = `http://127.0.0.1:${port}`;
  const call = async (method, path, body) => {
    const r = await fetch(base + path, { method, headers: { 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined });
    return (await r.json()).value;
  };
  await waitFor(async () => (await call('GET', '/status'))?.ready, 15000, 'safaridriver');
  const made = await call('POST', '/session', { capabilities: { alwaysMatch: { browserName: 'safari' } } });
  if (!made || made.error) {
    proc.kill('SIGKILL');
    const why = made ? `${made.error}: ${made.message}` : 'no reply';
    const e = new Error(`Safari refused the session (${why})`);
    e.gate = true;
    throw e;
  }
  const id = made.sessionId;
  await call('POST', `/session/${id}/window/rect`, { width: VW, height: VH + 80 });
  return {
    name: 'Safari',
    version: made.capabilities?.browserVersion ?? null,
    headless: false,
    binary: bin,
    navigate: (url) => call('POST', `/session/${id}/url`, { url }),
    eval: async (expr) => {
      const v = await call('POST', `/session/${id}/execute/sync`, { script: `return (${expr});`, args: [] });
      if (v && v.error) throw new Error(`${v.error}: ${v.message}`);
      return v;
    },
    stop: async () => {
      await call('DELETE', `/session/${id}`).catch(() => null);
      proc.kill('SIGKILL');
    },
  };
}

async function firefox(work) {
  const bin = opt('--binary', null) || (() => {
    const cache = join(homedir(), 'Library', 'Caches', 'ms-playwright');
    const dir = existsSync(cache) && readdirSync(cache).filter((d) => d.startsWith('firefox-')).sort().pop();
    if (!dir) return null;
    for (const app of ['Nightly.app', 'Firefox.app']) {
      const p = join(cache, dir, 'firefox', app, 'Contents', 'MacOS', 'firefox');
      if (existsSync(p)) return p;
    }
    return null;
  })();
  if (!bin || !existsSync(bin)) throw new Error('no Firefox binary (pass --binary)');
  const profile = join(work, 'firefox-profile');
  mkdirSync(profile, { recursive: true });
  writeFileSync(join(profile, 'user.js'), [
    'user_pref("browser.shell.checkDefaultBrowser", false);',
    'user_pref("browser.startup.homepage_override.mstone", "ignore");',
    'user_pref("datareporting.policy.dataSubmissionEnabled", false);',
    'user_pref("browser.cache.disk.enable", false);',
  ].join('\n'));
  const port = await freePort();
  const proc = spawn(bin, ['--remote-debugging-port', String(port), '-profile', profile, '-no-remote', '-new-instance', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  let err = '';
  proc.stderr.on('data', (d) => (err += d));
  const wsUrl = await waitFor(async () => (err.match(/WebDriver BiDi listening on (ws:\/\/\S+)/) || [])[1], 60000, 'Firefox');
  step(`Firefox is listening at ${wsUrl}`);
  const ws = await connect(`${wsUrl}/session`);
  const send = rpc(ws);
  const made = await send('session.new', { capabilities: {} });
  step(`session: ${made.type}`);
  if (made.type !== 'success') throw new Error(`Firefox refused the session: ${made.message}`);
  const caps = made.result.capabilities;
  const tree = await send('browsingContext.getTree', {});
  const context = tree.result.contexts[0].context;
  await send('browsingContext.setViewport', { context, viewport: { width: VW, height: VH } });
  // (`browsingContext.activate` waits for OS focus, which a session without a desk owner never grants;
  // each window records `document.visibilityState` and `document.hasFocus()` instead)
  return {
    name: 'Firefox',
    version: caps.browserVersion,
    user_agent: caps.userAgent,
    headless: caps['moz:headless'],
    binary: bin,
    navigate: (url) => send('browsingContext.navigate', { context, url, wait: 'complete' }),
    eval: async (expr) => {
      const r = await send('script.evaluate', { expression: expr, target: { context }, awaitPromise: false });
      if (r.type !== 'success') throw new Error(r.message || 'evaluate failed');
      if (r.result.type === 'exception') throw new Error(r.result.exceptionDetails?.text || 'the page threw');
      return r.result.result.value;
    },
    stop: async () => {
      await send('session.end', {}).catch(() => null);
      proc.kill('SIGKILL');
    },
  };
}

async function chromium(work) {
  const bin = opt('--binary', null);
  if (!bin || !existsSync(bin) || statSync(bin).size < 4096) {
    throw new Error('--browser chromium needs --binary <a working Chrome or Chromium executable>');
  }
  const profile = join(work, 'chromium-profile');
  mkdirSync(profile, { recursive: true });
  const proc = spawn(bin, [
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check',
    '--disable-extensions', `--window-size=${VW},${VH}`, 'about:blank',
  ], { stdio: 'ignore' });
  const http = await waitFor(async () => {
    const port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0];
    const r = await fetch(`http://127.0.0.1:${port}/json/version`);
    return r.ok ? `http://127.0.0.1:${port}` : null;
  }, 60000, 'Chromium');
  const version = await (await fetch(`${http}/json/version`)).json();
  const page = (await (await fetch(`${http}/json/list`)).json()).find((t) => t.type === 'page');
  const ws = await connect(page.webSocketDebuggerUrl);
  const send = rpc(ws);
  await send('Page.enable');
  await send('Runtime.enable');
  const evaluate = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true });
    if (r.error) throw new Error(r.error.message);
    if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.text || 'the page threw');
    return r.result.result.value;
  };
  return {
    name: version.Browser,
    version: version.Browser,
    user_agent: version['User-Agent'],
    headless: /Headless/.test(version.Browser),
    binary: bin,
    navigate: async (url) => {
      await send('Page.navigate', { url });
      await waitFor(async () => (await evaluate('document.readyState')) === 'complete', 60000, 'the page');
    },
    eval: evaluate,
    stop: async () => proc.kill('SIGKILL'),
  };
}

// ------------------------------------------------------------------------------- serve + verify the plane
const wasmPath = join(root, 'cockpit', 'pkg', 'drafthouse_cockpit_bg.wasm');
if (!existsSync(wasmPath)) {
  console.error('tab-timing: cockpit/pkg is not built - run: bash cockpit/tools/build-web.sh release');
  process.exit(3);
}
const work = mkdtempSync(join(tmpdir(), 'tab-timing-'));
const serveLog = join(work, 'serve.log');
const server = spawn('bash', ['-c', 'exec bash "$0" > "$1" 2>&1', join(root, 'cockpit', 'tools', 'serve.sh'), serveLog], {
  cwd: root,
  detached: true,
  stdio: 'ignore',
});
let browser = null;
const cleanup = () => {
  try { process.kill(-server.pid, 'SIGKILL'); } catch { /* gone */ }
};
process.on('exit', cleanup);
process.on('SIGINT', () => process.exit(130));

const PORT = await waitFor(async () => Number((existsSync(serveLog) && readFileSync(serveLog, 'utf8').match(/^PORT=(\d+)$/m) || [])[1]) || null, 10000, 'serve.sh');
const served = await waitFor(async () => {
  const r = await fetch(`http://127.0.0.1:${PORT}/pkg/drafthouse_cockpit_bg.wasm`);
  return r.ok ? Buffer.from(await r.arrayBuffer()) : null;
}, 10000, 'the served wasm');
const disk = readFileSync(wasmPath);
const sha = (b) => createHash('sha256').update(b).digest('hex');
const plane = { served_bytes: served.length, disk_bytes: disk.length, served_sha256: sha(served), disk_sha256: sha(disk), identical: served.equals(disk) };
console.log(`plane: served ${plane.served_bytes} B sha256 ${plane.served_sha256} (identical to the disk build: ${plane.identical})`);
if (!plane.identical) {
  console.error("tab-timing: REFUSED - the served wasm is not this worktree's build");
  process.exit(3);
}

try {
  browser = await { safari, firefox, chromium }[BROWSER](work);
} catch (e) {
  console.error(`tab-timing: ${e.message}`);
  if (e.gate) {
    console.error('tab-timing: Safari needs Develop > "Allow Remote Automation" (Safari > Settings > Advanced > "Show features for web developers" first). Nothing was measured.');
    process.exit(4);
  }
  process.exit(3);
}
console.log(`browser: ${browser.name} ${browser.version ?? ''} (headless: ${browser.headless})`);
console.log(OWNER_SURFACE ? 'surface: Safari on macOS - the owner\'s surface' : `surface: ${browser.name} - NOT the owner's surface (Mac Safari); these numbers do not satisfy the Safari criterion`);

const evalJson = async (expr) => JSON.parse(await browser.eval(expr));
async function window_(start) {
  await browser.eval(start);
  for (;;) {
    await sleep(250);
    const r = await browser.eval('JSON.stringify(window.__tt.result)');
    if (r && r !== 'null') return JSON.parse(r);
  }
}
const run = (cmds, slug) => window_(`window.__tt.run(${JSON.stringify(cmds)}, ${JSON.stringify(slug)}, ${SETTLE_MS}, ${SWITCH_TIMEOUT_MS})`);

async function runRow(def, n) {
  step(`run ${n} ${def.tab}: loading the page`);
  await browser.navigate(`http://127.0.0.1:${PORT}/index.html?b=${Date.now()}`);
  step(`run ${n} ${def.tab}: waiting for the app`);
  const started = Date.now();
  let load = '';
  while (Date.now() - started < 300000) {
    try { load = (await evalJson(MARKERS)).load; } catch { /* navigating */ }
    if (load === 'ready') break;
    await sleep(250);
  }
  if (load !== 'ready') {
    let why = '';
    try { why = await browser.eval(`String(document.getElementById('viz-root')?.dataset.loadError || document.body?.innerText?.slice(0, 300) || '')`); } catch { /* gone */ }
    return { tab: def.tab, run: n, error: `the app never became ready (data-load=${load}) ${why}`.trim() };
  }
  await browser.eval(INSTALL);
  const page = await evalJson(`JSON.stringify((() => { let gl = 'no webgl2'; try { const g = document.createElement('canvas').getContext('webgl2');
    const d = g && g.getExtension('WEBGL_debug_renderer_info'); gl = g ? (d ? g.getParameter(d.UNMASKED_RENDERER_WEBGL) : g.getParameter(g.RENDERER)) : gl; } catch (e) { gl = 'probe failed'; }
    return { renderer: gl, inner: [innerWidth, innerHeight], dpr: devicePixelRatio, ua: navigator.userAgent }; })())`);
  const home = def.home || 'screen:instrument';
  const homeSlug = home.split(':')[1];
  const warmup = await run([...(def.pre || []), home], homeSlug);
  const idle = await window_(`window.__tt.idle(${IDLE_MS})`);
  const loadBefore = hostLoad();
  const cold = await run(def.go, def.slug);
  const back = await run(def.back || [home], homeSlug);
  const warm = await run(def.go, def.slug);
  const loadAfter = hostLoad();
  return { tab: def.tab, run: n, page, load_before: loadBefore, load_after: loadAfter, ready_after_ms: Date.now() - started, warmup, idle, cold, back, warm };
}

const machine = {
  model: sh('sysctl', ['-n', 'hw.model']),
  chip: sh('sysctl', ['-n', 'machdep.cpu.brand_string']),
  cores: Number(sh('sysctl', ['-n', 'hw.ncpu'])),
  perf_cores: Number(sh('sysctl', ['-n', 'hw.perflevel0.physicalcpu'])),
  efficiency_cores: Number(sh('sysctl', ['-n', 'hw.perflevel1.physicalcpu'])),
  memory_gb: Number(sh('sysctl', ['-n', 'hw.memsize'])) / 2 ** 30,
  macos: `${sh('sw_vers', ['-productVersion'])} (${sh('sw_vers', ['-buildVersion'])})`,
};
console.log(`machine: ${JSON.stringify(machine)}`);
const head = sh('git', ['-C', root, 'rev-parse', 'HEAD']);
const dirty = sh('git', ['-C', root, 'status', '--porcelain', '--', 'cockpit/src', 'cockpit/Cargo.toml', 'rust']);

const results = [];
for (let n = 1; n <= RUNS; n++) {
  for (const def of ROWS) {
    let r;
    try {
      r = await runRow(def, n);
    } catch (e) {
      r = { tab: def.tab, run: n, error: e.message };
    }
    results.push(r);
    if (r.error) {
      console.log(`run ${n} ${def.tab.padEnd(10)} ERROR ${r.error}`);
      continue;
    }
    console.log(
      `run ${n} ${def.tab.padEnd(10)} cold: block ${String(r.cold.worst_block_ms).padStart(7)} ms, frame ${String(r.cold.worst_frame_ms).padStart(7)} ms, settled ${r.cold.settled_after_ms} ms${r.cold.timed_out ? ' TIMED OUT' : ''} ` +
        `| warm: block ${r.warm.worst_block_ms} ms | idle: frame p50 ${r.idle.p50_frame_ms} worst ${r.idle.worst_frame_ms} ms ` +
        `| load ${r.load_before.load_1_5_15?.join('/')} rustc ${r.load_before.rustc_processes} | ${r.cold.visibility}${r.cold.focused ? '' : ', unfocused'}`,
    );
  }
}
await browser.stop();

const max = (xs) => (xs.length ? Math.max(...xs) : null);
const span = (xs) => (xs.length ? `${Math.min(...xs)}–${Math.max(...xs)}` : '—');
// The budget is derived from this run, not chosen (unless --budget overrides it): the worst
// main-thread block the page itself shows in the idle windows - no input, the app doing nothing - is
// this machine's and browser's own floor, so no switch can be held below it. Round it up to the next
// 10 ms. The same derivation taken from a Safari run is the restated budget of issue #151 AC-2.
const idleWorst = results.filter((r) => !r.error).map((r) => r.idle.worst_block_ms).filter((x) => x != null);
const BUDGET_MS = BUDGET_OPT !== null ? Number(BUDGET_OPT) : idleWorst.length ? Math.ceil(Math.max(...idleWorst) / 10) * 10 : 100;
const BUDGET_HOW =
  BUDGET_OPT !== null
    ? 'given with --budget'
    : idleWorst.length
      ? `derived: the worst main-thread block this run's idle windows show while the app does nothing (${idleWorst.length} windows, worst ${Math.max(...idleWorst)} ms), rounded up to the next 10 ms`
      : 'default: this run carried no idle window to derive from';
const summary = ROWS.map((def) => {
  const rows = results.filter((r) => r.tab === def.tab && !r.error);
  const col = (w, k) => rows.map((r) => r[w][k]).filter((x) => x != null);
  return {
    tab: def.tab,
    runs: rows.length,
    cold_worst_block_ms: max(col('cold', 'worst_block_ms')),
    cold_worst_frame_ms: max(col('cold', 'worst_frame_ms')),
    cold_longest_longtask_ms: max(col('cold', 'longest_longtask_ms')),
    cold_settled_ms: max(col('cold', 'settled_after_ms')),
    warm_worst_block_ms: max(col('warm', 'worst_block_ms')),
    warm_worst_frame_ms: max(col('warm', 'worst_frame_ms')),
    idle_p50_frame_ms: span(col('idle', 'p50_frame_ms')),
    idle_worst_block_ms: max(col('idle', 'worst_block_ms')),
    load_1min: span(rows.map((r) => r.load_before.load_1_5_15?.[0]).filter((x) => x != null)),
    over_budget: max(col('cold', 'worst_block_ms').concat(col('warm', 'worst_block_ms'))) > BUDGET_MS,
    timed_out: rows.some((r) => r.cold.timed_out || r.warm.timed_out),
    errors: results.filter((r) => r.tab === def.tab && r.error).length,
  };
});

console.log('');
console.log(`${browser.name} ${browser.version ?? ''}, max of ${RUNS} runs, ms. Budget ${BUDGET_MS} ms on the worst block (${BUDGET_HOW}).${OWNER_SURFACE ? '' : " NOT the owner's surface (Mac Safari)."}`);
console.log('| row | cold: worst block | cold: worst frame | cold: settled | warm: worst block | warm: worst frame | idle: frame p50 | idle: worst block | load (1-min) | over budget |');
console.log('|---|---:|---:|---:|---:|---:|---:|---:|---:|---|');
for (const s of summary) {
  console.log(`| ${s.tab} | ${s.cold_worst_block_ms} | ${s.cold_worst_frame_ms} | ${s.cold_settled_ms} | ${s.warm_worst_block_ms} | ${s.warm_worst_frame_ms} | ${s.idle_p50_frame_ms} | ${s.idle_worst_block_ms} | ${s.load_1min} | ${s.over_budget ? '**yes**' : 'no'} |`);
}

const payload = {
  issue: 151,
  label: LABEL,
  command: `node cockpit/tools/tab-timing.mjs ${args.join(' ')}`,
  measured_at: new Date().toISOString(),
  worktree_head: head,
  worktree_dirty_paths: dirty ? dirty.split('\n') : [],
  surface: OWNER_SURFACE ? 'Safari on macOS (the owner\'s surface)' : `${browser.name} - NOT the owner's surface (Mac Safari)`,
  machine,
  browser: { driver: BROWSER, name: browser.name, version: browser.version, user_agent: browser.user_agent ?? results.find((r) => r.page)?.page.ua ?? null, headless: browser.headless, binary: shortPath(browser.binary), renderer: results.find((r) => r.page)?.page.renderer ?? null },
  served: { how: `cockpit/tools/serve.sh (python3 -m http.server) on 127.0.0.1:${PORT}`, ...plane },
  viewport: { requested: `${VW}x${VH}`, inner: results.find((r) => r.page)?.page.inner ?? null, dpr: results.find((r) => r.page)?.page.dpr ?? null },
  method: {
    worst_block: 'the largest gap between two callbacks of a self-re-arming zero-delay timer: the longest stretch the main thread ran nothing else (the longest task, plus the timer clamp of <= 4 ms)',
    worst_frame: 'the largest gap between consecutive requestAnimationFrame callbacks',
    window: 'from the dispatch of the switch until the target screen reports data-screen-busy=0, held for the settle window',
    settle_ms: SETTLE_MS,
    idle_ms: IDLE_MS,
    budget_ms: BUDGET_MS,
    budget_how: BUDGET_HOW,
    cold: 'a fresh page load per row: every heavy result is computed on this switch',
    warm: 'the same page, back to the home screen and to the row again',
  },
  summary,
  results,
};
const text = JSON.stringify(payload, null, 1).split(homedir()).join('~') + '\n';
mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, text);
console.log(`\nwrote ${shortPath(OUT)}`);
cleanup();
await sleep(300);
try { rmSync(work, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }); } catch { /* the browser's last writes */ }
const over = summary.filter((s) => s.over_budget);
const broken = summary.filter((s) => s.timed_out || s.errors);
console.log(`tab-timing: ${over.length} row(s) over the ${BUDGET_MS} ms budget${over.length ? ': ' + over.map((s) => `${s.tab} ${Math.max(s.cold_worst_block_ms, s.warm_worst_block_ms)} ms`).join(', ') : ''}; ${broken.length} with a timeout/error`);
process.exit(broken.length ? 1 : 0);
