function createStoreService({ app, fs, path, constants }) {
  const {
    STORE_FILE,
    NORMAL_WINDOW_DEFAULT,
    NORMAL_WINDOW_MIN,
    MINIMAL_WINDOW_DEFAULT,
    MINIMAL_WINDOW_MIN_DEFAULT,
    MINIMAL_FLOOR_W,
    MINIMAL_FLOOR_H,
  } = constants;

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
      MINIMAL_FLOOR_H,
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

  return {
    clampInt,
    optionalInt,
    sanitizeBounds,
    defaultStore,
    normalizeStore,
    readStore,
    writeStore,
  };
}

module.exports = {
  createStoreService,
};
