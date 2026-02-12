const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');

const PORT = Number(process.env.PORT || 4173);
const PUBLIC_DIR = path.join(__dirname, 'public');
const ANTHROPIC_OAUTH_BETA = 'oauth-2025-04-20';

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
    const upstream = await fetch('https://api.anthropic.com/api/oauth/usage', {
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/json',
        'anthropic-beta': ANTHROPIC_OAUTH_BETA,
      },
    });
    const body = await upstream.text();
    res.writeHead(upstream.status, {
      ...corsHeaders(req),
      'Content-Type': upstream.headers.get('content-type') || 'application/json',
    });
    res.end(body);
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
    const upstream = await fetch('https://chatgpt.com/backend-api/wham/usage', {
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/json',
      },
    });
    const body = await upstream.text();
    const contentType = upstream.headers.get('content-type') || 'application/json';

    if (!upstream.ok && upstream.status === 403 && contentType.includes('text/html')) {
      writeJson(res, req, 403, {
        error: 'Upstream blocked request (OpenAI edge / Cloudflare)',
        detail: 'Local request was blocked by chatgpt.com edge protections.',
        upstream_status: upstream.status,
      });
      return;
    }

    const headers = { ...corsHeaders(req), 'Content-Type': contentType };
    for (const [k, v] of upstream.headers.entries()) {
      if (k.startsWith('x-ratelimit') || k.startsWith('x-rate-limit') || k === 'retry-after') {
        headers[`x-upstream-${k}`] = v;
      }
    }
    res.writeHead(upstream.status, headers);
    res.end(body);
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
  process.stdout.write(`Dev server running at http://localhost:${PORT}\n`);
});
