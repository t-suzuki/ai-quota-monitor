const ANTHROPIC_OAUTH_BETA = 'oauth-2025-04-20';
const CLAUDE_USAGE_URL = 'https://api.anthropic.com/api/oauth/usage';
const CODEX_USAGE_URL = 'https://chatgpt.com/backend-api/wham/usage';

function assertToken(token) {
  if (!token || typeof token !== 'string' || !token.trim()) {
    throw new Error('Token is required');
  }
}

function assertUpstreamUrl(url) {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error('Invalid upstream URL');
  }
  if (parsed.protocol !== 'https:') {
    throw new Error('Upstream URL must use https');
  }
  if (parsed.hostname !== 'api.anthropic.com' && parsed.hostname !== 'chatgpt.com') {
    throw new Error('Upstream host is not allowlisted');
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

function createUsageClient(deps = {}) {
  const fetchImpl = deps.fetchImpl || fetch;

  async function fetchUsageRaw(url, headers) {
    assertUpstreamUrl(url);
    let upstream;
    try {
      upstream = await fetchImpl(url, { headers });
    } catch (error) {
      throw new Error(`Network request failed: ${error?.message || error}`);
    }
    const contentType = upstream.headers.get('content-type') || 'application/json';
    let body = '';
    try {
      body = await upstream.text();
    } catch (error) {
      throw new Error(`Failed to read upstream response body: ${error?.message || error}`);
    }
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
    return fetchUsageRaw(CLAUDE_USAGE_URL, {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
      'anthropic-beta': ANTHROPIC_OAUTH_BETA,
    });
  }

  async function fetchCodexUsageRaw(token) {
    assertToken(token);
    return fetchUsageRaw(CODEX_USAGE_URL, {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
    });
  }

  return {
    fetchUsageRaw,
    fetchClaudeUsageRaw,
    fetchCodexUsageRaw,
  };
}

const { fetchUsageRaw, fetchClaudeUsageRaw, fetchCodexUsageRaw } = createUsageClient();

module.exports = {
  ANTHROPIC_OAUTH_BETA,
  CLAUDE_USAGE_URL,
  CODEX_USAGE_URL,
  createUsageClient,
  safeJsonParse,
  fetchUsageRaw,
  fetchClaudeUsageRaw,
  fetchCodexUsageRaw,
};
