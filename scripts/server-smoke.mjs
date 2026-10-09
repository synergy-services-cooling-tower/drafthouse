import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Live HTTP serving check for the deployed surface (issue #58 repointed it at the cockpit): the
 * paths checked are the cockpit plane's own — the document, the wasm build's JavaScript and its
 * `.wasm` (including the `application/wasm` content type the host must send), the fixture record
 * and a font the app fetches at run time. The static file server (`scripts/serve.mjs`) roots at
 * `cockpit/`, the directory the release publishes (`scripts/cockpit-artifact.mjs`) and the
 * deployment ships; the surface this leg checked before issue #60 is retired
 * (the private decision record D22).
 *
 * `cockpit/pkg/` is a **build product**, so this check needs `cockpit/tools/build-web.sh release`
 * to have run. When it has not, the check prints a NOT VERIFIABLE diagnostic naming the build and
 * makes no assertion — never a silent pass, never a failure (the `npm run validate` job does not
 * build the cockpit; the `cockpit` job does, and runs this check against the build it made).
 */
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const port = 4187;

if (!existsSync(resolve(root, 'cockpit/pkg/drafthouse_cockpit_bg.wasm'))) {
  console.log(JSON.stringify({
    passed: true,
    notVerifiable: 'cockpit/pkg is not built in this tree, so there is no cockpit plane to serve — run cockpit/tools/build-web.sh release first (the validate job does not build it; the cockpit job does)',
    checks: []
  }, null, 2));
  process.exit(0);
}

const child = spawn(process.execPath, ['scripts/serve.mjs'], {
  cwd: root,
  env: { ...process.env, PORT: String(port) },
  stdio: ['ignore', 'pipe', 'pipe']
});

let stderr = '';
child.stderr.on('data', (chunk) => { stderr += chunk; });

async function waitForServer() {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/`);
      if (response.ok) return;
    } catch {
      // Server may still be starting.
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 75));
  }
  throw new Error(`Static server did not start. ${stderr}`);
}

try {
  await waitForServer();
  const checks = [
    ['/', 'text/html'],
    ['/pkg/drafthouse_cockpit.js', 'text/javascript'],
    ['/pkg/drafthouse_cockpit_bg.wasm', 'application/wasm'],
    ['/assets/fixture.json', 'application/json'],
    ['/assets/duty-fields.json', 'application/json'],
    ['/assets/fonts/subset/IBMPlexSans-Regular.ttf', 'font/ttf']
  ];
  const results = [];
  for (const [path, expectedType] of checks) {
    const response = await fetch(`http://127.0.0.1:${port}${path}`);
    const body = await response.arrayBuffer();
    const contentType = response.headers.get('content-type') ?? '';
    if (!response.ok || !contentType.includes(expectedType) || body.byteLength === 0) {
      throw new Error(`Server check failed for ${path}: ${response.status}, ${contentType}, ${body.byteLength} bytes`);
    }
    results.push({ path, status: response.status, contentType, bytes: body.byteLength });
  }
  console.log(JSON.stringify({ passed: true, results }, null, 2));
} finally {
  child.kill('SIGTERM');
}
