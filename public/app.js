// ═══════════════════════════════════════
// State
// ═══════════════════════════════════════
const state = {
  polling: false,
  pollInFlight: false,
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
  notifySettings: { critical: true, recovery: true, warning: false, thresholdWarning: 75, thresholdCritical: 90 },
};

const THRESHOLDS_EXHAUSTED = 100;
const POLL_RING_TICK_MS = 1000;
const SETUP_PERSIST_DEBOUNCE_MS = 200;
const HISTORY_RESET_DROP_PCT = 5;
const COPY_FEEDBACK_MS = 1500;
const MAX_LOG_ENTRIES = 200;
const MAX_RENDERED_LOGS = 50;
const HISTORY_MAX_POINTS = 10;
const POLL_INTERVAL_MIN_SEC = 30;
const POLL_INTERVAL_DEFAULT_SEC = 120;
const NOTIFY_THRESHOLD_MIN = 1;
const NOTIFY_THRESHOLD_MAX = 99;
const NOTIFY_THRESHOLD_WARNING_DEFAULT = 75;
const NOTIFY_THRESHOLD_CRITICAL_DEFAULT = 90;
const MINIMAL_FALLBACK_CARD_HEIGHT = 240;
const MINIMAL_MIN_HEIGHT_FLOOR = 220;
const MINIMAL_WINDOW_MAX_SIZE = 2000;
const MINIMAL_PREFERRED_HEIGHT_MAX = 4000;
const MINIMAL_DRAG_EDGE_PX = 8;
if (!window.quotaApi || window.quotaApi.platform !== 'tauri') {
  throw new Error('Tauri quotaApi bridge is required');
}
if (!window.UiLogic) {
  throw new Error('UiLogic helpers are required');
}
if (!window.AccountUi) {
  throw new Error('AccountUi helpers are required');
}
const {
  deriveServiceStatus,
  buildTransitionEffects,
  calcElapsedPct: calcElapsedPctValue,
  computePollingState,
} = window.UiLogic;
// Minimal-mode sizing — single source of truth: card width (must match CSS --minimal-card-width)
const MINIMAL_CARD_WIDTH = 290;
const MINIMAL_FLOOR_W = MINIMAL_CARD_WIDTH - 40;           // validation floor for clamp / drag
const SAVED_TOKEN_MASK = '********************';
const SESSION_KEYS = {
  services: 'qm-services',
  history: 'qm-history',
  fetchedAt: 'qm-fetched-at',
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
    setTimeout(() => { btn.textContent = '⧉'; btn.classList.remove('copied'); }, COPY_FEEDBACK_MS);
  });
});

// ═══════════════════════════════════════
// Utils
// ═══════════════════════════════════════
const $ = s => document.querySelector(s);
const log = (msg, level = '') => {
  const ts = new Date().toLocaleTimeString();
  state.logs.unshift({ ts, msg, level });
  if (state.logs.length > MAX_LOG_ENTRIES) state.logs.length = MAX_LOG_ENTRIES;
  renderLogs();
};
const toErrorMessage = (error) => {
  if (typeof error === 'string') return error;
  if (error && typeof error.message === 'string' && error.message.trim()) return error.message;
  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== '{}') return serialized;
  } catch {}
  return String(error ?? 'unknown error');
};

function classifyUtilization(pct) {
  return window.UiLogic.classifyUtilization(pct, state.notifySettings, THRESHOLDS_EXHAUSTED);
}

function classifyWindows(windows) {
  return window.UiLogic.classifyWindows(windows, state.notifySettings, THRESHOLDS_EXHAUSTED);
}

function reclassifyAllServices() {
  for (const svc of Object.values(state.services)) {
    if (!svc.windows || svc.windows.length === 0) continue;
    const prev = svc.status;
    classifyWindows(svc.windows);
    svc.status = deriveServiceStatus(svc.windows);
    checkStatusTransition(prev, svc.status, svc.label, svc.windows);
  }
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

async function notify(title, body) {
  await window.quotaApi.sendNotification({ title, body });
  return true;
}

function checkStatusTransition(prev, next, label, windows) {
  const effects = buildTransitionEffects(prev, next, label, windows, state.notifySettings);
  for (const item of effects.notifications) {
    notify(item.title, item.body).catch((e) => {
      log(`通知送信エラー: ${toErrorMessage(e)}`, 'warn');
    });
  }
  for (const item of effects.logs) {
    log(item.message, item.level);
  }
}

async function resolveAppVersion() {
  try {
    return await window.quotaApi.getVersion();
  } catch {
    return '';
  }
}

let didLogPersistError = false;
let didLogPollingPersistError = false;
let didLogWindowMoveError = false;
const minimalDragState = {
  active: false,
  offsetX: 0,
  offsetY: 0,
  width: 0,
  height: 0,
};
const accountUi = window.AccountUi.createAccountUi({
  query: $,
  serviceMeta: SERVICE_META,
  savedTokenMask: SAVED_TOKEN_MASK,
  escHtml,
  deriveTokenInputValue: window.UiLogic.deriveTokenInputValue,
  normalizeAccountToken: window.UiLogic.normalizeAccountToken,
  queuePersistSetup,
  deleteAccount: (payload) => window.quotaApi.deleteAccount(payload),
  oauthLogin: (payload) => window.quotaApi.oauthLogin(payload),
  log,
});
const {
  defaultAccount,
  writeAccountsToDom,
  addAccountRow,
  collectAccounts,
  upsertDomTokenState,
} = accountUi;

async function persistSetup() {
  try {
    const accounts = collectAccounts();
    const interval = Math.max(POLL_INTERVAL_MIN_SEC, parseInt($('#poll-interval').value, 10) || POLL_INTERVAL_DEFAULT_SEC);

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
    didLogPersistError = false;
  } catch (e) {
    if (!didLogPersistError) {
      log(`設定保存エラー: ${toErrorMessage(e)}`, 'warn');
      didLogPersistError = true;
    }
  }
}

function persistLastData() {
  try {
    sessionStorage.setItem(SESSION_KEYS.services, JSON.stringify(state.services));
    sessionStorage.setItem(SESSION_KEYS.history, JSON.stringify(state.history));
    sessionStorage.setItem(SESSION_KEYS.fetchedAt, new Date().toISOString());
  } catch {}
}

function persistPollingState() {
  const payload = {
    active: Boolean(state.polling),
    interval: state.pollInterval,
  };

  window.quotaApi.setPollingState(payload).then(() => {
    didLogPollingPersistError = false;
  }).catch((e) => {
    if (!didLogPollingPersistError) {
      log(`ポーリング状態保存エラー: ${toErrorMessage(e)}`, 'warn');
      didLogPollingPersistError = true;
    }
  });
}

let persistTimer = null;
function queuePersistSetup() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistSetup();
  }, SETUP_PERSIST_DEBOUNCE_MS);
}

// ═══════════════════════════════════════
// Poll orchestrator
// ═══════════════════════════════════════
async function tryAutoRefresh(service, id) {
  try {
    const status = await window.quotaApi.getTokenStatus({ service, id });
    if (status && status.needsRefresh && status.hasRefreshToken) {
      log(`トークンを自動更新中: ${SERVICE_META[service].label}`, '');
      const result = await window.quotaApi.refreshToken({ service, id });
      if (result.success) {
        log(`トークンを自動更新しました: ${SERVICE_META[service].label}`, 'ok');
        return true;
      } else {
        log(`トークン自動更新に失敗: ${result.message}`, 'warn');
      }
    }
  } catch (e) {
    log(`トークン更新チェックエラー: ${toErrorMessage(e)}`, 'warn');
  }
  return false;
}

async function pollServiceAccounts(service, accounts, nextServices, worstOf) {
  const meta = SERVICE_META[service];
  let anySuccess = false;

  for (const acc of accounts) {
    if (!hasUsableToken(acc)) continue;
    const serviceKey = `${service}:${acc.id}`;
    const label = `${meta.label}: ${acc.name}`;

    // Auto-refresh token if close to expiry
    await tryAutoRefresh(service, acc.id);

    try {
      const result = await window.quotaApi.fetchUsage({
        service,
        id: acc.id,
        name: acc.name,
        token: acc.token || undefined,
      });
      const data = result.raw;
      const windows = result.windows;
      classifyWindows(windows);
      const worstStatus = worstOf(windows);
      state.rawResponses[serviceKey] = data;
      for (const w of windows) {
        recordHistory(`${serviceKey}:${w.name}`, w.utilization);
      }

      const prev = state.services[serviceKey]?.status;
      nextServices[serviceKey] = { label, windows, status: worstStatus };
      checkStatusTransition(prev, worstStatus, label, windows);
      anySuccess = true;
      log(`${label} 取得成功: ${windows.map(w => `${w.name}=${w.utilization}%`).join(', ')}`);
      upsertDomTokenState(service, acc.id, true);
    } catch (e) {
      const errorMessage = toErrorMessage(e);
      nextServices[serviceKey] = { label, windows: [], status: 'error', error: errorMessage };
      log(`${label} エラー: ${errorMessage}`, 'warn');
    }
  }

  return anySuccess;
}

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
  anySuccess = (await pollServiceAccounts('claude', state.accounts.claude, nextServices, worstOf)) || anySuccess;
  anySuccess = (await pollServiceAccounts('codex', state.accounts.codex, nextServices, worstOf)) || anySuccess;

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
  if (arr.length > 0 && utilization < arr[arr.length - 1] - HISTORY_RESET_DROP_PCT) arr.length = 0;
  arr.push(utilization);
  if (arr.length > HISTORY_MAX_POINTS) arr.shift();
}

function calcElapsedPct(resetsAt, windowSeconds) {
  return calcElapsedPctValue(resetsAt, windowSeconds, Date.now());
}

function toWidthClass(pct) {
  const value = Number(pct);
  const normalized = Number.isFinite(value) ? value : 0;
  const clamped = Math.max(0, Math.min(100, Math.round(normalized)));
  return `w-${clamped}`;
}

// ═══════════════════════════════════════
// Polling countdown ring
// ═══════════════════════════════════════
function updatePollRing() {
  const el = $('#poll-ring');
  if (!el) return;
  const timing = computePollingState({
    polling: state.polling,
    pollStartedAt: state.pollStartedAt,
    pollInterval: state.pollInterval,
    nowMs: Date.now(),
  });
  if (!timing) { el.innerHTML = ''; return; }
  const R = 14, C = 2 * Math.PI * R;
  const offset = C * (1 - timing.fraction);
  el.innerHTML = `<svg viewBox="0 0 36 36" width="32" height="32">
    <circle cx="18" cy="18" r="${R}" fill="none" stroke="var(--bg3)" stroke-width="2.5"/>
    <circle cx="18" cy="18" r="${R}" fill="none" stroke="${timing.color}" stroke-width="2.5"
      stroke-dasharray="${C.toFixed(2)}" stroke-dashoffset="-${offset.toFixed(2)}"
      stroke-linecap="round" transform="rotate(-90 18 18)"/>
  </svg>`;
}

function startRingTimer() {
  if (!state.polling || document.hidden) return;
  if (state.ringTimer) return;
  state.ringTimer = setInterval(() => {
    updatePollRing();
    updateCountdown();
  }, POLL_RING_TICK_MS);
}

function stopRingTimer() {
  if (!state.ringTimer) return;
  clearInterval(state.ringTimer);
  state.ringTimer = null;
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
      return `<div class="card card-${svc.status}">
        <div class="card-header">
          <span class="card-header-left">${logoHtml}<span class="card-label">${svc.label}</span></span>
          <span class="card-status error">エラー</span>
        </div>
        <div class="card-error-message">${escHtml(svc.error)}</div>
      </div>`;
    }

    const windowsHtml = svc.windows.map(w => {
      const usageWidthClass = toWidthClass(w.utilization);
      const elPct = calcElapsedPct(w.resetsAt, w.windowSeconds);
      const elStr = elPct !== null ? ` / 経過 ${elPct.toFixed(0)}%` : '';
      const elBar = elPct !== null
        ? `<div class="bar-track bar-track-elapsed"><div class="bar-fill elapsed ${toWidthClass(elPct)}"></div></div>`
        : '';
      return `<div class="window">
        <div class="window-header">
          <span>${w.name}</span>
          <span>使用 ${w.utilization.toFixed(1)}%${elStr}</span>
        </div>
        <div class="bar-track">
          <div class="bar-fill ${w.status} ${usageWidthClass}"></div>
        </div>
        ${elBar}
        ${w.resetsAt ? `<div class="reset-info">リセット: ${formatReset(w.resetsAt)}</div>` : ''}
      </div>`;
    }).join('');

    return `<div class="card card-${svc.status}">
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
  el.innerHTML = state.logs.slice(0, MAX_RENDERED_LOGS).map(l =>
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

function targetElement(target) {
  if (target instanceof Element) return target;
  return target && target.parentElement instanceof Element ? target.parentElement : null;
}

function applyMinimalModeUI(isMinimal) {
  document.body.classList.toggle('minimal-mode', Boolean(isMinimal));
}

function isInteractiveUiTarget(target) {
  const el = targetElement(target);
  if (!el) return false;
  return Boolean(el.closest('button,input,textarea,select,summary,details,label,a,pre,.cmd-copy,.cmd-code'));
}

function computeMinimalWindowMetrics() {
  const container = document.querySelector('.container');
  const dashboard = $('#dashboard');
  const cards = Array.from(dashboard?.querySelectorAll('.card') || []);

  const rootStyle = getComputedStyle(document.documentElement);
  const configuredCardWidthRaw = parseFloat(rootStyle.getPropertyValue('--minimal-card-width'));
  const configuredCardWidth = Number.isFinite(configuredCardWidthRaw) && configuredCardWidthRaw > 0
    ? configuredCardWidthRaw
    : MINIMAL_CARD_WIDTH;
  const cardWidths = cards.map((card) => card.getBoundingClientRect().width).filter((v) => v > 0);
  const cardHeights = cards.map((card) => card.getBoundingClientRect().height).filter((v) => v > 0);
  const measuredCardWidth = cardWidths.length > 0 ? Math.ceil(Math.max(...cardWidths)) : configuredCardWidth;
  const cardWidth = clamp(measuredCardWidth, MINIMAL_FLOOR_W, Math.ceil(configuredCardWidth));
  const firstCardHeight = cardHeights.length > 0 ? Math.ceil(cardHeights[0]) : MINIMAL_FALLBACK_CARD_HEIGHT;
  const totalCardsHeight = cardHeights.length > 0
    ? Math.ceil(cardHeights.reduce((sum, h) => sum + h, 0))
    : MINIMAL_FALLBACK_CARD_HEIGHT;

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

  const minWidth = clamp(Math.ceil(cardWidth + paddingX), MINIMAL_FLOOR_W, MINIMAL_WINDOW_MAX_SIZE);
  const minHeight = clamp(Math.ceil(firstCardHeight + paddingY), MINIMAL_MIN_HEIGHT_FLOOR, MINIMAL_WINDOW_MAX_SIZE);
  const preferredHeight = clamp(
    Math.ceil(totalCardsHeight + (rowGap * Math.max(cards.length - 1, 0)) + paddingY),
    minHeight,
    MINIMAL_PREFERRED_HEIGHT_MAX
  );

  return { minWidth, minHeight, preferredWidth: minWidth, preferredHeight };
}

function isMinimalToggleTarget(target) {
  const el = targetElement(target);
  if (!el) return false;
  return !isInteractiveUiTarget(target);
}

async function toggleWindowModeByGesture() {
  minimalDragState.active = false;
  const nextMode = state.windowMode === 'minimal' ? 'normal' : 'minimal';
  state.windowMode = nextMode;
  applyMinimalModeUI(nextMode === 'minimal');

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
    log(`ウィンドウ切替エラー: ${toErrorMessage(e)}`, 'warn');
  }
}

function isMinimalDragTarget(target) {
  const el = targetElement(target);
  if (!el) return false;
  if (isInteractiveUiTarget(target)) return false;
  return true;
}

function isNearWindowEdge(event) {
  return (
    event.clientX <= MINIMAL_DRAG_EDGE_PX ||
    event.clientY <= MINIMAL_DRAG_EDGE_PX ||
    event.clientX >= window.innerWidth - MINIMAL_DRAG_EDGE_PX ||
    event.clientY >= window.innerHeight - MINIMAL_DRAG_EDGE_PX
  );
}

function setupMinimalWindowDragHandlers() {
  const endDrag = () => {
    minimalDragState.active = false;
  };

  document.addEventListener('mousedown', (event) => {
    if (state.windowMode !== 'minimal') return;
    if (event.button !== 0) return;
    if (!isMinimalDragTarget(event.target)) return;
    if (isNearWindowEdge(event)) return;
    minimalDragState.active = true;
    minimalDragState.offsetX = event.screenX - window.screenX;
    minimalDragState.offsetY = event.screenY - window.screenY;
    minimalDragState.width = Math.max(MINIMAL_FLOOR_W, Math.round(window.outerWidth));
    minimalDragState.height = Math.max(MINIMAL_FLOOR_W, Math.round(window.outerHeight));
  });

  document.addEventListener('mousemove', (event) => {
    if (!minimalDragState.active) return;
    const x = Math.round(event.screenX - minimalDragState.offsetX);
    const y = Math.round(event.screenY - minimalDragState.offsetY);
    window.quotaApi.setWindowPosition({
      x,
      y,
      width: minimalDragState.width,
      height: minimalDragState.height,
    })
      .then(() => {
        didLogWindowMoveError = false;
      })
      .catch((e) => {
        if (!didLogWindowMoveError) {
          log(`ウィンドウ移動エラー: ${toErrorMessage(e)}`, 'warn');
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
async function runPollCycle() {
  if (!state.polling || state.pollInFlight) return;
  state.pollInFlight = true;
  state.pollStartedAt = Date.now();
  persistPollingState();
  updatePollRing();
  updateCountdown();
  updatePollStatus('取得中...');

  try {
    await pollAll();
    updatePollStatus('取得完了');
  } finally {
    state.pollInFlight = false;
  }
}

function startPolling() {
  stopPolling(false);
  const intervalSec = Math.max(POLL_INTERVAL_MIN_SEC, parseInt($('#poll-interval').value, 10) || POLL_INTERVAL_DEFAULT_SEC);
  state.polling = true;
  state.pollInterval = intervalSec;
  state.pollStartedAt = Date.now();
  state.pollInFlight = false;
  persistPollingState();
  $('#btn-start').textContent = '⏹ 停止';
  $('#btn-start').classList.add('active');
  state.timer = setInterval(() => {
    runPollCycle();
  }, intervalSec * 1000);
  startRingTimer();
  runPollCycle();
  queuePersistSetup();
}

function stopPolling(persistState = true) {
  state.polling = false;
  state.pollInFlight = false;
  if (state.timer) { clearInterval(state.timer); state.timer = null; }
  stopRingTimer();
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
  if (document.hidden) return;
  const timing = computePollingState({
    polling: state.polling,
    pollStartedAt: state.pollStartedAt,
    pollInterval: state.pollInterval,
    nowMs: Date.now(),
  });
  if (!timing) return;
  updatePollStatus(`次回更新: ${timing.remainingSecLabel}秒後`);
}

function hasUsableToken(acc) {
  return acc.hasToken || Boolean(acc.token);
}

function hasAnyUsableToken(accounts) {
  const all = [...(accounts?.claude || []), ...(accounts?.codex || [])];
  return all.some(hasUsableToken);
}

function ensureServicePlaceholders() {
  for (const service of Object.keys(SERVICE_META)) {
    for (const acc of (state.accounts[service] || [])) {
      if (!hasUsableToken(acc)) continue;
      const serviceKey = `${service}:${acc.id}`;
      if (state.services[serviceKey]) continue;
      state.services[serviceKey] = {
        label: `${SERVICE_META[service].label}: ${acc.name}`,
        windows: [],
        status: 'unknown',
      };
    }
  }
}

function ensureSetupOpenIfMissingToken(accounts) {
  if (!hasAnyUsableToken(accounts)) $('#setup').open = true;
}

function showInitFailure(error) {
  const message = toErrorMessage(error);
  log(`Tauri 初期化に失敗: ${message}`, 'warn');
  const dash = $('#dashboard');
  if (dash) {
    dash.innerHTML = '<div class="empty">初期化に失敗しました。アプリを再起動してください。</div>';
  }
  updatePollStatus('初期化失敗');
  const startBtn = $('#btn-start');
  const pollBtn = $('#btn-poll');
  if (startBtn) startBtn.disabled = true;
  if (pollBtn) pollBtn.disabled = true;
}

// ═══════════════════════════════════════
// Init
// ═══════════════════════════════════════
document.addEventListener('DOMContentLoaded', async () => {
  let restoredPollState = null;

  const version = await resolveAppVersion();
  const versionEl = $('#app-version');
  if (versionEl && version) versionEl.textContent = `v${version}`;

  try {
    const snapshot = await window.quotaApi.listAccounts();
    state.accounts.claude = Array.isArray(snapshot?.claude) ? snapshot.claude.map((x) => ({ ...x, token: '' })) : [];
    state.accounts.codex = Array.isArray(snapshot?.codex) ? snapshot.codex.map((x) => ({ ...x, token: '' })) : [];
    const settings = await window.quotaApi.getSettings();
    if (settings?.pollInterval) $('#poll-interval').value = String(settings.pollInterval);
    if (settings?.notifySettings) {
      const ns = settings.notifySettings;
      if (typeof ns.critical === 'boolean') state.notifySettings.critical = ns.critical;
      if (typeof ns.recovery === 'boolean') state.notifySettings.recovery = ns.recovery;
      if (typeof ns.warning === 'boolean') state.notifySettings.warning = ns.warning;
      if (typeof ns.thresholdWarning === 'number') state.notifySettings.thresholdWarning = ns.thresholdWarning;
      if (typeof ns.thresholdCritical === 'number') state.notifySettings.thresholdCritical = ns.thresholdCritical;
    }
    restoredPollState = await window.quotaApi.getPollingState();
    const windowState = await window.quotaApi.getWindowState();
    state.windowMode = windowState?.mode === 'minimal' ? 'minimal' : 'normal';
    state.hasSavedMinimalBounds = Boolean(windowState?.minimalBounds);
    applyMinimalModeUI(state.windowMode === 'minimal');
  } catch (e) {
    showInitFailure(e);
    return;
  }

  if (state.accounts.claude.length === 0) state.accounts.claude = [defaultAccount('claude', 0)];
  if (state.accounts.codex.length === 0) state.accounts.codex = [defaultAccount('codex', 0)];
  writeAccountsToDom('claude', state.accounts.claude);
  writeAccountsToDom('codex', state.accounts.codex);

  const subtitle = document.querySelector('.subtitle');
  if (subtitle) subtitle.textContent = 'トークンは OS キーチェーンに保存されます';

  ensureSetupOpenIfMissingToken(state.accounts);

  try {
    const svcs = sessionStorage.getItem(SESSION_KEYS.services);
    const fetchedAt = sessionStorage.getItem(SESSION_KEYS.fetchedAt);
    if (svcs) {
      state.services = JSON.parse(svcs);
      state.rawResponses = {};
      const histRaw = sessionStorage.getItem(SESSION_KEYS.history);
      if (histRaw) state.history = JSON.parse(histRaw);
      reclassifyAllServices();
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
    if (state.pollInFlight) {
      log('現在の取得処理が完了するまで待機してください', 'warn');
      return;
    }
    const accounts = collectAccounts();
    if (!hasAnyUsableToken(accounts)) {
      log('トークンが設定されていません', 'warn');
      ensureSetupOpenIfMissingToken(accounts);
      return;
    }
    if (state.polling) {
      runPollCycle();
      return;
    }
    state.pollInFlight = true;
    updatePollStatus('取得中...');
    pollAll()
      .then(() => updatePollStatus('取得完了'))
      .finally(() => {
        state.pollInFlight = false;
      });
  });

  $('#notify-critical').checked = state.notifySettings.critical;
  $('#notify-recovery').checked = state.notifySettings.recovery;
  $('#notify-warning').checked = state.notifySettings.warning;
  $('#threshold-warning').value = String(state.notifySettings.thresholdWarning);
  $('#threshold-critical').value = String(state.notifySettings.thresholdCritical);
  const persistNotifySettings = () => {
    state.notifySettings.critical = $('#notify-critical').checked;
    state.notifySettings.recovery = $('#notify-recovery').checked;
    state.notifySettings.warning = $('#notify-warning').checked;
    state.notifySettings.thresholdWarning = Math.max(
      NOTIFY_THRESHOLD_MIN,
      Math.min(
        NOTIFY_THRESHOLD_MAX,
        parseInt($('#threshold-warning').value, 10) || NOTIFY_THRESHOLD_WARNING_DEFAULT
      )
    );
    state.notifySettings.thresholdCritical = Math.max(
      NOTIFY_THRESHOLD_MIN,
      Math.min(
        NOTIFY_THRESHOLD_MAX,
        parseInt($('#threshold-critical').value, 10) || NOTIFY_THRESHOLD_CRITICAL_DEFAULT
      )
    );
    window.quotaApi.setSettings({ notifySettings: state.notifySettings }).catch(() => {});
    reclassifyAllServices();
    render();
  };
  $('#notify-critical').addEventListener('change', persistNotifySettings);
  $('#notify-recovery').addEventListener('change', persistNotifySettings);
  $('#notify-warning').addEventListener('change', persistNotifySettings);
  $('#threshold-warning').addEventListener('change', persistNotifySettings);
  $('#threshold-critical').addEventListener('change', persistNotifySettings);
  $('#btn-notify-test').addEventListener('click', async () => {
    try {
      await notify('テスト', 'AI Quota Monitor の通知が有効です');
      log('テスト通知を送信しました', 'ok');
    } catch (e) {
      log(`通知テストに失敗しました: ${toErrorMessage(e)}`, 'warn');
    }
  });

  $(SERVICE_META.claude.addBtnId).addEventListener('click', () => addAccountRow('claude'));
  $(SERVICE_META.codex.addBtnId).addEventListener('click', () => addAccountRow('codex'));
  $('#poll-interval').addEventListener('change', queuePersistSetup);
  queuePersistSetup();

  document.addEventListener('dblclick', (event) => {
    if (!isMinimalToggleTarget(event.target)) return;
    toggleWindowModeByGesture();
  });
  document.addEventListener('visibilitychange', () => {
    if (!state.polling) return;
    if (document.hidden) {
      stopRingTimer();
      return;
    }
    startRingTimer();
    updatePollRing();
    updateCountdown();
  });
  setupMinimalWindowDragHandlers();

  ensureServicePlaceholders();
  render();

  if (state.windowMode === 'minimal') {
    const metrics = computeMinimalWindowMetrics();
    window.quotaApi.setWindowMode({
      mode: 'minimal',
      minWidth: metrics.minWidth,
      minHeight: metrics.minHeight,
    }).catch((e) => {
      log(`ミニマル制約更新エラー: ${toErrorMessage(e)}`, 'warn');
    });
  }

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
