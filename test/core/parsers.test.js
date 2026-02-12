const test = require('node:test');
const assert = require('node:assert/strict');

const { parseClaudeUsage, parseCodexUsage } = require('../../src/core/parsers');

test('parseClaudeUsage orders known windows and keeps unknown windows', () => {
  const input = {
    custom_bucket: { utilization: 21, resets_at: '2026-02-13T00:00:00Z' },
    seven_day: { utilization: 87, resets_at: '2026-02-14T00:00:00Z' },
    five_hour: { utilization: 42, resets_at: '2026-02-13T06:00:00Z' },
  };

  const windows = parseClaudeUsage(input);

  assert.equal(windows.length, 3);
  assert.deepEqual(
    windows.map((w) => w.name),
    ['5時間', '7日間', 'custom bucket']
  );
  assert.equal(windows[0].windowSeconds, 18000);
  assert.equal(windows[1].windowSeconds, 604800);
  assert.equal(windows[2].windowSeconds, null);
});

test('parseClaudeUsage returns unknown entry when shape is unsupported', () => {
  const windows = parseClaudeUsage({ foo: { bar: 1 } });

  assert.equal(windows.length, 1);
  assert.equal(windows[0].status, 'unknown');
  assert.equal(windows[0].name, '(不明な形式)');
});

test('parseCodexUsage parses wham structure and forceExhausted flags', () => {
  const input = {
    rate_limit: {
      allowed: true,
      primary_window: {
        used_percent: 50,
        limit_window_seconds: 18000,
        reset_at: '2026-02-13T08:00:00Z',
      },
      secondary_window: {
        used: 90,
        limit: 100,
        limit_window_seconds: 604800,
        reset_at: '2026-02-20T00:00:00Z',
      },
    },
    code_review_rate_limit: {
      limit_reached: true,
      primary_window: {
        used_percent: 10,
        limit_window_seconds: 18000,
      },
      secondary_window: {
        used_percent: 20,
        limit_window_seconds: 604800,
      },
    },
  };

  const windows = parseCodexUsage(input);

  assert.equal(windows.length, 4);
  assert.deepEqual(
    windows.map((w) => w.name),
    ['5時間', '7日間', 'Code Review (primary)', 'Code Review (secondary)']
  );
  assert.equal(windows[0].utilization, 50);
  assert.equal(windows[1].utilization, 90);
  assert.equal(windows[2].forceExhausted, true);
  assert.equal(windows[3].forceExhausted, true);
});

test('parseCodexUsage falls back to legacy rate_limits structure', () => {
  const input = {
    rate_limits: {
      primary: { used: 25, limit: 100, limit_window_seconds: 18000 },
      secondary: { used: 200, limit: 400, limit_window_seconds: 604800 },
    },
  };

  const windows = parseCodexUsage(input);

  assert.equal(windows.length, 2);
  assert.equal(windows[0].name, '5時間');
  assert.equal(windows[0].utilization, 25);
  assert.equal(windows[1].name, '7日間');
  assert.equal(windows[1].utilization, 50);
});

test('parseCodexUsage returns unknown entry for unsupported payload', () => {
  const windows = parseCodexUsage({ hello: 'world' });

  assert.equal(windows.length, 1);
  assert.equal(windows[0].status, 'unknown');
  assert.equal(windows[0].name, '(不明な形式)');
});
