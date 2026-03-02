const test = require('node:test');
const assert = require('node:assert/strict');

const {
  classifyUtilization,
  classifyWindows,
  deriveServiceStatus,
  buildTransitionEffects,
  formatResetRemaining,
  buildResetSummary,
  deriveTokenInputValue,
  normalizeAccountToken,
  calcElapsedPct,
  computePollingState,
} = require('../../public/ui-logic.js');

const BASE_NOTIFY = {
  critical: true,
  recovery: true,
  warning: true,
  thresholdWarning: 75,
  thresholdCritical: 90,
};

test('classifyUtilization respects warning/critical/exhausted thresholds', () => {
  assert.equal(classifyUtilization(20, BASE_NOTIFY), 'ok');
  assert.equal(classifyUtilization(80, BASE_NOTIFY), 'warning');
  assert.equal(classifyUtilization(91, BASE_NOTIFY), 'critical');
  assert.equal(classifyUtilization(100, BASE_NOTIFY), 'exhausted');
});

test('classifyWindows prioritizes forceExhausted and updates status in-place', () => {
  const windows = [
    { utilization: 95, forceExhausted: false, status: null },
    { utilization: 10, forceExhausted: true, status: null },
  ];

  classifyWindows(windows, BASE_NOTIFY);

  assert.equal(windows[0].status, 'critical');
  assert.equal(windows[1].status, 'exhausted');
});

test('threshold change alters service status classification', () => {
  const windows = [{ utilization: 80, status: null }, { utilization: 95, status: null }];

  classifyWindows(windows, {
    ...BASE_NOTIFY,
    thresholdWarning: 75,
    thresholdCritical: 90,
  });
  assert.equal(deriveServiceStatus(windows), 'critical');

  classifyWindows(windows, {
    ...BASE_NOTIFY,
    thresholdWarning: 85,
    thresholdCritical: 97,
  });
  assert.equal(deriveServiceStatus(windows), 'warning');
});

test('buildTransitionEffects emits critical notification and log', () => {
  const windows = [{ name: '5時間', utilization: 91 }];

  const effects = buildTransitionEffects('ok', 'critical', 'Claude Code: A', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].title, /⚠️/);
  assert.match(effects.notifications[0].body, /ステータス: critical/);
  assert.deepEqual(effects.logs, [{ level: 'crit', message: 'Claude Code: A → critical' }]);
});

test('buildTransitionEffects includes reset time in critical notification', () => {
  const nowMs = 1000000;
  // resetsAt 2 hours and 30 minutes from now (epoch seconds)
  const resetsAt = (nowMs / 1000) + 2 * 3600 + 30 * 60;
  const windows = [{ name: '5時間', utilization: 95, resetsAt }];

  const effects = buildTransitionEffects('ok', 'critical', 'Claude: A', windows, BASE_NOTIFY, nowMs);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /あと2時間30分でリセット/);
});

test('buildTransitionEffects includes reset time in warning notification', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs / 1000) + 45 * 60; // 45 minutes from now
  const windows = [{ name: '5時間', utilization: 80, resetsAt }];

  const effects = buildTransitionEffects('ok', 'warning', 'Claude: B', windows, BASE_NOTIFY, nowMs);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /あと45分でリセット/);
});

test('buildTransitionEffects does not include reset time in recovery notification', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs / 1000) + 3600;
  const windows = [{ name: '5時間', utilization: 10, resetsAt }];

  const effects = buildTransitionEffects('exhausted', 'ok', 'Codex: C', windows, BASE_NOTIFY, nowMs);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /クォータが回復しました/);
  assert.equal(effects.notifications[0].body.includes('リセット'), false);
});

test('buildTransitionEffects suppresses warning after critical/exhausted', () => {
  const windows = [{ name: '7日間', utilization: 80 }];

  const effects = buildTransitionEffects('critical', 'warning', 'Codex: B', windows, BASE_NOTIFY);

  assert.deepEqual(effects.notifications, []);
  assert.deepEqual(effects.logs, []);
});

test('buildTransitionEffects emits recovery notification and log', () => {
  const windows = [{ name: '5時間', utilization: 10 }];

  const effects = buildTransitionEffects('exhausted', 'ok', 'Codex: C', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].title, /✅/);
  assert.deepEqual(effects.logs, [{ level: 'ok', message: 'Codex: C → ok (回復)' }]);
});

test('buildTransitionEffects returns no effects when unchanged', () => {
  const effects = buildTransitionEffects('warning', 'warning', 'X', [], BASE_NOTIFY);
  assert.deepEqual(effects, { notifications: [], logs: [] });
});

test('deriveTokenInputValue masks saved token without exposing plaintext', () => {
  const masked = deriveTokenInputValue({
    hasToken: true,
    token: '',
    savedTokenMask: '********',
  });
  assert.deepEqual(masked, { tokenMasked: true, tokenValue: '********' });

  const plain = deriveTokenInputValue({
    hasToken: false,
    token: 'abc123',
    savedTokenMask: '********',
  });
  assert.deepEqual(plain, { tokenMasked: false, tokenValue: 'abc123' });
});

test('normalizeAccountToken ignores unchanged masked token value', () => {
  const value = normalizeAccountToken({
    rawToken: '********************',
    tokenMasked: true,
    savedTokenMask: '********************',
  });
  assert.equal(value, '');

  const edited = normalizeAccountToken({
    rawToken: '  sk-live-123  ',
    tokenMasked: false,
    savedTokenMask: '********************',
  });
  assert.equal(edited, 'sk-live-123');
});

test('normalizeAccountToken extracts token when auth JSON is pasted', () => {
  const codexJson = JSON.stringify({
    tokens: { access_token: 'sk-codex-abc' },
  });
  const claudeJson = JSON.stringify({
    claudeAiOauth: { accessToken: 'claude-xyz' },
  });

  const codexToken = normalizeAccountToken({
    rawToken: codexJson,
    tokenMasked: false,
    savedTokenMask: '********************',
  });
  const claudeToken = normalizeAccountToken({
    rawToken: claudeJson,
    tokenMasked: false,
    savedTokenMask: '********************',
  });

  assert.equal(codexToken, 'sk-codex-abc');
  assert.equal(claudeToken, 'claude-xyz');
});

test('formatResetRemaining formats hours and minutes', () => {
  const nowMs = 1000000;
  // 2h 30m from now
  const resetsAt = (nowMs / 1000) + 2 * 3600 + 30 * 60;
  assert.equal(formatResetRemaining(resetsAt, nowMs), 'あと2時間30分でリセット');
});

test('formatResetRemaining formats days when >= 24h', () => {
  const nowMs = 1000000;
  // 1 day 3 hours from now
  const resetsAt = (nowMs / 1000) + 27 * 3600;
  assert.equal(formatResetRemaining(resetsAt, nowMs), 'あと1日3時間でリセット');
});

test('formatResetRemaining formats minutes only when < 1h', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs / 1000) + 15 * 60;
  assert.equal(formatResetRemaining(resetsAt, nowMs), 'あと15分でリセット');
});

test('formatResetRemaining returns reset done when past', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs / 1000) - 60;
  assert.equal(formatResetRemaining(resetsAt, nowMs), 'リセット済み');
});

test('formatResetRemaining returns empty for null/invalid', () => {
  assert.equal(formatResetRemaining(null, Date.now()), '');
  assert.equal(formatResetRemaining('invalid', Date.now()), '');
});

test('formatResetRemaining supports ISO datetime string', () => {
  const nowMs = Date.parse('2026-03-02T10:00:00Z');
  const resetsAt = '2026-03-02T12:30:00Z';
  assert.equal(formatResetRemaining(resetsAt, nowMs), 'あと2時間30分でリセット');
});

test('buildResetSummary combines multiple windows', () => {
  const nowMs = 1000000;
  const windows = [
    { name: '5時間', resetsAt: (nowMs / 1000) + 2 * 3600 },
    { name: '日次', resetsAt: (nowMs / 1000) + 10 * 3600 },
  ];
  const result = buildResetSummary(windows, nowMs);
  assert.match(result, /5時間: あと2時間0分でリセット/);
  assert.match(result, /日次: あと10時間0分でリセット/);
});

test('buildResetSummary returns empty when no resetsAt', () => {
  const windows = [{ name: '5時間', utilization: 80 }];
  assert.equal(buildResetSummary(windows, Date.now()), '');
});

test('calcElapsedPct supports epoch seconds and ISO datetime', () => {
  const nowMs = 55_000;
  const epochBased = calcElapsedPct(100, 100, nowMs);
  assert.ok(Math.abs(epochBased - 55) < 1e-9);

  const isoBased = calcElapsedPct('1970-01-01T00:01:40.000Z', 100, nowMs);
  assert.ok(Math.abs(isoBased - 55) < 1e-9);
});

test('calcElapsedPct returns null for invalid inputs', () => {
  assert.equal(calcElapsedPct(null, 100, Date.now()), null);
  assert.equal(calcElapsedPct('invalid-date', 100, Date.now()), null);
});

test('computePollingState returns progress and display metadata', () => {
  const nowMs = 120_000;
  const timing = computePollingState({
    polling: false,
    pollStartedAt: 10_000,
    pollInterval: 120,
    nowMs,
  });

  assert.equal(timing, null);

  const running = computePollingState({
    polling: true,
    pollStartedAt: 5_000,
    pollInterval: 120,
    nowMs,
  });
  assert.equal(running.remainingSecLabel, 5);
  assert.equal(running.color, 'var(--crit)');
  assert.ok(running.fraction <= 0.1);

  const safe = computePollingState({
    polling: true,
    pollStartedAt: 100_000,
    pollInterval: 120,
    nowMs,
  });
  assert.equal(safe.remainingSecLabel, 100);
  assert.equal(safe.color, 'var(--ok)');
  assert.ok(safe.fraction > 0.3);
});
