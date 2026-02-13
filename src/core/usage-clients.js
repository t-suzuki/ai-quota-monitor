const ANTHROPIC_OAUTH_BETA = 'oauth-2025-04-20';

function assertToken(token) {
  if (!token || typeof token !== 'string' || !token.trim()) {
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

function createUsageClient(deps = {}) {
  const fetchImpl = deps.fetchImpl || fetch;

  async function fetchUsageRaw(url, headers) {
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

  return {
    fetchUsageRaw,
    fetchClaudeUsageRaw,
    fetchCodexUsageRaw,
  };
}

const { fetchUsageRaw, fetchClaudeUsageRaw, fetchCodexUsageRaw } = createUsageClient();

module.exports = {
  ANTHROPIC_OAUTH_BETA,
  createUsageClient,
  safeJsonParse,
  fetchUsageRaw,
  fetchClaudeUsageRaw,
  fetchCodexUsageRaw,
};
