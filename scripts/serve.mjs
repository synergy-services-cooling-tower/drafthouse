import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * The development server for the served surface. Its root is `cockpit/` — the product UI the
 * release publishes (`cockpit/index.html`, `cockpit/pkg/**`, `cockpit/assets/**`) and the
 * directory the deployment ships (issue #58) — so what a browser sees here is what the host
 * serves, at the paths the plane carries.
 *
 * The former surface (a bounded JavaScript preview, retired by issue #60,
 * the private decision record D22) is gone: this server stopped pointing at it when the cockpit became
 * the product UI, and the surface itself is deleted.
 */
const root = normalize(join(fileURLToPath(new URL('..', import.meta.url)), 'cockpit'));
const port = Number(process.env.PORT ?? 4173);
const mime = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.txt': 'text/plain; charset=utf-8'
};

createServer((req, res) => {
  const requested = decodeURIComponent((req.url ?? '/').split('?')[0]);
  const relative = requested === '/' ? 'index.html' : requested.replace(/^\/+/, '');
  const filePath = normalize(join(root, relative));

  if (!filePath.startsWith(root)) {
    res.writeHead(403).end('Forbidden');
    return;
  }

  try {
    if (!statSync(filePath).isFile()) throw new Error('Not a file');
    res.writeHead(200, {
      'Content-Type': mime[extname(filePath)] ?? 'application/octet-stream',
      'Cache-Control': 'no-store'
    });
    createReadStream(filePath).pipe(res);
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('Not found');
  }
}).listen(port, () => {
  console.log(`Synergy Drafthouse: http://localhost:${port}`);
});
