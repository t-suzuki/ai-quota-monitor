const path = require('node:path');
const fs = require('node:fs/promises');
const { app, BrowserWindow, ipcMain } = require('electron');
const keytar = require('keytar');
const { fetchNormalizedUsage } = require('../src/core/usage-service');

const APP_NAME = 'AI Quota Monitor';
const STORE_FILE = 'accounts.json';
const SERVICES = new Set(['claude', 'codex']);

const NORMAL_WINDOW_DEFAULT = { width: 1100, height: 840 };
const NORMAL_WINDOW_MIN = { width: 980, height: 700 };

// Minimal-mode sizing — single source of truth: card width (must match CSS --minimal-card-width & app.js)
const MINIMAL_CARD_WIDTH = 290;
const MINIMAL_PAD = 16;                                            // .container padding in minimal mode (8px × 2)
const MINIMAL_MIN_W = MINIMAL_CARD_WIDTH + MINIMAL_PAD;            // 306 — minimum window width
const MINIMAL_FLOOR_W = MINIMAL_CARD_WIDTH - 40;                   // 250 — validation floor
const MINIMAL_WINDOW_DEFAULT = { width: MINIMAL_MIN_W + 74, height: 420 };
const MINIMAL_WINDOW_MIN_DEFAULT = { width: MINIMAL_MIN_W, height: 240 };

let mainWindow = null;
let isSwitchingWindow = false;

function storePath() {
  return path.join(app.getPath('userData'), STORE_FILE);
}

function clampInt(value, fallback, min, max) {
  const n = Number(value);
  if (!Number.isFinite(n)) return fallback;
  return Math.max(min, Math.min(max, Math.floor(n)));
}

function optionalInt(value) {
  const n = Number(value);
  return Number.isFinite(n) ? Math.floor(n) : null;
}

function sanitizeBounds(bounds, minWidth, minHeight, fallback) {
  const source = bounds && typeof bounds === 'object' ? bounds : fallback;
  const width = clampInt(source?.width, fallback.width, minWidth, 8192);
  const height = clampInt(source?.height, fallback.height, minHeight, 8192);
  const x = optionalInt(source?.x);
  const y = optionalInt(source?.y);

  const out = { width, height };
  if (x !== null) out.x = x;
  if (y !== null) out.y = y;
  return out;
}

function defaultStore() {
  return {
    services: { claude: [], codex: [] },
    settings: {
      pollInterval: 120,
      pollingState: {
        active: false,
        startedAt: null,
        interval: 120,
      },
      windowState: {
        mode: 'normal',
        normalBounds: { ...NORMAL_WINDOW_DEFAULT },
        minimalBounds: null,
        minimalMinWidth: MINIMAL_WINDOW_MIN_DEFAULT.width,
        minimalMinHeight: MINIMAL_WINDOW_MIN_DEFAULT.height,
      },
      notifySettings: {
        critical: true,
        recovery: true,
        warning: false,
        thresholdWarning: 75,
        thresholdCritical: 90,
      },
    },
  };
}

function normalizeStore(parsed) {
  const base = defaultStore();
  const settings = parsed?.settings || {};

  const pollInterval = clampInt(settings.pollInterval, base.settings.pollInterval, 30, 600);

  const pollingRaw = settings.pollingState || {};
  const pollingInterval = clampInt(pollingRaw.interval, pollInterval, 30, 600);
  const pollingStartedAt = optionalInt(pollingRaw.startedAt);
  const pollingState = {
    active: Boolean(pollingRaw.active),
    startedAt: pollingStartedAt && pollingStartedAt > 0 ? pollingStartedAt : null,
    interval: pollingInterval,
  };

  const windowRaw = settings.windowState || {};
  const mode = windowRaw.mode === 'minimal' ? 'minimal' : 'normal';
  const minimalMinWidth = clampInt(
    windowRaw.minimalMinWidth,
    MINIMAL_WINDOW_MIN_DEFAULT.width,
    MINIMAL_FLOOR_W,
    4096
  );
  const minimalMinHeight = clampInt(
    windowRaw.minimalMinHeight,
    MINIMAL_WINDOW_MIN_DEFAULT.height,
    MINIMAL_FLOOR_W,
    4096
  );

  const normalBounds = sanitizeBounds(
    windowRaw.normalBounds,
    NORMAL_WINDOW_MIN.width,
    NORMAL_WINDOW_MIN.height,
    NORMAL_WINDOW_DEFAULT
  );
  const minimalBoundsRaw = windowRaw.minimalBounds;
  const minimalBounds = minimalBoundsRaw
    ? sanitizeBounds(minimalBoundsRaw, minimalMinWidth, minimalMinHeight, MINIMAL_WINDOW_DEFAULT)
    : null;

  const nsRaw = settings.notifySettings || {};
  const notifySettings = {
    critical: typeof nsRaw.critical === 'boolean' ? nsRaw.critical : base.settings.notifySettings.critical,
    recovery: typeof nsRaw.recovery === 'boolean' ? nsRaw.recovery : base.settings.notifySettings.recovery,
    warning: typeof nsRaw.warning === 'boolean' ? nsRaw.warning : base.settings.notifySettings.warning,
    thresholdWarning: clampInt(nsRaw.thresholdWarning, base.settings.notifySettings.thresholdWarning, 1, 99),
    thresholdCritical: clampInt(nsRaw.thresholdCritical, base.settings.notifySettings.thresholdCritical, 1, 99),
  };

  return {
    services: {
      claude: Array.isArray(parsed?.services?.claude) ? parsed.services.claude : [],
      codex: Array.isArray(parsed?.services?.codex) ? parsed.services.codex : [],
    },
    settings: {
      pollInterval,
      pollingState,
      windowState: {
        mode,
        normalBounds,
        minimalBounds,
        minimalMinWidth,
        minimalMinHeight,
      },
      notifySettings,
    },
  };
}

async function readStore() {
  try {
    const raw = await fs.readFile(storePath(), 'utf8');
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return defaultStore();
    return normalizeStore(parsed);
  } catch {
    return defaultStore();
  }
}

async function writeStore(store) {
  await fs.mkdir(app.getPath('userData'), { recursive: true });
  await fs.writeFile(storePath(), JSON.stringify(store, null, 2), 'utf8');
}

function ensureService(service) {
  if (!SERVICES.has(service)) throw new Error('Unsupported service');
}

function tokenAccountKey(service, id) {
  return `${service}:${id}`;
}

function sanitizeString(value, fallback = '') {
  const s = typeof value === 'string' ? value.trim() : '';
  return s || fallback;
}

function currentWindow() {
  if (mainWindow && !mainWindow.isDestroyed()) return mainWindow;
  const [win] = BrowserWindow.getAllWindows();
  return win || null;
}

async function listAccountsWithTokenState() {
  const store = await readStore();
  const result = { claude: [], codex: [], settings: store.settings };

  for (const service of ['claude', 'codex']) {
    const accounts = [];
    for (const entry of store.services[service]) {
      const id = sanitizeString(entry.id);
      if (!id) continue;
      const name = sanitizeString(entry.name, `${service}:${id}`);
      const token = await keytar.getPassword(APP_NAME, tokenAccountKey(service, id));
      accounts.push({ id, name, hasToken: Boolean(token) });
    }
    result[service] = accounts;
  }

  return result;
}

async function upsertAccount(payload) {
  const service = sanitizeString(payload?.service);
  ensureService(service);
  const id = sanitizeString(payload?.id);
  if (!id) throw new Error('Account id is required');
  const fallbackName = `${service.toUpperCase()} ${id}`;
  const name = sanitizeString(payload?.name, fallbackName);

  const store = await readStore();
  const list = store.services[service];
  const idx = list.findIndex((x) => x.id === id);
  const next = { id, name };
  if (idx === -1) list.push(next);
  else list[idx] = next;

  const token = typeof payload?.token === 'string' ? payload.token.trim() : null;
  if (token) {
    await keytar.setPassword(APP_NAME, tokenAccountKey(service, id), token);
  }
  if (payload?.clearToken === true) {
    await keytar.deletePassword(APP_NAME, tokenAccountKey(service, id));
  }

  await writeStore(store);
  const hasToken = Boolean(await keytar.getPassword(APP_NAME, tokenAccountKey(service, id)));
  return { id, name, hasToken };
}

async function deleteAccount(payload) {
  const service = sanitizeString(payload?.service);
  ensureService(service);
  const id = sanitizeString(payload?.id);
  if (!id) throw new Error('Account id is required');

  const store = await readStore();
  store.services[service] = store.services[service].filter((x) => x.id !== id);
  await writeStore(store);
  await keytar.deletePassword(APP_NAME, tokenAccountKey(service, id));
  return { ok: true };
}

async function getSettings() {
  return (await readStore()).settings;
}

async function setSettings(payload) {
  const store = await readStore();
  const pollInterval = Number(payload?.pollInterval);
  if (Number.isFinite(pollInterval) && pollInterval >= 30 && pollInterval <= 600) {
    const next = Math.floor(pollInterval);
    store.settings.pollInterval = next;
    if (!store.settings.pollingState.active) {
      store.settings.pollingState.interval = next;
    }
  }
  if (payload?.notifySettings && typeof payload.notifySettings === 'object') {
    const ns = payload.notifySettings;
    const current = store.settings.notifySettings;
    if (typeof ns.critical === 'boolean') current.critical = ns.critical;
    if (typeof ns.recovery === 'boolean') current.recovery = ns.recovery;
    if (typeof ns.warning === 'boolean') current.warning = ns.warning;
    const tw = Number(ns.thresholdWarning);
    if (Number.isFinite(tw) && tw >= 1 && tw <= 99) current.thresholdWarning = Math.floor(tw);
    const tc = Number(ns.thresholdCritical);
    if (Number.isFinite(tc) && tc >= 1 && tc <= 99) current.thresholdCritical = Math.floor(tc);
  }
  await writeStore(store);
  return store.settings;
}

async function getPollingState() {
  return (await readStore()).settings.pollingState;
}

async function setPollingState(payload) {
  const store = await readStore();
  const current = store.settings.pollingState;

  current.active = Boolean(payload?.active);
  const startedAt = optionalInt(payload?.startedAt);
  current.startedAt = startedAt && startedAt > 0 ? startedAt : null;

  const interval = Number(payload?.interval);
  if (Number.isFinite(interval) && interval >= 30 && interval <= 600) {
    current.interval = Math.floor(interval);
  }

  await writeStore(store);
  return current;
}

async function fetchUsage(payload) {
  const service = sanitizeString(payload?.service);
  ensureService(service);

  const id = sanitizeString(payload?.id);
  if (!id) throw new Error('Account id is required');
  const name = sanitizeString(payload?.name, `${service.toUpperCase()} ${id}`);

  // Keep account metadata in sync even if renderer-side persistence lags.
  const store = await readStore();
  const list = store.services[service];
  const idx = list.findIndex((x) => x.id === id);
  if (idx === -1) {
    list.push({ id, name });
    await writeStore(store);
  } else if (list[idx].name !== name) {
    list[idx] = { id, name };
    await writeStore(store);
  }

  const overrideToken = typeof payload?.token === 'string' ? payload.token.trim() : '';
  if (overrideToken) {
    await keytar.setPassword(APP_NAME, tokenAccountKey(service, id), overrideToken);
  }

  const token = await keytar.getPassword(APP_NAME, tokenAccountKey(service, id));
  if (!token) {
    throw new Error('Token is not set for this account');
  }

  return fetchNormalizedUsage(service, token);
}

function attachBoundsPersistence(win, mode) {
  let timer = null;

  const scheduleSave = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      if (win.isDestroyed()) return;
      const store = await readStore();
      const ws = store.settings.windowState;
      const bounds = win.getBounds();
      if (mode === 'minimal') {
        ws.minimalBounds = sanitizeBounds(
          bounds,
          ws.minimalMinWidth,
          ws.minimalMinHeight,
          MINIMAL_WINDOW_DEFAULT
        );
      } else {
        ws.normalBounds = sanitizeBounds(
          bounds,
          NORMAL_WINDOW_MIN.width,
          NORMAL_WINDOW_MIN.height,
          NORMAL_WINDOW_DEFAULT
        );
      }
      await writeStore(store);
    }, 300);
  };

  win.on('resize', scheduleSave);
  win.on('move', scheduleSave);
}

async function createMainWindow(forcedMode = null) {
  const store = await readStore();
  const ws = store.settings.windowState;
  const mode = forcedMode || ws.mode;
  const isMinimal = mode === 'minimal';

  const minWidth = isMinimal ? ws.minimalMinWidth : NORMAL_WINDOW_MIN.width;
  const minHeight = isMinimal ? ws.minimalMinHeight : NORMAL_WINDOW_MIN.height;

  const sourceBounds = isMinimal
    ? (ws.minimalBounds || MINIMAL_WINDOW_DEFAULT)
    : (ws.normalBounds || NORMAL_WINDOW_DEFAULT);

  const bounds = sanitizeBounds(
    sourceBounds,
    minWidth,
    minHeight,
    isMinimal ? MINIMAL_WINDOW_DEFAULT : NORMAL_WINDOW_DEFAULT
  );

  const win = new BrowserWindow({
    ...bounds,
    minWidth,
    minHeight,
    frame: !isMinimal,
    autoHideMenuBar: isMinimal,
    alwaysOnTop: isMinimal,
    backgroundColor: '#0d1117',
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      preload: path.join(__dirname, 'preload.js'),
    },
  });

  win.setMenuBarVisibility(!isMinimal);
  win.setAlwaysOnTop(isMinimal, 'floating');
  attachBoundsPersistence(win, mode);
  await win.loadFile(path.join(__dirname, '..', 'public', 'index.html'));
  return win;
}

async function setWindowMode(payload) {
  const requestedMode = payload?.mode === 'minimal' ? 'minimal' : 'normal';
  const store = await readStore();
  const ws = store.settings.windowState;
  const currentMode = ws.mode;

  if (requestedMode === 'minimal') {
    const minWidth = Number(payload?.minWidth);
    const minHeight = Number(payload?.minHeight);
    if (Number.isFinite(minWidth) && minWidth >= MINIMAL_FLOOR_W) {
      ws.minimalMinWidth = Math.floor(minWidth);
    }
    if (Number.isFinite(minHeight) && minHeight >= 220) {
      ws.minimalMinHeight = Math.floor(minHeight);
    }

    const preferredWidth = Number(payload?.preferredWidth);
    const preferredHeight = Number(payload?.preferredHeight);
    if (Number.isFinite(preferredWidth) && Number.isFinite(preferredHeight)) {
      const win = currentWindow();
      const current = win && !win.isDestroyed() ? win.getBounds() : {};
      ws.minimalBounds = sanitizeBounds(
        {
          width: Math.floor(preferredWidth),
          height: Math.floor(preferredHeight),
          x: current.x,
          y: current.y,
        },
        ws.minimalMinWidth,
        ws.minimalMinHeight,
        MINIMAL_WINDOW_DEFAULT
      );
    }
  }

  const win = currentWindow();
  if (win && !win.isDestroyed()) {
    const currentBounds = win.getBounds();
    if (currentMode === 'minimal') {
      ws.minimalBounds = sanitizeBounds(
        currentBounds,
        ws.minimalMinWidth,
        ws.minimalMinHeight,
        MINIMAL_WINDOW_DEFAULT
      );
    } else {
      ws.normalBounds = sanitizeBounds(
        currentBounds,
        NORMAL_WINDOW_MIN.width,
        NORMAL_WINDOW_MIN.height,
        NORMAL_WINDOW_DEFAULT
      );
    }
  }

  ws.mode = requestedMode;
  await writeStore(store);

  if (requestedMode === currentMode && win && !win.isDestroyed()) {
    if (requestedMode === 'minimal') {
      win.setMinimumSize(ws.minimalMinWidth, ws.minimalMinHeight);
      win.setAlwaysOnTop(true, 'floating');
      win.setMenuBarVisibility(false);
    } else {
      win.setAlwaysOnTop(false);
      win.setMenuBarVisibility(true);
    }
    return ws;
  }

  const oldWin = win;
  isSwitchingWindow = true;
  const newWin = await createMainWindow(requestedMode);
  mainWindow = newWin;
  if (oldWin && !oldWin.isDestroyed()) oldWin.destroy();
  isSwitchingWindow = false;

  return ws;
}

async function getWindowState() {
  return (await readStore()).settings.windowState;
}

function setWindowPosition(payload) {
  const win = currentWindow();
  if (!win || win.isDestroyed()) return { ok: false };

  const x = optionalInt(payload?.x);
  const y = optionalInt(payload?.y);
  if (x === null || y === null) throw new Error('x and y are required');

  const width = optionalInt(payload?.width);
  const height = optionalInt(payload?.height);
  if (width !== null && height !== null && width >= MINIMAL_FLOOR_W && height >= MINIMAL_FLOOR_W) {
    win.setBounds({ x, y, width, height }, false);
  } else {
    win.setPosition(x, y);
  }
  return { ok: true };
}

app.whenReady().then(async () => {
  ipcMain.handle('quota:list-accounts', () => listAccountsWithTokenState());
  ipcMain.handle('quota:save-account', (_event, payload) => upsertAccount(payload));
  ipcMain.handle('quota:delete-account', (_event, payload) => deleteAccount(payload));
  ipcMain.handle('quota:get-settings', () => getSettings());
  ipcMain.handle('quota:set-settings', (_event, payload) => setSettings(payload));
  ipcMain.handle('quota:get-polling-state', () => getPollingState());
  ipcMain.handle('quota:set-polling-state', (_event, payload) => setPollingState(payload));
  ipcMain.handle('quota:fetch-usage', (_event, payload) => fetchUsage(payload));
  ipcMain.handle('quota:get-window-state', () => getWindowState());
  ipcMain.handle('quota:set-window-mode', (_event, payload) => setWindowMode(payload));
  ipcMain.handle('quota:set-window-position', (_event, payload) => setWindowPosition(payload));
  ipcMain.handle('quota:get-version', () => app.getVersion());

  const store = await readStore();
  mainWindow = await createMainWindow(store.settings.windowState.mode);

  app.on('activate', async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      const latest = await readStore();
      mainWindow = await createMainWindow(latest.settings.windowState.mode);
    }
  });
});

app.on('window-all-closed', () => {
  if (isSwitchingWindow) return;
  if (process.platform !== 'darwin') app.quit();
});
