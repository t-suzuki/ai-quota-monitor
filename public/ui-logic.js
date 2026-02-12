(function initUiLogic(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.UiLogic = api;
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function factory() {
  const STATUS_ORDER = ['unknown', 'ok', 'warning', 'critical', 'exhausted'];

  function classifyUtilization(pct, notifySettings, exhaustedThreshold = 100) {
    const value = Number(pct);
    if (!Number.isFinite(value)) return 'ok';
    if (value >= exhaustedThreshold) return 'exhausted';
    if (value >= notifySettings.thresholdCritical) return 'critical';
    if (value >= notifySettings.thresholdWarning) return 'warning';
    return 'ok';
  }

  function classifyWindows(windows, notifySettings, exhaustedThreshold = 100) {
    for (const windowInfo of windows) {
      if (windowInfo.forceExhausted) {
        windowInfo.status = 'exhausted';
        continue;
      }
      windowInfo.status = classifyUtilization(windowInfo.utilization, notifySettings, exhaustedThreshold);
    }
    return windows;
  }

  function deriveServiceStatus(windows) {
    return windows.reduce((current, windowInfo) => {
      return STATUS_ORDER.indexOf(windowInfo.status) > STATUS_ORDER.indexOf(current)
        ? windowInfo.status
        : current;
    }, 'ok');
  }

  function buildTransitionEffects(prev, next, label, windows, notifySettings) {
    if (!prev || prev === next) return { notifications: [], logs: [] };

    const notifications = [];
    const logs = [];
    const detail = windows.map((windowInfo) => `${windowInfo.name}: ${windowInfo.utilization}%`).join(', ');

    if ((next === 'critical' || next === 'exhausted') && notifySettings.critical) {
      notifications.push({
        title: `${label} ⚠️`,
        body: `ステータス: ${next} — ${detail}`,
      });
    }

    if (next === 'warning' && prev !== 'critical' && prev !== 'exhausted' && notifySettings.warning) {
      notifications.push({
        title: `${label} ⚠`,
        body: `ステータス: ${next} — ${detail}`,
      });
    }

    if (next === 'ok' && (prev === 'critical' || prev === 'exhausted') && notifySettings.recovery) {
      notifications.push({
        title: `${label} ✅`,
        body: 'クォータが回復しました',
      });
    }

    if (next === 'critical' || next === 'exhausted') {
      logs.push({ level: 'crit', message: `${label} → ${next}` });
    } else if (next === 'warning' && prev !== 'critical' && prev !== 'exhausted') {
      logs.push({ level: 'warn', message: `${label} → ${next}` });
    } else if (next === 'ok' && (prev === 'critical' || prev === 'exhausted')) {
      logs.push({ level: 'ok', message: `${label} → ok (回復)` });
    }

    return { notifications, logs };
  }

  function deriveTokenInputValue({ hasToken, token, savedTokenMask }) {
    const tokenMasked = Boolean(hasToken && !token);
    return {
      tokenMasked,
      tokenValue: tokenMasked ? savedTokenMask : (token || ''),
    };
  }

  function normalizeAccountToken({ rawToken, tokenMasked, savedTokenMask }) {
    const trimmed = typeof rawToken === 'string' ? rawToken.trim() : '';
    if (tokenMasked && trimmed === savedTokenMask) return '';
    return trimmed;
  }

  function calcElapsedPct(resetsAt, windowSeconds, nowMs = Date.now()) {
    if (!resetsAt || !windowSeconds) return null;
    const resetMs = typeof resetsAt === 'number' ? resetsAt * 1000 : Date.parse(resetsAt);
    if (!Number.isFinite(resetMs)) return null;

    const remainSec = Math.max(0, (resetMs - nowMs) / 1000);
    const elapsedSec = Number(windowSeconds) - remainSec;
    return Math.max(0, Math.min(100, (elapsedSec / Number(windowSeconds)) * 100));
  }

  function computePollingState({ polling, pollStartedAt, pollInterval, nowMs = Date.now() }) {
    if (!polling || !pollStartedAt) return null;
    const intervalSec = Number(pollInterval);
    if (!Number.isFinite(intervalSec) || intervalSec <= 0) return null;

    const elapsedSec = Math.max(0, (nowMs - Number(pollStartedAt)) / 1000);
    const remainingSec = Math.max(0, intervalSec - elapsedSec);
    const fraction = Math.max(0, Math.min(1, remainingSec / intervalSec));
    const color = fraction > 0.3 ? 'var(--ok)' : fraction > 0.1 ? 'var(--warn)' : 'var(--crit)';

    return {
      elapsedSec,
      remainingSec,
      remainingSecLabel: Math.ceil(remainingSec),
      fraction,
      color,
    };
  }

  return {
    STATUS_ORDER,
    classifyUtilization,
    classifyWindows,
    deriveServiceStatus,
    buildTransitionEffects,
    deriveTokenInputValue,
    normalizeAccountToken,
    calcElapsedPct,
    computePollingState,
  };
}));
