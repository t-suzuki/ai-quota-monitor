const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('quotaApi', {
  platform: 'electron',
  listAccounts: () => ipcRenderer.invoke('quota:list-accounts'),
  saveAccount: (payload) => ipcRenderer.invoke('quota:save-account', payload),
  deleteAccount: (payload) => ipcRenderer.invoke('quota:delete-account', payload),
  getSettings: () => ipcRenderer.invoke('quota:get-settings'),
  setSettings: (payload) => ipcRenderer.invoke('quota:set-settings', payload),
  getPollingState: () => ipcRenderer.invoke('quota:get-polling-state'),
  setPollingState: (payload) => ipcRenderer.invoke('quota:set-polling-state', payload),
  fetchUsage: (payload) => ipcRenderer.invoke('quota:fetch-usage', payload),
  getWindowState: () => ipcRenderer.invoke('quota:get-window-state'),
  setWindowMode: (payload) => ipcRenderer.invoke('quota:set-window-mode', payload),
  setWindowPosition: (payload) => ipcRenderer.invoke('quota:set-window-position', payload),
  getVersion: () => ipcRenderer.invoke('quota:get-version'),
});
