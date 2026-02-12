const THRESHOLDS = { warning: 75, critical: 90, exhausted: 100 };

function classifyUtilization(pct) {
  if (pct >= THRESHOLDS.exhausted) return 'exhausted';
  if (pct >= THRESHOLDS.critical) return 'critical';
  if (pct >= THRESHOLDS.warning) return 'warning';
  return 'ok';
}

function parseClaudeUsage(data) {
  const windows = [];
  const labels = {
    five_hour: '5時間',
    seven_day: '7日間',
    seven_day_opus: '7日間 (Opus)',
    seven_day_sonnet: '7日間 (Sonnet)',
    seven_day_oauth_apps: '7日間 (OAuth Apps)',
    seven_day_cowork: '7日間 (Cowork)',
  };
  const winSecMap = {
    five_hour: 18000,
    seven_day: 604800,
    seven_day_opus: 604800,
    seven_day_sonnet: 604800,
    seven_day_oauth_apps: 604800,
    seven_day_cowork: 604800,
  };
  const preferredOrder = Object.keys(labels);
  const pushed = new Set();

  const pushWindow = (key, label) => {
    const w = data?.[key];
    if (!w || typeof w !== 'object') return;
    if (typeof w.utilization !== 'number') return;
    windows.push({
      name: label,
      utilization: w.utilization,
      resetsAt: w.resets_at || null,
      status: classifyUtilization(w.utilization),
      windowSeconds: winSecMap[key] || (key.startsWith('seven_day') ? 604800 : key.includes('hour') ? 18000 : null),
    });
    pushed.add(key);
  };

  for (const key of preferredOrder) {
    pushWindow(key, labels[key]);
  }
  for (const [key, value] of Object.entries(data || {})) {
    if (pushed.has(key)) continue;
    if (!value || typeof value !== 'object') continue;
    if (typeof value.utilization !== 'number') continue;
    pushWindow(key, key.replaceAll('_', ' '));
  }

  if (windows.length === 0) {
    windows.push({ name: '(不明な形式)', utilization: 0, resetsAt: null, status: 'unknown', windowSeconds: null });
  }
  return windows;
}

function toNumber(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function normalizeWindowName(seconds, fallback) {
  if (fallback) return fallback;
  const sec = toNumber(seconds);
  if (sec === 18000) return '5時間';
  if (sec === 604800) return '7日間';
  if (sec === 86400) return '24時間';
  if (!sec || sec <= 0) return 'ウィンドウ';
  if (sec % 86400 === 0) return `${Math.round(sec / 86400)}日間`;
  return `${Math.round(sec / 3600)}時間`;
}

function parseCodexUsage(data) {
  const windows = [];

  const pushWindow = (windowData, label, parent) => {
    if (!windowData || typeof windowData !== 'object') return;
    const utilization =
      toNumber(windowData.used_percent ?? windowData.usedPercent ?? windowData.utilization) ??
      (() => {
        const used = toNumber(windowData.used);
        const limit = toNumber(windowData.limit);
        if (used !== null && limit && limit > 0) return (used / limit) * 100;
        return 0;
      })();

    const limitReached = windowData.limit_reached ?? windowData.limitReached ?? parent?.limit_reached ?? parent?.limitReached;
    const allowed = windowData.allowed ?? parent?.allowed;
    let status = classifyUtilization(utilization);
    if (limitReached === true || allowed === false) status = 'exhausted';
    const windowSeconds = toNumber(windowData.limit_window_seconds ?? windowData.limitWindowSeconds) || null;

    windows.push({
      name: normalizeWindowName(windowSeconds, label),
      utilization,
      resetsAt: windowData.reset_at ?? windowData.resetAt ?? windowData.resets_at ?? windowData.resetsAt ?? null,
      status,
      windowSeconds,
    });
  };

  const parseWhamRateLimit = (block, prefix) => {
    if (!block || typeof block !== 'object') return;
    const primary = block.primary_window ?? block.primaryWindow ?? block.primary;
    const secondary = block.secondary_window ?? block.secondaryWindow ?? block.secondary;
    pushWindow(primary, prefix ? `${prefix} (primary)` : null, block);
    pushWindow(secondary, prefix ? `${prefix} (secondary)` : null, block);
  };

  parseWhamRateLimit(data?.rate_limit, null);
  parseWhamRateLimit(data?.code_review_rate_limit, 'Code Review');

  if (Array.isArray(data?.additional_rate_limits)) {
    for (const [idx, block] of data.additional_rate_limits.entries()) {
      parseWhamRateLimit(block, block?.name || `Additional ${idx + 1}`);
    }
  } else {
    parseWhamRateLimit(data?.additional_rate_limits, 'Additional');
  }

  if (windows.length === 0) {
    const rl = data?.rate_limits || data?.rateLimits || data || {};
    if (rl.primary || rl.secondary) {
      pushWindow(rl.primary, '5時間', rl);
      pushWindow(rl.secondary, '7日間', rl);
    }
  }

  if (windows.length === 0) {
    const arr = data?.windows || data?.limits || data?.rate_limits;
    if (Array.isArray(arr)) {
      for (const w of arr) {
        pushWindow(w, w.name || w.label || w.window || null, null);
      }
    }
  }

  if (windows.length === 0) {
    for (const [key, name] of [
      ['five_hour', '5時間'],
      ['fiveHour', '5時間'],
      ['weekly', '7日間'],
      ['seven_day', '7日間'],
    ]) {
      if (data?.[key] && typeof data[key] === 'object') {
        pushWindow(data[key], name, data[key]);
      }
    }
  }

  if (windows.length === 0) {
    windows.push({ name: '(不明な形式)', utilization: 0, resetsAt: null, status: 'unknown', windowSeconds: null });
  }

  return windows;
}

function worstStatus(windows) {
  const statusOrder = ['unknown', 'ok', 'warning', 'critical', 'exhausted'];
  return (windows || []).reduce((acc, w) => {
    return statusOrder.indexOf(w.status) > statusOrder.indexOf(acc) ? w.status : acc;
  }, 'ok');
}

module.exports = {
  THRESHOLDS,
  classifyUtilization,
  parseClaudeUsage,
  parseCodexUsage,
  worstStatus,
};
