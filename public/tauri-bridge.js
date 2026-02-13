(function initTauriBridge(root) {
  const tauri = root.__TAURI__;
  const invoke = tauri?.core?.invoke || tauri?.invoke;

  if (typeof invoke !== 'function') {
    throw new Error('Tauri invoke API is unavailable');
  }

  const call = (command, args = {}) => invoke(command, args);

  root.quotaApi = {
    platform: 'tauri',
    listAccounts: () => call('list_accounts'),
    saveAccount: (payload) => call('save_account', { payload }),
    deleteAccount: (payload) => call('delete_account', { payload }),
    getSettings: () => call('get_settings'),
    setSettings: (payload) => call('set_settings', { payload }),
    getPollingState: () => call('get_polling_state'),
    setPollingState: (payload) => call('set_polling_state', { payload }),
    fetchUsage: (payload) => call('fetch_usage', { payload }),
    getWindowState: () => call('get_window_state'),
    setWindowMode: (payload) => call('set_window_mode', { payload }),
    setWindowPosition: (payload) => call('set_window_position', { payload }),
    getVersion: () => call('get_version'),
  };
}(typeof window !== 'undefined' ? window : globalThis));
