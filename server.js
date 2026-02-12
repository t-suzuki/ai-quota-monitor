const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { fetchClaudeUsageRaw, fetchCodexUsageRaw, safeJsonParse } = require('./src/core/usage-clients');
const { version: APP_VERSION } = require('./package.json');

const PORT = Number(process.env.PORT || 4173);
const PUBLIC_DIR = path.join(__dirname, 'public');

function corsHeaders(req) {
  return {
    'Access-Control-Allow-Origin': req.headers.origin || '*',
    'Access-Control-Allow-Methods': 'GET, OPTIONS',
    'Access-Control-Allow-Headers': 'x-quota-token',
    'Access-Control-Max-Age': '86400',
  };
}

function writeJson(res, req, status, data) {
  res.writeHead(status, { ...corsHeaders(req), 'Content-Type': 'application/json' });
  res.end(JSON.stringify(data));
}

async function handleClaude(req, res) {
  if (req.method === 'OPTIONS') {
    res.writeHead(204, corsHeaders(req));
    res.end();
    return;
  }
  if (req.method !== 'GET') {
    writeJson(res, req, 405, { error: 'Method not allowed' });
    return;
  }
  const token = req.headers['x-quota-token'];
  if (!token) {
    writeJson(res, req, 400, { error: 'x-quota-token header required' });
    return;
  }
  try {
    const upstream = await fetchClaudeUsageRaw(token);
    res.writeHead(upstream.status, {
      ...corsHeaders(req),
      'Content-Type': upstream.contentType,
    });
    res.end(upstream.body);
  } catch (e) {
    writeJson(res, req, 502, { error: 'Upstream request failed', detail: String(e.message || e) });
  }
}

async function handleCodex(req, res) {
  if (req.method === 'OPTIONS') {
    res.writeHead(204, corsHeaders(req));
    res.end();
    return;
  }
  if (req.method !== 'GET') {
    writeJson(res, req, 405, { error: 'Method not allowed' });
    return;
  }
  const token = req.headers['x-quota-token'];
  if (!token) {
    writeJson(res, req, 400, { error: 'x-quota-token header required' });
    return;
  }
  try {
    const upstream = await fetchCodexUsageRaw(token);
    const contentType = upstream.contentType;
    const parsed = safeJsonParse(upstream.body);

    if (!upstream.ok && upstream.status === 403 && contentType.includes('text/html')) {
      writeJson(res, req, 403, {
        error: 'Upstream blocked request (OpenAI edge / Cloudflare)',
        detail: 'Local request was blocked by chatgpt.com edge protections.',
        upstream_status: 403,
      });
      return;
    }

    const headers = { ...corsHeaders(req), 'Content-Type': contentType };
    for (const [k, v] of Object.entries(upstream.headers)) {
      if (k.startsWith('x-ratelimit') || k.startsWith('x-rate-limit') || k === 'retry-after') {
        headers[`x-upstream-${k}`] = v;
      }
    }
    res.writeHead(upstream.status, headers);
    if (!upstream.ok && parsed && typeof parsed === 'object') {
      res.end(JSON.stringify(parsed));
      return;
    }
    res.end(upstream.body);
  } catch (e) {
    writeJson(res, req, 502, { error: 'Upstream request failed', detail: String(e.message || e) });
  }
}

function guessMime(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  if (ext === '.html') return 'text/html; charset=utf-8';
  if (ext === '.js') return 'text/javascript; charset=utf-8';
  if (ext === '.css') return 'text/css; charset=utf-8';
  if (ext === '.json') return 'application/json; charset=utf-8';
  if (ext === '.svg') return 'image/svg+xml';
  if (ext === '.png') return 'image/png';
  if (ext === '.jpg' || ext === '.jpeg') return 'image/jpeg';
  if (ext === '.ico') return 'image/x-icon';
  return 'application/octet-stream';
}

async function serveStatic(req, res, urlPath) {
  let pathname = decodeURIComponent(urlPath.split('?')[0]);
  if (pathname === '/') pathname = '/index.html';
  const normalized = path.normalize(pathname).replace(/^(\.\.(\/|\\|$))+/, '');
  let filePath = path.join(PUBLIC_DIR, normalized);

  try {
    const stat = await fs.stat(filePath);
    if (stat.isDirectory()) filePath = path.join(filePath, 'index.html');
    const body = await fs.readFile(filePath);
    res.writeHead(200, { 'Content-Type': guessMime(filePath) });
    res.end(body);
  } catch {
    // Single-page fallback
    try {
      const body = await fs.readFile(path.join(PUBLIC_DIR, 'index.html'));
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(body);
    } catch {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('Not found');
    }
  }
}

const server = http.createServer(async (req, res) => {
  const url = req.url || '/';
  if (url.startsWith('/api/version')) {
    writeJson(res, req, 200, { version: APP_VERSION });
    return;
  }
  if (url.startsWith('/api/claude')) {
    await handleClaude(req, res);
    return;
  }
  if (url.startsWith('/api/codex')) {
    await handleCodex(req, res);
    return;
  }
  await serveStatic(req, res, url);
});

server.listen(PORT, () => {
  process.stdout.write(`http://localhost:${PORT}\n`);
});
