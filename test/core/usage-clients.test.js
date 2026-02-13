const test = require('node:test');
const assert = require('node:assert/strict');

const {
  ANTHROPIC_OAUTH_BETA,
  createUsageClient,
  safeJsonParse,
} = require('../../src/core/usage-clients');

function makeResponse({ ok, status, contentType, body, extraHeaders = {} }) {
  const headers = new Headers({
    'content-type': contentType,
    ...extraHeaders,
  });
  return {
    ok,
    status,
    headers,
    text: async () => body,
  };
}

test('safeJsonParse returns parsed object or null', () => {
  assert.deepEqual(safeJsonParse('{"a":1}'), { a: 1 });
  assert.equal(safeJsonParse('not-json'), null);
});

test('fetchClaudeUsageRaw calls anthropic endpoint with expected headers', async () => {
  let calledUrl = '';
  let calledHeaders = null;

  const client = createUsageClient({
    fetchImpl: async (url, options) => {
      calledUrl = url;
      calledHeaders = options.headers;
      return makeResponse({
        ok: true,
        status: 200,
        contentType: 'application/json',
        body: '{"ok":true}',
      });
    },
  });

  const result = await client.fetchClaudeUsageRaw('abc123');

  assert.equal(calledUrl, 'https://api.anthropic.com/api/oauth/usage');
  assert.equal(calledHeaders.Authorization, 'Bearer abc123');
  assert.equal(calledHeaders.Accept, 'application/json');
  assert.equal(calledHeaders['anthropic-beta'], ANTHROPIC_OAUTH_BETA);
  assert.equal(result.ok, true);
  assert.equal(result.status, 200);
  assert.equal(result.contentType, 'application/json');
  assert.deepEqual(result.headers, { 'content-type': 'application/json' });
});

test('fetchCodexUsageRaw calls codex endpoint with expected headers', async () => {
  let calledUrl = '';
  let calledHeaders = null;

  const client = createUsageClient({
    fetchImpl: async (url, options) => {
      calledUrl = url;
      calledHeaders = options.headers;
      return makeResponse({
        ok: false,
        status: 429,
        contentType: 'application/json',
        body: '{"error":"rate limited"}',
        extraHeaders: { 'x-ratelimit-remaining': '0' },
      });
    },
  });

  const result = await client.fetchCodexUsageRaw('xyz789');

  assert.equal(calledUrl, 'https://chatgpt.com/backend-api/wham/usage');
  assert.equal(calledHeaders.Authorization, 'Bearer xyz789');
  assert.equal(calledHeaders.Accept, 'application/json');
  assert.equal(result.ok, false);
  assert.equal(result.status, 429);
  assert.equal(result.headers['x-ratelimit-remaining'], '0');
});

test('fetch client throws when token is missing', async () => {
  const client = createUsageClient({ fetchImpl: async () => makeResponse({
    ok: true,
    status: 200,
    contentType: 'application/json',
    body: '{}',
  }) });

  await assert.rejects(() => client.fetchClaudeUsageRaw(''), /Token is required/);
  await assert.rejects(() => client.fetchCodexUsageRaw(null), /Token is required/);
  await assert.rejects(() => client.fetchCodexUsageRaw('   '), /Token is required/);
});

test('fetchUsageRaw surfaces network errors with context', async () => {
  const client = createUsageClient({
    fetchImpl: async () => {
      throw new Error('socket hang up');
    },
  });

  await assert.rejects(
    () => client.fetchClaudeUsageRaw('abc123'),
    /Network request failed: socket hang up/
  );
});
