/**
 * Issue #117: the comparison's parity test - the web plane and the native route, same inputs, same
 * sheet, byte for byte.
 *
 * The comparison surface (#89) has one model (`cockpit/src/compare.rs`) and two ways to hand it
 * saved files:
 *
 * - the native route: `drafthouse --compare a.drafthouse b.drafthouse` reads each file where the
 *   file system is (`crate::files::compare_file_command` / `file_work`) and queues
 *   `compare:open:<name>:<text>`;
 * - the web route: the page's "compare files" picker reads the same two files in the browser and
 *   dispatches the same commands through the same channel (`cockpit/index.html`).
 *
 * This test drives BOTH routes and compares what each one exports:
 *
 * 1. the native leg runs `cargo test --lib compare::tests::native_compare_sheet_for_parity -- --ignored`
 *    - a helper that opens the two committed fixture files through the route's own functions and
 *    writes the exported sheet (`native-sheet.pdf`) plus the readback facts (`native-facts.json`:
 *    the variant names and digests, the sheet's name and digest, and every row's displayed string);
 * 2. the web leg serves the built plane (`cockpit/`, where `build-web.sh` put `pkg/`), opens
 *    `index.html?host=internal` in a real browser over CDP, sets the SAME two fixture files on the
 *    page's own `#nav-compare-file` input, waits for the comparison screen, checks it paints the
 *    native leg's own rows, then clicks the screen's export control through the app's own published
 *    hit map (`data-screen-hits`, the screens' counterpart of the instrument's `data-hits`) and
 *    reads the downloaded bytes.
 *
 * The two sheets must be IDENTICAL bytes - same engine recomputation, same PDF writer, same
 * delivery - not merely equal to the eye. The leg runs only where its inputs exist (a built plane,
 * a built native target, a CDP browser); anywhere else it reports NOT VERIFIABLE rather than
 * pretending (the repo's own convention - `tests/deployment-serving.test.js` does the same for the
 * wasm response). It never silently skips an input that is present.
 *
 * Run it directly (`node --test tests/compare-web-parity.test.js`) or through `npm test`. The CDP
 * endpoint is `COMPARE_WEB_CDP` (default `http://127.0.0.1:9333`, the host's headless browser);
 * `COMPARE_WEB_PARITY_EVIDENCE=<dir>` also writes a screenshot of the loaded comparison there.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cockpit = join(root, 'cockpit');
const variants = {
  a: join(cockpit, 'assets/variants/a-base.drafthouse'),
  b: join(cockpit, 'assets/variants/b-fan-faster.drafthouse'),
};
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

const withTimeout = (promise, ms, what) => Promise.race([
  promise,
  new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} did not answer within ${ms} ms`)), ms)),
]);

async function waitFor(check, timeoutMs, what, everyMs = 250) {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await check();
    if (last) return last;
    if (Date.now() >= deadline) {
      throw new Error(`${what} did not happen within ${timeoutMs} ms (last readback: ${JSON.stringify(last)})`);
    }
    await sleep(everyMs);
  }
}

const firstDifference = (left, right) => {
  const limit = Math.min(left.length, right.length);
  for (let at = 0; at < limit; at += 1) if (left[at] !== right[at]) return at;
  return left.length === right.length ? -1 : limit;
};

/** The wiring pins: the web entry is the page's own picker, and it hands the app the native route's commands. */
function assertWiringPins() {
  const page = readFileSync(join(cockpit, 'index.html'), 'utf8');
  assert.match(page, /id="nav-compare-open"/, 'the page has no compare control');
  assert.match(page, /id="nav-compare-file"[^>]*multiple/, 'the compare input must take two or three files');
  assert.match(page, /dispatch\('screen:compare'\)/, 'the compare picker must open the comparison screen');
  assert.match(page, /dispatch\('compare:open:'/, 'the compare picker must hand files over as compare:open commands');
  assert.match(page, /nav-compare-file'\]/, 'the public host must remove the compare input with the file group');

  const files = readFileSync(join(cockpit, 'src/files.rs'), 'utf8');
  assert.match(files, /pub fn compare_file_command\(/, 'the native route has no single command producer');
  assert.match(files, /compare:open:\{name\}:\{text\}/, 'the producer must write the compare:open command');
  assert.match(files, /compare_file_command\(&path\)/, 'the native file work must go through the producer');
}

test('issue #117: the web plane opens saved files into the comparison and exports the native route\'s sheet, byte for byte', async (t) => {
  assertWiringPins();
  t.diagnostic('wiring pins read: the page has the compare picker; the native route produces compare:open commands in one function');

  if (Number(process.env.CT_VALIDATION_COUNT_DEPTH ?? 0) > 0) {
    t.diagnostic('NOT VERIFIABLE here: this is the validation-results drift gate\'s counting child, which skips the heavy legs by design; run `node --test tests/compare-web-parity.test.js` for the parity itself');
    return;
  }

  /* ---- the legs' inputs: a built plane, a built native target, a browser. Absent inputs are
     reported, never skipped silently; a present input that misbehaves is a failure. ---- */
  const notVerifiable = (why) => {
    t.diagnostic(`NOT VERIFIABLE: ${why}`);
  };
  const endpoint = (process.env.COMPARE_WEB_CDP ?? 'http://127.0.0.1:9333').replace(/\/$/, '');
  const missing = [];
  if (!existsSync(join(cockpit, 'pkg/drafthouse_cockpit_bg.wasm')) || !existsSync(join(cockpit, 'pkg/drafthouse_cockpit.js'))) {
    missing.push('the built plane (cockpit/pkg - run cockpit/tools/build-web.sh)');
  }
  if (!existsSync(join(cockpit, 'target'))) {
    missing.push('the native target (cockpit/target - run `cd cockpit && cargo test`)');
  }
  if (typeof WebSocket === 'undefined') missing.push('a Node with the global WebSocket (22+)');
  if (missing.length > 0) {
    notVerifiable(`this checkout has no ${missing.join(', no ')}`);
    return;
  }

  try {
    await withTimeout(fetch(`${endpoint}/json/version`).then((response) => response.json()), 5000, 'the CDP endpoint');
  } catch (error) {
    notVerifiable(`no CDP browser at ${endpoint} (${error.message}); start one, e.g. "chrome-headless-shell --remote-debugging-port=9333"`);
    return;
  }

  const work = mkdtempSync(join(tmpdir(), 'drafthouse-compare-parity-'));
  const nativeOut = join(work, 'native');
  const downloads = join(work, 'downloads');
  mkdirSync(nativeOut);
  mkdirSync(downloads);

  let server = null;
  let socket = null;
  let closeTarget = async () => {};
  try {
    /* ---- leg 1: the native route, driven through its own functions ---- */
    t.diagnostic('native leg: cargo test --lib compare::tests::native_compare_sheet_for_parity -- --ignored');
    const cargo = spawnSync(
      'cargo',
      ['test', '--lib', 'compare::tests::native_compare_sheet_for_parity', '--', '--ignored', '--nocapture'],
      {
        cwd: cockpit,
        env: { ...process.env, DRAFTHOUSE_COMPARE_PARITY_OUT: nativeOut },
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
        timeout: 900_000,
      },
    );
    const cargoTail = `${cargo.stdout ?? ''}\n${cargo.stderr ?? ''}`.trim().split('\n').slice(-25).join('\n');
    assert.notEqual(cargo.error?.code, 'ENOENT', 'no cargo on PATH');
    assert.equal(cargo.status, 0, `the native parity helper did not run (exit ${cargo.status}):\n${cargoTail}`);
    assert.match(cargo.stdout ?? '', /1 passed/, `the native parity helper did not run its one test:\n${cargoTail}`);
    const nativeFacts = JSON.parse(readFileSync(join(nativeOut, 'native-facts.json'), 'utf8'));
    const nativeSheet = readFileSync(join(nativeOut, 'native-sheet.pdf'));
    assert.equal(nativeSheet.length, nativeFacts.sheet_bytes, 'the native sheet is not the one the facts describe');
    assert.equal(sha256(nativeSheet), nativeFacts.sheet_sha256, 'the native sheet does not hash to its own facts');
    t.diagnostic(`native leg: ${nativeFacts.sheet_name} - ${nativeSheet.length} bytes, sha256 ${nativeFacts.sheet_sha256}, digests ${nativeFacts.digests.join(' + ')}`);

    /* ---- leg 2: the web plane, served and driven in a real browser ---- */
    const { startDeployServer } = await import(pathToFileURL(join(root, 'scripts/deploytest-serve.mjs')).href);
    server = await startDeployServer({ servedRoot: cockpit, basePath: '/' });

    const created = await withTimeout(
      fetch(`${endpoint}/json/new?about:blank`, { method: 'PUT' }).then((response) => response.json()),
      15000,
      'target creation',
    );
    assert.ok(created.webSocketDebuggerUrl, `the CDP endpoint created a target without a debugger socket: ${JSON.stringify(created)}`);
    closeTarget = () => fetch(`${endpoint}/json/close/${created.id}`).then(() => {});
    socket = new WebSocket(created.webSocketDebuggerUrl);
    await withTimeout(new Promise((done, fail) => {
      socket.onopen = done;
      socket.onerror = () => fail(new Error('the DevTools socket could not be opened'));
    }), 8000, 'the DevTools socket');

    let sequence = 0;
    const pending = new Map();
    const loadingFailed = [];
    const responses = new Map();
    const consoleErrors = [];
    socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.id && pending.has(message.id)) {
        const settle = pending.get(message.id);
        pending.delete(message.id);
        settle(message);
        return;
      }
      if (message.method === 'Network.responseReceived') {
        responses.set(message.params.requestId, {
          url: message.params.response.url,
          status: message.params.response.status,
          mimeType: message.params.response.mimeType,
        });
      }
      if (message.method === 'Network.loadingFailed' && !message.params.canceled) {
        loadingFailed.push({ errorText: message.params.errorText, blockedReason: message.params.blockedReason ?? null });
      }
      if (message.method === 'Runtime.exceptionThrown') {
        consoleErrors.push(String(message.params.exceptionDetails?.exception?.description ?? message.params.exceptionDetails?.text ?? ''));
      }
    };
    const send = (method, params = {}, timeout = 30000) => withTimeout(new Promise((done, fail) => {
      const id = ++sequence;
      pending.set(id, (message) => (message.error ? fail(new Error(`${method}: ${message.error.message}`)) : done(message.result)));
      socket.send(JSON.stringify({ id, method, params }));
    }), timeout, method);
    const evaluate = async (expression, timeout = 30000) =>
      (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, timeout)).result.value;

    await send('Page.enable');
    await send('Runtime.enable');
    await send('Network.enable');
    await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
    /* The host's headless browser runs the old headless mode: the legacy page-scoped behaviour is
       the one that mode honours, so it is set first and the browser-scoped command beside it. */
    let downloadBehavior = null;
    for (const [method, params] of [
      ['Page.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads }],
      ['Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads }],
    ]) {
      try {
        await send(method, params, 8000);
        downloadBehavior = `${downloadBehavior === null ? '' : `${downloadBehavior} + `}${method}`;
      } catch (error) {
        t.diagnostic(`web leg: ${method} refused (${error.message}); continuing with ${downloadBehavior ?? 'none'}`);
      }
    }
    assert.ok(downloadBehavior !== null, 'neither download-behaviour command was accepted; a download could not be captured');

    await send('Page.navigate', { url: `${server.url}/index.html?host=internal` });
    const booted = await waitFor(async () => {
      const state = await evaluate(`(() => { const r = document.getElementById('viz-root'); return r ? { load: r.dataset.load, host: r.dataset.host, source: r.dataset.source, screen: r.dataset.screen } : null; })()`);
      return state && state.load === 'ready' && state.host === 'internal' ? state : null;
    }, 120_000, 'the served plane to boot');
    t.diagnostic(`web leg: the plane booted (engine source ${JSON.stringify(booted.source)}) at ${server.url}/index.html?host=internal`);

    /* The probe input must be the real one: read where the page put it (a public host removes it). */
    const probe = await evaluate(`(() => {
      const input = document.getElementById('nav-compare-file');
      const button = document.getElementById('nav-compare-open');
      return { input: input ? { multiple: input.multiple, accept: input.accept } : null, button: Boolean(button) };
    })()`);
    assert.ok(probe.input, 'the internal host is missing #nav-compare-file');
    assert.ok(probe.input.multiple, 'the compare input does not take two or three files');
    assert.ok(probe.button, 'the internal host is missing #nav-compare-open');

    /* The screen's own rows, cross-checked after loading (below) against the native leg's. */
    const readScreen = () => evaluate(`(() => {
      const r = document.getElementById('viz-root');
      const mirror = document.getElementById('mirror-screens');
      let parsed = null;
      try { parsed = JSON.parse(mirror ? mirror.textContent : 'null'); } catch { parsed = null; }
      let hits = {};
      try { hits = JSON.parse(r.dataset.screenHits || '{}'); } catch { hits = {}; }
      const canvas = document.getElementById('viz-canvas');
      const rect = canvas ? canvas.getBoundingClientRect() : { x: 0, y: 0 };
      return {
        screen: r.dataset.screen || '',
        engine: r.dataset.source || '',
        nav: r.dataset.nav || '',
        strings: parsed ? parsed.strings || [] : [],
        viewport: parsed ? parsed.viewport : null,
        content: parsed ? parsed.content : null,
        hits,
        canvas: { x: rect.x, y: rect.y },
      };
    })()`);

    /* Hand the two files to the page's own picker input - exactly what a user's dialog would set -
       then let the page's own change handler do the reading and the dispatching. */
    await send('DOM.enable');
    const { root: domRoot } = await send('DOM.getDocument', { depth: -1 });
    const { nodeId } = await send('DOM.querySelector', { nodeId: domRoot.nodeId, selector: '#nav-compare-file' });
    assert.ok(nodeId, 'the page has no #nav-compare-file input to set files on');
    await send('DOM.setFileInputFiles', { files: [variants.a, variants.b], nodeId });

    let how = 'the input\'s own change event';
    const loaded = await waitFor(async () => {
      const state = await readScreen();
      const wanted = [nativeFacts.files[0], nativeFacts.files[1]].every((name) => state.strings.includes(name));
      const first = nativeFacts.rows[0]?.[1]?.[0];
      return state.screen === 'compare' && wanted && first !== undefined && state.strings.includes(first) ? state : null;
    }, 12_000, 'the page\'s picker path to open the comparison').catch(async () => {
      /* Headless Chromium does not always fire `change` when files are set over the protocol (the
         platform dialog is not part of headless). The page's handler is the same one a picker would
         call, so it is invoked on the page's own element once - and the diagnostic says so, rather
         than the test claiming a dialog path that did not run. */
      how = 'the input\'s change handler, invoked explicitly (headless Chromium did not fire change on DOM.setFileInputFiles)';
      await evaluate(`(() => { document.getElementById('nav-compare-file').dispatchEvent(new Event('change', { bubbles: true })); })()`);
      return waitFor(async () => {
        const state = await readScreen();
        const wanted = [nativeFacts.files[0], nativeFacts.files[1]].every((name) => state.strings.includes(name));
        const first = nativeFacts.rows[0]?.[1]?.[0];
        return state.screen === 'compare' && wanted && first !== undefined && state.strings.includes(first) ? state : null;
      }, 20_000, 'the page\'s change handler to open the comparison');
    });
    t.diagnostic(`web leg: the comparison opened via ${how}`);

    /* The screen must paint the engine's own recomputation - the native route's rows, string for
       string. A screen rendering the files' stored values, or a fixture replay, misses rows here. */
    const missingRows = [];
    for (const [label, values] of nativeFacts.rows) {
      for (const value of values) {
        if (!loaded.strings.includes(value)) missingRows.push(`${label}: ${JSON.stringify(value)}`);
      }
    }
    assert.deepEqual(missingRows, [], `the web comparison does not paint the native route's engine values: ${missingRows.slice(0, 8).join('; ')} (painted sample: ${JSON.stringify(loaded.strings.slice(0, 40))})`);
    for (const name of nativeFacts.files) {
      assert.ok(loaded.strings.includes(name), `the web comparison does not paint ${name}`);
    }
    /* The head of each column carries the variant's digest tag - the native leg's own digest - and
       the status row carries the engine's verdict, so the screen is provably showing THESE files,
       run by the engine. */
    for (const digest of nativeFacts.digests) {
      assert.ok(
        loaded.strings.includes(digest.slice(0, 8)),
        `the web comparison does not carry the digest tag ${digest.slice(0, 8)} (painted sample: ${JSON.stringify(loaded.strings.slice(0, 50))})`,
      );
    }
    assert.ok(
      loaded.strings.includes('accepted'),
      `the web comparison does not carry the engine's own status (painted sample: ${JSON.stringify(loaded.strings.slice(0, 60))})`,
    );
    t.diagnostic(`web leg: the screen paints every one of the native leg's ${nativeFacts.rows.length} rows for both variants`);

    if (process.env.COMPARE_WEB_PARITY_EVIDENCE) {
      const shot = await send('Page.captureScreenshot', { format: 'png' }, 30000);
      const dir = process.env.COMPARE_WEB_PARITY_EVIDENCE;
      mkdirSync(dir, { recursive: true });
      const path = join(dir, 'compare-opened-on-the-web.png');
      writeFileSync(path, Buffer.from(shot.data, 'base64'));
      t.diagnostic(`web leg: screenshot written to ${path}`);
    }

    /* ---- the export: click the screen's own control, through the app's published hit map ---- */
    /* The app parks when idle (#82): `data-screen-hits` is from its last frame, and the canvas grows
       when the control bar hides for a non-instrument screen, so the published layout only settles a
       frame after a device event wakes it. Move over an empty spot first, wait for the published rect
       to hold still, then aim - and re-read the rect on every attempt, in case it settles further. */
    let exportHit = null;
    for (let settle = 0; settle < 6; settle += 1) {
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 720, y: 500, button: 'none', buttons: 0 });
      await sleep(450);
      const fresh = await readScreen();
      const rect = fresh.hits['cmp.export'];
      assert.ok(rect, `the comparison screen publishes no cmp.export rect (hits: ${JSON.stringify(Object.keys(fresh.hits))})`);
      if (exportHit !== null && JSON.stringify(exportHit) === JSON.stringify(rect)) break;
      exportHit = rect;
    }
    t.diagnostic(`web leg: the export rect settled at ${JSON.stringify(exportHit)}`);
    assert.ok(exportHit[1] >= 4 && exportHit[1] + exportHit[3] <= 1000 - 20, `the cmp.export rect ${JSON.stringify(exportHit)} is not inside the viewport`);

    /* What the app said about the export, from the screen it paints (the toast is one of its texts). */
    const exportWords = () => evaluate(`(() => {
      const mirror = document.getElementById('mirror-screens');
      let parsed = null;
      try { parsed = JSON.parse(mirror ? mirror.textContent : 'null'); } catch { parsed = null; }
      return (parsed ? parsed.strings || [] : []).filter((s) => s.includes('download started') || s.includes('export failed'));
    })()`).catch(() => []);

    let downloaded = null;
    for (let attempt = 1; attempt <= 3 && downloaded === null; attempt += 1) {
      const fresh = await readScreen();
      const rect = fresh.hits['cmp.export'] ?? exportHit;
      const clickAt = { x: fresh.canvas.x + rect[0] + rect[2] / 2, y: fresh.canvas.y + rect[1] + rect[3] / 2 };
      const clickX = Math.round(clickAt.x);
      const clickY = Math.round(clickAt.y);
      t.diagnostic(`web leg: clicking cmp.export at canvas rect ${JSON.stringify(rect)} -> page (${clickX}, ${clickY}) (attempt ${attempt}); layout nav=${fresh.nav} viewport=${JSON.stringify(fresh.viewport)} content=${JSON.stringify(fresh.content)}`);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: clickX, y: clickY, button: 'none', buttons: 0 });
      await sleep(200);
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: clickX, y: clickY, button: 'left', buttons: 1, clickCount: 1 });
      await sleep(80);
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: clickX, y: clickY, button: 'left', buttons: 0, clickCount: 1 });
      const words = await waitFor(async () => {
        const seen = await exportWords();
        return seen.length > 0 ? seen : null;
      }, 8000, 'the app to answer the export click').catch(() => null);
      t.diagnostic(`web leg: the app answered with ${JSON.stringify(words)}`);
      if (words && words.some((s) => s.includes('export failed'))) {
        throw new Error(`the app's own export handler failed: ${words.join('; ')}`);
      }
      const landed = await waitFor(async () => {
        const files = readdirSync(downloads).filter((name) => name.endsWith('.pdf'));
        return files.length > 0 ? files[0] : null;
      }, 12_000, 'the export download to land').catch(() => null);
      downloaded = landed;
    }
    assert.ok(
      downloaded,
      `the export download did not land in the download directory after three clicks; the app said ${JSON.stringify(await exportWords())}; the page's console errors: ${JSON.stringify(consoleErrors.slice(0, 3))}`,
    );
    const webSheet = readFileSync(join(downloads, downloaded));

    /* The app's own toast for the export is a bonus reading; the bytes below are the assertion. */
    const afterExport = await readScreen().catch(() => ({ strings: [] }));
    if (!afterExport.strings.some((s) => s.includes('download started'))) {
      t.diagnostic('web leg: the export toast was not caught in the painted strings (it expires); the download is the evidence');
    }

    /* ---- the parity itself ---- */
    assert.equal(downloaded, nativeFacts.sheet_name, `the web download is named ${downloaded}; the native route names its sheet ${nativeFacts.sheet_name}`);
    const at = firstDifference(webSheet, nativeSheet);
    assert.equal(at, -1, [
      'the two routes\' exported sheets differ - the web plane and the native route must recompute, write and deliver the same bytes',
      `native: ${nativeSheet.length} bytes, sha256 ${sha256(nativeSheet)}`,
      `web:    ${webSheet.length} bytes, sha256 ${sha256(webSheet)}`,
      at === -1 ? '' : `first difference at byte ${at}`,
    ].filter(Boolean).join('\n'));
    t.diagnostic(`PARITY: both routes exported ${nativeFacts.sheet_name} - ${webSheet.length} bytes, sha256 ${sha256(webSheet)} (native ${sha256(nativeSheet)}), identical`);

    /* The served page must not have failed a load while doing it. */
    const pageFailures = loadingFailed.filter((failure) => failure.blockedReason !== null);
    assert.deepEqual(pageFailures, [], `the served page had blocked network loads: ${JSON.stringify(pageFailures)}`);
    const wasmResponse = [...responses.values()].find((response) => response.url.endsWith('drafthouse_cockpit_bg.wasm'));
    assert.ok(wasmResponse && wasmResponse.status === 200, `the served wasm response is ${JSON.stringify(wasmResponse)}`);
    if (consoleErrors.length > 0) t.diagnostic(`web leg: ${consoleErrors.length} console exception(s): ${consoleErrors.slice(0, 3).join(' | ')}`);
  } finally {
    await closeTarget().catch(() => {});
    if (socket) { try { socket.close(); } catch { /* the socket may already be gone */ } }
    if (server) await server.close().catch(() => {});
    rmSync(work, { recursive: true, force: true });
  }
});
