const test = require('node:test');
const assert = require('node:assert/strict');

const { buildError, createUsageService } = require('../../src/core/usage-service');

test('buildError returns sanitized status-based messages', () => {
  const raw = { status: 400, contentType: 'application/json' };
  assert.equal(buildError(raw, { error: 'token invalid' }), 'Upstream rejected request (HTTP 400)');
  assert.equal(buildError({ status: 401, contentType: 'application/json' }, { detail: 'bad request' }), 'Authentication failed (HTTP 401)');
});

test('buildError maps html 403 to edge block message', () => {
  const raw = { status: 403, contentType: 'text/html; charset=utf-8' };
  assert.equal(buildError(raw, null), 'Upstream blocked request (OpenAI edge / Cloudflare)');
});

test('fetchNormalizedUsage returns normalized claude payload', async () => {
  const parsedPayload = { any: 'payload' };
  const expectedWindows = [{ name: '5時間', utilization: 12 }];

  const svc = createUsageService({
    fetchClaudeUsageRaw: async (token) => {
      assert.equal(token, 'claude-token');
      return {
        ok: true,
        status: 200,
        contentType: 'application/json',
        body: '{"any":"payload"}',
      };
    },
    safeJsonParse: (body) => {
      assert.equal(body, '{"any":"payload"}');
      return parsedPayload;
    },
    parseClaudeUsage: (data) => {
      assert.equal(data, parsedPayload);
      return expectedWindows;
    },
  });

  const result = await svc.fetchNormalizedUsage('claude', 'claude-token');

  assert.equal(result.raw, parsedPayload);
  assert.equal(result.windows, expectedWindows);
});

test('fetchNormalizedUsage returns normalized codex payload', async () => {
  const svc = createUsageService({
    fetchCodexUsageRaw: async () => ({
      ok: true,
      status: 200,
      contentType: 'application/json',
      body: '{"rate_limit":{}}',
    }),
    safeJsonParse: () => ({ rate_limit: {} }),
    parseCodexUsage: () => [{ name: '7日間', utilization: 55 }],
  });

  const result = await svc.fetchNormalizedUsage('codex', 'codex-token');

  assert.deepEqual(result.windows, [{ name: '7日間', utilization: 55 }]);
});

test('fetchNormalizedUsage throws for unsupported service', async () => {
  const svc = createUsageService();

  await assert.rejects(
    () => svc.fetchNormalizedUsage('unknown', 'token'),
    /Unsupported service/
  );
});

test('fetchNormalizedUsage surfaces sanitized upstream status message', async () => {
  const svc = createUsageService({
    fetchClaudeUsageRaw: async () => ({
      ok: false,
      status: 401,
      contentType: 'application/json',
      body: '{"error":"invalid token"}',
    }),
    safeJsonParse: () => ({ error: 'invalid token' }),
  });

  await assert.rejects(
    () => svc.fetchNormalizedUsage('claude', 'bad-token'),
    /Authentication failed \(HTTP 401\)/
  );
});

test('fetchNormalizedUsage throws for successful non-JSON payload', async () => {
  const svc = createUsageService({
    fetchCodexUsageRaw: async () => ({
      ok: true,
      status: 200,
      contentType: 'text/html',
      body: '<html>oops</html>',
    }),
    safeJsonParse: () => null,
  });

  await assert.rejects(
    () => svc.fetchNormalizedUsage('codex', 'token'),
    /Upstream returned non-JSON response/
  );
});
