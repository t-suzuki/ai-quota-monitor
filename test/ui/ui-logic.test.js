const test = require('node:test');
const assert = require('node:assert/strict');

const {
  classifyUtilization,
  classifyWindows,
  deriveServiceStatus,
  buildTransitionEffects,
  formatResetCompact,
  formatRecoveryWindowNote,
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

test('buildTransitionEffects suppresses warning after critical/exhausted', () => {
  const windows = [{ name: '7日間', utilization: 80 }];

  const effects = buildTransitionEffects('critical', 'warning', 'Codex: B', windows, BASE_NOTIFY);

  assert.deepEqual(effects.notifications, []);
  assert.deepEqual(effects.logs, []);
});

test('buildTransitionEffects emits recovery notification with detail and log', () => {
  const windows = [{ name: '5時間', utilization: 10 }];

  const effects = buildTransitionEffects('exhausted', 'ok', 'Codex: C', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].title, /✅/);
  assert.match(effects.notifications[0].body, /クォータが回復しました — 5時間: 10%/);
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

test('formatResetCompact returns human-readable remaining time', () => {
  const nowMs = 1000000;
  // 2 hours 30 minutes from now (in epoch seconds)
  const resetsAt = (nowMs + 2 * 3600000 + 30 * 60000) / 1000;
  assert.equal(formatResetCompact(resetsAt, nowMs), 'あと2時間30分でリセット');

  // 45 minutes from now
  const soon = (nowMs + 45 * 60000) / 1000;
  assert.equal(formatResetCompact(soon, nowMs), 'あと45分でリセット');

  // 2 days 3 hours from now
  const far = (nowMs + 2 * 86400000 + 3 * 3600000) / 1000;
  assert.equal(formatResetCompact(far, nowMs), 'あと2日3時間でリセット');
});

test('formatResetCompact handles edge cases', () => {
  assert.equal(formatResetCompact(null), '');
  assert.equal(formatResetCompact(undefined), '');
  assert.equal(formatResetCompact('invalid-date'), '');
  // Past reset time
  const nowMs = 1000000;
  const past = (nowMs - 60000) / 1000;
  assert.equal(formatResetCompact(past, nowMs), 'リセット済み');
});

test('formatResetCompact supports ISO datetime string', () => {
  const nowMs = Date.parse('2026-03-02T12:00:00Z');
  assert.equal(
    formatResetCompact('2026-03-02T14:30:00Z', nowMs),
    'あと2時間30分でリセット',
  );
});

test('buildTransitionEffects includes reset time in critical notification', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs + 2 * 3600000 + 30 * 60000) / 1000;
  const windows = [{ name: '5時間', utilization: 95, resetsAt }];

  const effects = buildTransitionEffects('ok', 'critical', 'Claude: A', windows, BASE_NOTIFY, nowMs);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /5時間: 95% \(あと2時間30分でリセット\)/);
});

test('buildTransitionEffects includes reset time in recovery notification', () => {
  const nowMs = 1000000;
  const resetsAt = (nowMs + 3 * 3600000) / 1000;
  const windows = [{ name: '5時間', utilization: 10, resetsAt }];

  const effects = buildTransitionEffects('exhausted', 'ok', 'Codex: B', windows, BASE_NOTIFY, nowMs);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /クォータが回復しました — 5時間: 10% \(あと3時間0分でリセット\)/);
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

test('formatRecoveryWindowNote returns recovery info for 5-hour window', () => {
  const windows = [{ windowSeconds: 18000 }];
  assert.equal(formatRecoveryWindowNote(windows), '\n使用量は次回使用開始から5時間で回復します');
});

test('formatRecoveryWindowNote picks shortest window when multiple exist', () => {
  const windows = [{ windowSeconds: 604800 }, { windowSeconds: 18000 }];
  assert.equal(formatRecoveryWindowNote(windows), '\n使用量は次回使用開始から5時間で回復します');
});

test('formatRecoveryWindowNote returns empty string when no windowSeconds', () => {
  const windows = [{ name: '5時間', utilization: 91 }];
  assert.equal(formatRecoveryWindowNote(windows), '');
});

test('buildTransitionEffects includes recovery note in critical notification with windowSeconds', () => {
  const windows = [{ name: '5時間', utilization: 95, windowSeconds: 18000 }];
  const effects = buildTransitionEffects('ok', 'critical', 'Claude: A', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /ステータス: critical/);
  assert.match(effects.notifications[0].body, /使用量は次回使用開始から5時間で回復します/);
});

test('buildTransitionEffects includes recovery note in warning notification with windowSeconds', () => {
  const windows = [{ name: '5時間', utilization: 80, windowSeconds: 18000 }];
  const effects = buildTransitionEffects('ok', 'warning', 'Claude: A', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /ステータス: warning/);
  assert.match(effects.notifications[0].body, /使用量は次回使用開始から5時間で回復します/);
});

test('buildTransitionEffects does NOT include recovery note in recovery notification', () => {
  const windows = [{ name: '5時間', utilization: 10, windowSeconds: 18000 }];
  const effects = buildTransitionEffects('exhausted', 'ok', 'Codex: C', windows, BASE_NOTIFY);

  assert.equal(effects.notifications.length, 1);
  assert.match(effects.notifications[0].body, /クォータが回復しました/);
  assert.ok(!effects.notifications[0].body.includes('次回使用開始から'));
});
