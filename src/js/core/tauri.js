export const { invoke } = window.__TAURI__ ? window.__TAURI__.core : { invoke: async () => [] };
