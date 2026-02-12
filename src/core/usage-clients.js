const ANTHROPIC_OAUTH_BETA = 'oauth-2025-04-20';

function assertToken(token) {
  if (!token || typeof token !== 'string') {
    throw new Error('Token is required');
  }
}

function safeJsonParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function extractHeaders(headers) {
  const out = {};
  for (const [k, v] of headers.entries()) {
    out[k.toLowerCase()] = v;
  }
  return out;
}

async function fetchUsageRaw(url, headers) {
  const upstream = await fetch(url, { headers });
  const contentType = upstream.headers.get('content-type') || 'application/json';
  const body = await upstream.text();
  return {
    ok: upstream.ok,
    status: upstream.status,
    contentType,
    body,
    headers: extractHeaders(upstream.headers),
  };
}

async function fetchClaudeUsageRaw(token) {
  assertToken(token);
  return fetchUsageRaw('https://api.anthropic.com/api/oauth/usage', {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
    'anthropic-beta': ANTHROPIC_OAUTH_BETA,
  });
}

async function fetchCodexUsageRaw(token) {
  assertToken(token);
  return fetchUsageRaw('https://chatgpt.com/backend-api/wham/usage', {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
  });
}

module.exports = {
  ANTHROPIC_OAUTH_BETA,
  safeJsonParse,
  fetchClaudeUsageRaw,
  fetchCodexUsageRaw,
};
