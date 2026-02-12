// ═══════════════════════════════════════
// State
// ═══════════════════════════════════════
const state = {
  polling: false,
  timer: null,
  pollStartedAt: null,
  pollInterval: 120,
  ringTimer: null,
  windowMode: 'normal',
  hasSavedMinimalBounds: false,
  accounts: { claude: [], codex: [] }, // [{ id, name, token }]
  services: {},   // { id: { label, windows: [{name, utilization, resetsAt}], status, lastRaw } }
  logs: [],
  rawResponses: {},
  history: {},    // { 'serviceKey:windowName': [util1, util2, ...] }
};

const THRESHOLDS = { warning: 75, critical: 90, exhausted: 100 };
const IS_ELECTRON = Boolean(window.quotaApi && window.quotaApi.platform === 'electron');
const SESSION_KEYS = {
  accounts: 'qm-accounts',
  interval: 'qm-interval',
  services: 'qm-services',
  raw: 'qm-raw',
  history: 'qm-history',
  fetchedAt: 'qm-fetched-at',
  pollState: 'qm-poll-state',
  legacyClaude: 'qm-claude',
  legacyCodex: 'qm-codex',
};
const SERVICE_META = {
  claude: {
    label: 'Claude Code', listId: '#claude-accounts', addBtnId: '#btn-add-claude',
    icon: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="8" y1="1" x2="8" y2="15"/><line x1="1.9" y1="4.5" x2="14.1" y2="11.5"/><line x1="14.1" y1="4.5" x2="1.9" y2="11.5"/></svg>',
  },
  codex: {
    label: 'Codex', listId: '#codex-accounts', addBtnId: '#btn-add-codex',
    icon: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M8 1l6.1 3.5v7L8 15l-6.1-3.5v-7z"/><path d="M8 5l3 1.75v3.5L8 12l-3-1.75v-3.5z"/></svg>',
  },
};

// ═══════════════════════════════════════
// Copy-to-clipboard for command snippets
// ═══════════════════════════════════════
document.addEventListener('click', e => {
  const btn = e.target.closest('.cmd-copy');
  if (!btn) return;
  const code = btn.parentElement.querySelector('.cmd-code').value;
  navigator.clipboard.writeText(code).then(() => {
    btn.textContent = '✓';
    btn.classList.add('copied');
    setTimeout(() => { btn.textContent = '⧉'; btn.classList.remove('copied'); }, 1500);
  });
});

// ═══════════════════════════════════════
// Utils
// ═══════════════════════════════════════
const $ = s => document.querySelector(s);
const log = (msg, level = '') => {
  const ts = new Date().toLocaleTimeString();
  state.logs.unshift({ ts, msg, level });
  if (state.logs.length > 200) state.logs.length = 200;
  renderLogs();
};

function classifyUtilization(pct) {
  if (pct >= THRESHOLDS.exhausted) return 'exhausted';
  if (pct >= THRESHOLDS.critical) return 'critical';
  if (pct >= THRESHOLDS.warning) return 'warning';
  return 'ok';
}

function formatReset(epoch) {
  if (!epoch) return '';
  const d = typeof epoch === 'number' ? new Date(epoch * 1000) : new Date(epoch);
  if (isNaN(d)) return '';
  const now = new Date();
  const diff = d - now;
  if (diff <= 0) return 'リセット済み';
  const h = Math.floor(diff / 3600000);
  const m = Math.floor((diff % 3600000) / 60000);
  const dateStr = d.toLocaleDateString('ja-JP', { month: 'short', day: 'numeric' });
  const timeStr = d.toLocaleTimeString('ja-JP', { hour: '2-digit', minute: '2-digit' });
  if (h >= 24) return `${dateStr} ${timeStr} (あと${Math.floor(h/24)}日${h%24}時間)`;
  if (h > 0) return `${timeStr} (あと${h}時間${m}分)`;
  return `${timeStr} (あと${m}分)`;
}

function notify(title, body) {
  if (Notification.permission === 'granted') {
    new Notification(title, { body, icon: '⚡' });
  }
}

function makeAccountId(service) {
  return `${service}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function defaultAccount(service, idx = 0) {
  const base = SERVICE_META[service].label;
  return { id: makeAccountId(service), name: `${base} ${idx + 1}`, token: '', hasToken: false };
}

function accountFromRow(row) {
  const id = row.dataset.accountId || '';
  const name = row.querySelector('.account-name')?.value?.trim() || '';
  const token = row.querySelector('.account-token')?.value?.trim() || '';
  const hasToken = row.dataset.hasToken === '1';
  return { id, name, token, hasToken };
}

function readAccountsFromDom(service) {
  const list = $(SERVICE_META[service].listId);
  const rows = Array.from(list.querySelectorAll('.account-row'));
  return rows.map(accountFromRow);
}

function writeAccountsToDom(service, accounts) {
  const list = $(SERVICE_META[service].listId);
  list.innerHTML = '';
  for (const acc of accounts) {
    const row = document.createElement('div');
    row.className = 'account-row';
    row.dataset.accountId = acc.id || makeAccountId(service);
    row.dataset.hasToken = acc.hasToken ? '1' : '0';
    const tokenPlaceholder = IS_ELECTRON && acc.hasToken
      ? '保存済み (入力すると更新)'
      : 'eyJhbG... / sk-...';
    row.innerHTML = `
      <input class="account-name" type="text" placeholder="表示名" value="${escHtml(acc.name || '')}">
      <input class="account-token" type="text" placeholder="${escHtml(tokenPlaceholder)}" value="${escHtml(acc.token || '')}">
      <button class="btn-mini btn-remove-account" type="button">削除</button>
    `;
    row.querySelector('.btn-remove-account').addEventListener('click', async () => {
      const removed = accountFromRow(row);
      if (IS_ELECTRON && removed.id) {
        try {
          await window.quotaApi.deleteAccount({ service, id: removed.id });
        } catch (e) {
          log(`削除失敗: ${SERVICE_META[service].label} ${removed.name || removed.id} (${e.message || e})`, 'warn');
        }
      }
      row.remove();
      if (!list.querySelector('.account-row')) addAccountRow(service);
      queuePersistSetup();
    });
    row.querySelector('.account-name').addEventListener('input', queuePersistSetup);
    row.querySelector('.account-token').addEventListener('input', () => {
      row.dataset.hasToken = row.querySelector('.account-token')?.value?.trim() ? '0' : row.dataset.hasToken;
      queuePersistSetup();
    });
    list.appendChild(row);
  }
}

function addAccountRow(service, account = null) {
  const existing = readAccountsFromDom(service);
  const next = account || defaultAccount(service, existing.length);
  writeAccountsToDom(service, [...existing, next]);
  queuePersistSetup();
}

function collectAccounts() {
  const collected = {};
  for (const service of Object.keys(SERVICE_META)) {
    const rows = readAccountsFromDom(service);
    collected[service] = rows.map((acc, idx) => ({
      id: acc.id || makeAccountId(service),
      name: acc.name || `${SERVICE_META[service].label} ${idx + 1}`,
      token: acc.token || '',
      hasToken: Boolean(acc.hasToken),
    }));
  }
  return collected;
}

function scrubAccountsForSession(accounts) {
  if (!IS_ELECTRON) return accounts;
  return {
    claude: (accounts.claude || []).map((x) => ({ id: x.id, name: x.name, hasToken: Boolean(x.hasToken) })),
    codex: (accounts.codex || []).map((x) => ({ id: x.id, name: x.name, hasToken: Boolean(x.hasToken) })),
  };
}

function upsertDomTokenState(service, id, hasToken) {
  const list = $(SERVICE_META[service].listId);
  const row = Array.from(list.querySelectorAll('.account-row'))
    .find((candidate) => candidate.dataset.accountId === id);
  if (!row) return;
  row.dataset.hasToken = hasToken ? '1' : '0';
  const tokenInput = row.querySelector('.account-token');
  if (tokenInput) {
    tokenInput.placeholder = hasToken ? '保存済み (入力すると更新)' : 'eyJhbG... / sk-...';
  }
}

let didLogPersistError = false;
let didLogPollingPersistError = false;
let didLogWindowMoveError = false;
const minimalDragState = {
  active: false,
  offsetX: 0,
  offsetY: 0,
};
async function persistSetup() {
  try {
    const accounts = collectAccounts();
    const interval = Math.max(30, parseInt($('#poll-interval').value, 10) || 120);

    if (IS_ELECTRON) {
      for (const service of Object.keys(SERVICE_META)) {
        for (const acc of accounts[service]) {
          const saved = await window.quotaApi.saveAccount({
            service,
            id: acc.id,
            name: acc.name,
            token: acc.token || undefined,
          });
          acc.hasToken = Boolean(saved.hasToken);
          upsertDomTokenState(service, acc.id, acc.hasToken);
        }
      }
      await window.quotaApi.setSettings({ pollInterval: interval });
    }

    sessionStorage.setItem(SESSION_KEYS.accounts, JSON.stringify(scrubAccountsForSession(accounts)));
    sessionStorage.setItem(SESSION_KEYS.interval, String(interval));
    didLogPersistError = false;
  } catch (e) {
    if (!didLogPersistError) {
      log(`設定保存エラー: ${e.message || e}`, 'warn');
      didLogPersistError = true;
    }
  }
}

function persistLastData() {
  try {
    sessionStorage.setItem(SESSION_KEYS.services, JSON.stringify(state.services));
    sessionStorage.setItem(SESSION_KEYS.raw, JSON.stringify(state.rawResponses));
    sessionStorage.setItem(SESSION_KEYS.history, JSON.stringify(state.history));
    sessionStorage.setItem(SESSION_KEYS.fetchedAt, new Date().toISOString());
  } catch {}
}

function persistPollingState() {
  const payload = {
    active: Boolean(state.polling),
    interval: state.pollInterval,
  };

  if (IS_ELECTRON) {
    window.quotaApi.setPollingState(payload).then(() => {
      didLogPollingPersistError = false;
    }).catch((e) => {
      if (!didLogPollingPersistError) {
        log(`ポーリング状態保存エラー: ${e.message || e}`, 'warn');
        didLogPollingPersistError = true;
      }
    });
    return;
  }

  try {
    if (!payload.active) {
      sessionStorage.removeItem(SESSION_KEYS.pollState);
      return;
    }
    sessionStorage.setItem(SESSION_KEYS.pollState, JSON.stringify({
      active: true,
      interval: payload.interval,
    }));
  } catch {}
}

let persistTimer = null;
function queuePersistSetup() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistSetup();
  }, 200);
}

// ═══════════════════════════════════════
// API proxy base URL detection
// ═══════════════════════════════════════
function apiBase() {
  // Works both in Vercel deployment and local dev
  return window.location.origin;
}

// ═══════════════════════════════════════
// Claude Code fetcher
// ═══════════════════════════════════════
async function fetchClaude(token) {
  if (IS_ELECTRON) throw new Error('fetchClaude(token) should not run in Electron mode');
  const resp = await fetch(`${apiBase()}/api/claude`, {
    headers: { 'x-quota-token': token },
  });
  const data = await resp.json();
  if (!resp.ok) throw new Error(data.error || `HTTP ${resp.status}`);
  return data;
}

function parseClaude(data) {
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
    seven_day: 604800, seven_day_opus: 604800, seven_day_sonnet: 604800,
    seven_day_oauth_apps: 604800, seven_day_cowork: 604800,
  };
  const preferredOrder = Object.keys(labels);
  const pushed = new Set();
  const pushWindow = (key, name) => {
    const w = data[key];
    if (!w || typeof w !== 'object') return;
    if (typeof w.utilization !== 'number') return;
    windows.push({
      name,
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
  for (const [key, value] of Object.entries(data)) {
    if (pushed.has(key)) continue;
    if (!value || typeof value !== 'object') continue;
    if (typeof value.utilization !== 'number') continue;
    const name = key.replaceAll('_', ' ');
    pushWindow(key, name);
  }
  return windows;
}

// ═══════════════════════════════════════
// Codex fetcher
// ═══════════════════════════════════════
async function fetchCodex(token) {
  if (IS_ELECTRON) throw new Error('fetchCodex(token) should not run in Electron mode');
  const resp = await fetch(`${apiBase()}/api/codex`, {
    headers: { 'x-quota-token': token },
  });

  // Capture upstream headers for debugging
  const upstreamHeaders = {};
  for (const [k, v] of resp.headers.entries()) {
    if (k.startsWith('x-upstream-')) upstreamHeaders[k] = v;
  }

  const text = await resp.text();
  let data;
  try { data = JSON.parse(text); } catch { data = { _raw: text }; }
  data._upstreamHeaders = upstreamHeaders;

  if (!resp.ok) throw new Error(data.error || data.detail || `HTTP ${resp.status}`);
  return data;
}

function parseCodex(data) {
  const windows = [];
  const toNumber = (v) => {
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  };
  const normalizeWindowName = (seconds, fallback) => {
    if (fallback) return fallback;
    const sec = toNumber(seconds);
    if (sec === 18000) return '5時間';
    if (sec === 604800) return '7日間';
    if (sec === 86400) return '24時間';
    if (!sec || sec <= 0) return 'ウィンドウ';
    if (sec % 86400 === 0) return `${Math.round(sec / 86400)}日間`;
    return `${Math.round(sec / 3600)}時間`;
  };
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
    const ws = toNumber(windowData.limit_window_seconds ?? windowData.limitWindowSeconds) || null;
    windows.push({
      name: normalizeWindowName(ws, label),
      utilization,
      resetsAt: windowData.reset_at ?? windowData.resetAt ?? windowData.resets_at ?? windowData.resetsAt ?? null,
      status,
      windowSeconds: ws,
    });
  };
  const parseWhamRateLimit = (block, prefix) => {
    if (!block || typeof block !== 'object') return;
    const primary = block.primary_window ?? block.primaryWindow ?? block.primary;
    const secondary = block.secondary_window ?? block.secondaryWindow ?? block.secondary;
    const primaryLabel = prefix ? `${prefix} (primary)` : null;
    const secondaryLabel = prefix ? `${prefix} (secondary)` : null;
    pushWindow(primary, primaryLabel, block);
    pushWindow(secondary, secondaryLabel, block);
  };

  // Current wham/usage shape:
  // {
  //   rate_limit: { primary_window, secondary_window, allowed, limit_reached },
  //   code_review_rate_limit: { primary_window, ... },
  //   additional_rate_limits: [...]
  // }
  parseWhamRateLimit(data.rate_limit, null);
  parseWhamRateLimit(data.code_review_rate_limit, 'Code Review');
  if (Array.isArray(data.additional_rate_limits)) {
    for (const [idx, block] of data.additional_rate_limits.entries()) {
      parseWhamRateLimit(block, block?.name || `Additional ${idx + 1}`);
    }
  } else {
    parseWhamRateLimit(data.additional_rate_limits, 'Additional');
  }

  // Backward-compatible fallback parsers
  if (windows.length === 0) {
    const rl = data.rate_limits || data.rateLimits || data;
    if (rl.primary || rl.secondary) {
      pushWindow(rl.primary, '5時間', rl);
      pushWindow(rl.secondary, '7日間', rl);
    }
  }
  if (windows.length === 0) {
    const arr = data.windows || data.limits || data.rate_limits;
    if (Array.isArray(arr)) {
      for (const w of arr) {
        pushWindow(w, w.name || w.label || w.window || null, null);
      }
    }
  }
  if (windows.length === 0) {
    for (const [key, name] of [['five_hour', '5時間'], ['fiveHour', '5時間'], ['weekly', '7日間'], ['seven_day', '7日間']]) {
      if (data[key] && typeof data[key] === 'object') {
        pushWindow(data[key], name, data[key]);
      }
    }
  }

  if (windows.length === 0) {
    windows.push({ name: '(不明な形式)', utilization: 0, resetsAt: null, status: 'unknown' });
  }

  return windows;
}

// ═══════════════════════════════════════
// Poll orchestrator
// ═══════════════════════════════════════
async function pollAll() {
  state.accounts = collectAccounts();
  queuePersistSetup();
  let anySuccess = false;
  const nextServices = {};
  state.rawResponses = {};
  const statusOrder = ['unknown', 'ok', 'warning', 'critical', 'exhausted'];
  const worstOf = (windows) => windows.reduce((a, w) => (
    statusOrder.indexOf(w.status) > statusOrder.indexOf(a) ? w.status : a
  ), 'ok');
  const hasUsableToken = (acc) => IS_ELECTRON ? (acc.hasToken || Boolean(acc.token)) : Boolean(acc.token);

  for (const acc of state.accounts.claude) {
    if (!hasUsableToken(acc)) continue;
    const serviceKey = `claude:${acc.id}`;
    const label = `${SERVICE_META.claude.label}: ${acc.name}`;
    try {
      let data;
      let windows;
      let worstStatus;
      if (IS_ELECTRON) {
        const result = await window.quotaApi.fetchUsage({
          service: 'claude',
          id: acc.id,
          name: acc.name,
          token: acc.token || undefined,
        });
        data = result.raw;
        windows = result.windows;
        worstStatus = result.status || worstOf(windows);
      } else {
        data = await fetchClaude(acc.token);
        windows = parseClaude(data);
        worstStatus = worstOf(windows);
      }
      state.rawResponses[serviceKey] = data;
      for (const w of windows) { recordHistory(`${serviceKey}:${w.name}`, w.utilization); }

      const prev = state.services[serviceKey]?.status;
      nextServices[serviceKey] = { label, windows, status: worstStatus };

      if (prev && prev !== worstStatus) {
        if (worstStatus === 'critical' || worstStatus === 'exhausted') {
          notify(`${label} ⚠️`, `ステータス: ${worstStatus} — ${windows.map(w => `${w.name}: ${w.utilization}%`).join(', ')}`);
          log(`${label} → ${worstStatus}`, 'crit');
        } else if (worstStatus === 'ok' && (prev === 'critical' || prev === 'exhausted')) {
          notify(`${label} ✅`, 'クォータが回復しました');
          log(`${label} → ok (回復)`, 'ok');
        }
      }
      anySuccess = true;
      log(`${label} 取得成功: ${windows.map(w => `${w.name}=${w.utilization}%`).join(', ')}`);
      if (IS_ELECTRON) upsertDomTokenState('claude', acc.id, true);
    } catch (e) {
      nextServices[serviceKey] = { label, windows: [], status: 'error', error: e.message };
      log(`${label} エラー: ${e.message}`, 'warn');
    }
  }

  for (const acc of state.accounts.codex) {
    if (!hasUsableToken(acc)) continue;
    const serviceKey = `codex:${acc.id}`;
    const label = `${SERVICE_META.codex.label}: ${acc.name}`;
    try {
      let data;
      let windows;
      let worstStatus;
      if (IS_ELECTRON) {
        const result = await window.quotaApi.fetchUsage({
          service: 'codex',
          id: acc.id,
          name: acc.name,
          token: acc.token || undefined,
        });
        data = result.raw;
        windows = result.windows;
        worstStatus = result.status || worstOf(windows);
      } else {
        data = await fetchCodex(acc.token);
        windows = parseCodex(data);
        worstStatus = worstOf(windows);
      }
      state.rawResponses[serviceKey] = data;
      for (const w of windows) { recordHistory(`${serviceKey}:${w.name}`, w.utilization); }

      const prev = state.services[serviceKey]?.status;
      nextServices[serviceKey] = { label, windows, status: worstStatus };

      if (prev && prev !== worstStatus) {
        if (worstStatus === 'critical' || worstStatus === 'exhausted') {
          notify(`${label} ⚠️`, `ステータス: ${worstStatus}`);
          log(`${label} → ${worstStatus}`, 'crit');
        } else if (worstStatus === 'ok' && (prev === 'critical' || prev === 'exhausted')) {
          notify(`${label} ✅`, 'クォータが回復しました');
          log(`${label} → ok (回復)`, 'ok');
        }
      }
      anySuccess = true;
      log(`${label} 取得成功: ${windows.map(w => `${w.name}=${w.utilization}%`).join(', ')}`);
      if (IS_ELECTRON) upsertDomTokenState('codex', acc.id, true);
    } catch (e) {
      nextServices[serviceKey] = { label, windows: [], status: 'error', error: e.message };
      log(`${label} エラー: ${e.message}`, 'warn');
    }
  }

  state.services = nextServices;

  const allAccounts = [...state.accounts.claude, ...state.accounts.codex];
  const withToken = allAccounts.filter((a) => hasUsableToken(a));
  if (withToken.length === 0) {
    log('トークンが未設定', 'warn');
    ensureSetupOpenIfMissingToken(state.accounts);
  }

  render();
  persistLastData();
  return anySuccess;
}

// ═══════════════════════════════════════
// History & bar helpers
// ═══════════════════════════════════════
function recordHistory(key, utilization) {
  if (!state.history[key]) state.history[key] = [];
  const arr = state.history[key];
  // If utilization dropped significantly (quota reset), clear history
  if (arr.length > 0 && utilization < arr[arr.length - 1] - 5) arr.length = 0;
  arr.push(utilization);
  if (arr.length > 10) arr.shift();
}

const STATUS_RGB = {
  ok: '126,231,135', warning: '240,208,80',
  critical: '255,161,152', exhausted: '255,161,152',
};

function buildBarGradient(history, status) {
  const rgb = STATUS_RGB[status] || STATUS_RGB.ok;
  if (!history || history.length <= 1) return '';
  const total = history[history.length - 1];
  if (total <= 0) return '';
  const stops = [];
  const n = history.length;
  for (let i = 0; i < n; i++) {
    const segStart = i === 0 ? 0 : history[i - 1];
    const segEnd = history[i];
    if (segEnd <= segStart) continue;
    const alpha = (0.6 + 0.4 * (i / Math.max(n - 1, 1))).toFixed(2);
    const pS = ((segStart / total) * 100).toFixed(1);
    const pE = ((segEnd / total) * 100).toFixed(1);
    stops.push(`rgba(${rgb},${alpha}) ${pS}%`, `rgba(${rgb},${alpha}) ${pE}%`);
  }
  if (stops.length === 0) return '';
  return `background:linear-gradient(to right,${stops.join(',')})`;
}

function calcElapsedPct(resetsAt, windowSeconds) {
  if (!resetsAt || !windowSeconds) return null;
  const d = typeof resetsAt === 'number' ? new Date(resetsAt * 1000) : new Date(resetsAt);
  if (isNaN(d)) return null;
  const remainSec = Math.max(0, (d - new Date()) / 1000);
  const elapsedSec = windowSeconds - remainSec;
  return Math.max(0, Math.min(100, (elapsedSec / windowSeconds) * 100));
}

// ═══════════════════════════════════════
// Polling countdown ring
// ═══════════════════════════════════════
function updatePollRing() {
  const el = $('#poll-ring');
  if (!el) return;
  if (!state.polling || !state.pollStartedAt) { el.innerHTML = ''; return; }
  const elapsed = (Date.now() - state.pollStartedAt) / 1000;
  const remaining = Math.max(0, state.pollInterval - elapsed);
  const fraction = remaining / state.pollInterval;
  const R = 14, C = 2 * Math.PI * R;
  const offset = C * (1 - fraction);
  const color = fraction > 0.3 ? 'var(--ok)' : fraction > 0.1 ? 'var(--warn)' : 'var(--crit)';
  el.innerHTML = `<svg viewBox="0 0 36 36" width="32" height="32">
    <circle cx="18" cy="18" r="${R}" fill="none" stroke="var(--bg3)" stroke-width="2.5"/>
    <circle cx="18" cy="18" r="${R}" fill="none" stroke="${color}" stroke-width="2.5"
      stroke-dasharray="${C.toFixed(2)}" stroke-dashoffset="-${offset.toFixed(2)}"
      stroke-linecap="round" transform="rotate(-90 18 18)"/>
  </svg>`;
}

// ═══════════════════════════════════════
// Render
// ═══════════════════════════════════════
function render() {
  const dash = $('#dashboard');
  const entries = Object.entries(state.services);

  if (entries.length === 0) {
    dash.innerHTML = '<div class="empty">トークンを設定して「開始」を押してください</div>';
    return;
  }

  dash.innerHTML = entries.map(([id, svc]) => {
    const serviceType = id.split(':')[0];
    const meta = SERVICE_META[serviceType];
    const logoHtml = meta?.icon ? `<span class="card-logo">${meta.icon}</span>` : '';

    if (svc.error && svc.windows.length === 0) {
      return `<div class="card">
        <div class="card-header">
          <span class="card-header-left">${logoHtml}<span class="card-label">${svc.label}</span></span>
          <span class="card-status error">エラー</span>
        </div>
        <div style="font-size:.72rem;color:var(--crit)">${escHtml(svc.error)}</div>
      </div>`;
    }

    const windowsHtml = svc.windows.map(w => {
      const histKey = `${id}:${w.name}`;
      const hist = state.history[histKey] || [];
      const grad = buildBarGradient(hist, w.status);
      const barStyle = grad ? `${grad};width:${Math.min(w.utilization, 100)}%`
                            : `width:${Math.min(w.utilization, 100)}%`;
      const elPct = calcElapsedPct(w.resetsAt, w.windowSeconds);
      const elStr = elPct !== null ? ` / 経過 ${elPct.toFixed(0)}%` : '';
      const elBar = elPct !== null
        ? `<div class="bar-track bar-track-elapsed"><div class="bar-fill elapsed" style="width:${Math.min(elPct, 100)}%"></div></div>`
        : '';
      return `<div class="window">
        <div class="window-header">
          <span>${w.name}</span>
          <span>使用 ${w.utilization.toFixed(1)}%${elStr}</span>
        </div>
        <div class="bar-track">
          <div class="bar-fill ${w.status}" style="${barStyle}"></div>
        </div>
        ${elBar}
        ${w.resetsAt ? `<div class="reset-info">リセット: ${formatReset(w.resetsAt)}</div>` : ''}
      </div>`;
    }).join('');

    return `<div class="card">
      <div class="card-header">
        <span class="card-header-left">${logoHtml}<span class="card-label">${svc.label}</span></span>
        <span class="card-status ${svc.status}">${svc.status}</span>
      </div>
      ${windowsHtml}
    </div>`;
  }).join('');

  // Raw data
  $('#raw-data').textContent = JSON.stringify(state.rawResponses, null, 2);
}

function renderLogs() {
  const el = $('#log-list');
  el.innerHTML = state.logs.slice(0, 50).map(l =>
    `<div class="${l.level}">[${l.ts}] ${escHtml(l.msg)}</div>`
  ).join('');
}

function escHtml(s) {
  return String(s)
    .replace(/&/g,'&amp;')
    .replace(/</g,'&lt;')
    .replace(/>/g,'&gt;')
    .replace(/"/g,'&quot;')
    .replace(/'/g,'&#39;');
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function applyMinimalModeUI(isMinimal) {
  document.body.classList.toggle('minimal-mode', Boolean(isMinimal));
}

function isInteractiveUiTarget(target) {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest('button,input,textarea,select,summary,details,label,a,pre,.cmd-copy,.cmd-code'));
}

function computeMinimalWindowMetrics() {
  const container = document.querySelector('.container');
  const dashboard = $('#dashboard');
  const cards = Array.from(dashboard?.querySelectorAll('.card') || []);
  const fallbackCardWidth = 420;
  const fallbackCardHeight = 240;

  const cardWidths = cards.map((card) => card.getBoundingClientRect().width).filter((v) => v > 0);
  const cardHeights = cards.map((card) => card.getBoundingClientRect().height).filter((v) => v > 0);
  const cardWidth = cardWidths.length > 0 ? Math.ceil(Math.max(...cardWidths)) : fallbackCardWidth;
  const firstCardHeight = cardHeights.length > 0 ? Math.ceil(cardHeights[0]) : fallbackCardHeight;
  const totalCardsHeight = cardHeights.length > 0
    ? Math.ceil(cardHeights.reduce((sum, h) => sum + h, 0))
    : fallbackCardHeight;

  const containerStyle = container ? getComputedStyle(container) : null;
  const dashboardStyle = dashboard ? getComputedStyle(dashboard) : null;
  const paddingX = containerStyle
    ? (parseFloat(containerStyle.paddingLeft) || 0) + (parseFloat(containerStyle.paddingRight) || 0)
    : 0;
  const paddingY = containerStyle
    ? (parseFloat(containerStyle.paddingTop) || 0) + (parseFloat(containerStyle.paddingBottom) || 0)
    : 0;
  const rowGap = dashboardStyle
    ? (parseFloat(dashboardStyle.rowGap) || parseFloat(dashboardStyle.gap) || 0)
    : 0;

  const minWidth = clamp(Math.ceil(cardWidth + paddingX), 360, 2000);
  const minHeight = clamp(Math.ceil(firstCardHeight + paddingY), 220, 2000);
  const preferredHeight = clamp(
    Math.ceil(totalCardsHeight + (rowGap * Math.max(cards.length - 1, 0)) + paddingY),
    minHeight,
    4000
  );

  return { minWidth, minHeight, preferredWidth: minWidth, preferredHeight };
}

function isMinimalToggleTarget(target) {
  if (!(target instanceof Element)) return false;
  if (!target.closest('.container')) return false;
  if (isInteractiveUiTarget(target)) {
    return false;
  }
  if (state.windowMode === 'minimal') return true;
  if (target.closest('h1,.subtitle,.card,.window,.controls,.setup,.raw,.log-section')) return false;
  return true;
}

async function toggleWindowModeByGesture() {
  minimalDragState.active = false;
  const nextMode = state.windowMode === 'minimal' ? 'normal' : 'minimal';
  state.windowMode = nextMode;
  applyMinimalModeUI(nextMode === 'minimal');

  if (!IS_ELECTRON) return;

  const payload = { mode: nextMode };
  if (nextMode === 'minimal') {
    const metrics = computeMinimalWindowMetrics();
    payload.minWidth = metrics.minWidth;
    payload.minHeight = metrics.minHeight;
    if (!state.hasSavedMinimalBounds) {
      payload.preferredWidth = metrics.preferredWidth;
      payload.preferredHeight = metrics.preferredHeight;
      state.hasSavedMinimalBounds = true;
    }
  }

  try {
    await window.quotaApi.setWindowMode(payload);
  } catch (e) {
    log(`ウィンドウ切替エラー: ${e.message || e}`, 'warn');
  }
}

function isMinimalDragTarget(target) {
  if (!(target instanceof Element)) return false;
  if (!target.closest('.container')) return false;
  if (isInteractiveUiTarget(target)) return false;
  return true;
}

function setupMinimalWindowDragHandlers() {
  if (!IS_ELECTRON) return;

  const endDrag = () => {
    minimalDragState.active = false;
  };

  document.addEventListener('mousedown', (event) => {
    if (state.windowMode !== 'minimal') return;
    if (event.button !== 0) return;
    if (!isMinimalDragTarget(event.target)) return;
    minimalDragState.active = true;
    minimalDragState.offsetX = event.screenX - window.screenX;
    minimalDragState.offsetY = event.screenY - window.screenY;
  });

  document.addEventListener('mousemove', (event) => {
    if (!minimalDragState.active) return;
    const x = Math.round(event.screenX - minimalDragState.offsetX);
    const y = Math.round(event.screenY - minimalDragState.offsetY);
    window.quotaApi.setWindowPosition({ x, y })
      .then(() => {
        didLogWindowMoveError = false;
      })
      .catch((e) => {
        if (!didLogWindowMoveError) {
          log(`ウィンドウ移動エラー: ${e.message || e}`, 'warn');
          didLogWindowMoveError = true;
        }
      });
  });

  document.addEventListener('mouseup', endDrag);
  document.addEventListener('mouseleave', endDrag);
}

// ═══════════════════════════════════════
// Controls
// ═══════════════════════════════════════
function startPolling() {
  stopPolling(false);
  const intervalSec = Math.max(30, parseInt($('#poll-interval').value, 10) || 120);
  state.polling = true;
  state.pollInterval = intervalSec;
  state.pollStartedAt = Date.now();
  persistPollingState();
  $('#btn-start').textContent = '⏹ 停止';
  $('#btn-start').classList.add('active');
  updatePollStatus('取得中...');
  pollAll().then(() => { state.pollStartedAt = Date.now(); persistPollingState(); updatePollRing(); updateCountdown(); });
  state.timer = setInterval(() => {
    updatePollStatus('取得中...');
    pollAll().then(() => {
      state.pollStartedAt = Date.now();
      persistPollingState();
      updateCountdown();
    });
  }, intervalSec * 1000);
  state.ringTimer = setInterval(() => { updatePollRing(); updateCountdown(); }, 1000);
  queuePersistSetup();
}

function stopPolling(persistState = true) {
  state.polling = false;
  if (state.timer) { clearInterval(state.timer); state.timer = null; }
  if (state.ringTimer) { clearInterval(state.ringTimer); state.ringTimer = null; }
  state.pollStartedAt = null;
  if (persistState) persistPollingState();
  updatePollRing();
  $('#btn-start').textContent = '▶ 開始';
  $('#btn-start').classList.remove('active');
  updatePollStatus('停止中');
}

function resumePolling(_startedAt, intervalSec) {
  if (intervalSec) $('#poll-interval').value = String(intervalSec);
  startPolling();
}

function updatePollStatus(msg) {
  $('#poll-status').textContent = msg;
}

function updateCountdown() {
  if (!state.polling || !state.pollStartedAt) return;
  const elapsed = (Date.now() - state.pollStartedAt) / 1000;
  const remaining = Math.max(0, Math.ceil(state.pollInterval - elapsed));
  updatePollStatus(`次回更新: ${remaining}秒後`);
}

function hasAnyUsableToken(accounts) {
  const all = [...(accounts?.claude || []), ...(accounts?.codex || [])];
  if (IS_ELECTRON) return all.some((a) => a.hasToken || a.token);
  return all.some((a) => a.token);
}

function ensureSetupOpenIfMissingToken(accounts) {
  if (!hasAnyUsableToken(accounts)) $('#setup').open = true;
}

// ═══════════════════════════════════════
// Init
// ═══════════════════════════════════════
document.addEventListener('DOMContentLoaded', async () => {
  let restored = null;
  let restoredPollState = null;
  if (IS_ELECTRON) {
    try {
      const snapshot = await window.quotaApi.listAccounts();
      state.accounts.claude = Array.isArray(snapshot?.claude) ? snapshot.claude.map((x) => ({ ...x, token: '' })) : [];
      state.accounts.codex = Array.isArray(snapshot?.codex) ? snapshot.codex.map((x) => ({ ...x, token: '' })) : [];
      const settings = await window.quotaApi.getSettings();
      if (settings?.pollInterval) $('#poll-interval').value = String(settings.pollInterval);
      restoredPollState = await window.quotaApi.getPollingState();
      const windowState = await window.quotaApi.getWindowState();
      state.windowMode = windowState?.mode === 'minimal' ? 'minimal' : 'normal';
      state.hasSavedMinimalBounds = Boolean(windowState?.minimalBounds);
      applyMinimalModeUI(state.windowMode === 'minimal');
    } catch (e) {
      log(`Electron 初期化に失敗: ${e.message || e}`, 'warn');
    }
  } else {
    try {
      const raw = sessionStorage.getItem(SESSION_KEYS.accounts);
      restored = raw ? JSON.parse(raw) : null;
      if (restored && typeof restored === 'object') {
        state.accounts.claude = Array.isArray(restored.claude) ? restored.claude : [];
        state.accounts.codex = Array.isArray(restored.codex) ? restored.codex : [];
      } else {
        // Backward compatibility with old single-token storage.
        const legacyClaude = sessionStorage.getItem(SESSION_KEYS.legacyClaude) || '';
        const legacyCodex = sessionStorage.getItem(SESSION_KEYS.legacyCodex) || '';
        if (legacyClaude) state.accounts.claude = [{ ...defaultAccount('claude', 0), token: legacyClaude }];
        if (legacyCodex) state.accounts.codex = [{ ...defaultAccount('codex', 0), token: legacyCodex }];
      }
      const iv = sessionStorage.getItem(SESSION_KEYS.interval);
      if (iv) $('#poll-interval').value = iv;
      const psRaw = sessionStorage.getItem(SESSION_KEYS.pollState);
      restoredPollState = psRaw ? JSON.parse(psRaw) : null;
    } catch {}
  }

  if (state.accounts.claude.length === 0) state.accounts.claude = [defaultAccount('claude', 0)];
  if (state.accounts.codex.length === 0) state.accounts.codex = [defaultAccount('codex', 0)];
  writeAccountsToDom('claude', state.accounts.claude);
  writeAccountsToDom('codex', state.accounts.codex);

  if (IS_ELECTRON) {
    const subtitle = document.querySelector('.subtitle');
    if (subtitle) subtitle.textContent = 'Electron 版 — トークンは OS キーチェーンに保存されます';
  }

  ensureSetupOpenIfMissingToken(state.accounts);

  try {
    const svcs = sessionStorage.getItem(SESSION_KEYS.services);
    const raw = sessionStorage.getItem(SESSION_KEYS.raw);
    const fetchedAt = sessionStorage.getItem(SESSION_KEYS.fetchedAt);
    if (svcs) {
      state.services = JSON.parse(svcs);
      state.rawResponses = raw ? JSON.parse(raw) : {};
      const histRaw = sessionStorage.getItem(SESSION_KEYS.history);
      if (histRaw) state.history = JSON.parse(histRaw);
      render();
      if (fetchedAt) log(`前回取得: ${new Date(fetchedAt).toLocaleString()}`);
    }
  } catch {}

  $('#btn-start').addEventListener('click', () => {
    if (state.polling) { stopPolling(); return; }
    const accounts = collectAccounts();
    if (!hasAnyUsableToken(accounts)) {
      log('トークンが設定されていません', 'warn');
      ensureSetupOpenIfMissingToken(accounts);
      return;
    }
    startPolling();
  });

  $('#btn-poll').addEventListener('click', () => {
    const accounts = collectAccounts();
    if (!hasAnyUsableToken(accounts)) {
      log('トークンが設定されていません', 'warn');
      ensureSetupOpenIfMissingToken(accounts);
      return;
    }
    updatePollStatus('取得中...');
    pollAll().then(() => updatePollStatus('取得完了'));
  });

  $('#btn-notify').addEventListener('click', async () => {
    const perm = await Notification.requestPermission();
    if (perm === 'granted') {
      log('通知が許可されました', 'ok');
      notify('テスト', 'AI Quota Monitor の通知が有効です');
    } else {
      log('通知が拒否されました', 'warn');
    }
  });
  $(SERVICE_META.claude.addBtnId).addEventListener('click', () => addAccountRow('claude'));
  $(SERVICE_META.codex.addBtnId).addEventListener('click', () => addAccountRow('codex'));
  $('#poll-interval').addEventListener('change', queuePersistSetup);
  if (!restored) queuePersistSetup();

  document.addEventListener('dblclick', (event) => {
    if (!isMinimalToggleTarget(event.target)) return;
    toggleWindowModeByGesture();
  });
  setupMinimalWindowDragHandlers();

  render();

  try {
    if (restoredPollState?.interval) {
      $('#poll-interval').value = String(restoredPollState.interval);
    }
    const shouldResume = Boolean(restoredPollState?.active || restoredPollState?.startedAt);
    if (shouldResume && restoredPollState?.interval) {
      resumePolling(null, restoredPollState.interval);
    } else {
      updatePollStatus('停止中');
    }
  } catch {
    updatePollStatus('停止中');
  }
});
