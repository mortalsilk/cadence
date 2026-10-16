import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import type {
  DaySummary,
  EntryInput,
  ImportPreview,
  Mode,
  Session,
  SessionInput,
  Stint,
  View,
} from './types';
export const desktop = '__TAURI_INTERNALS__' in window;
export const api = {
  state: () => invoke<View>('get_state'),
  start: (mode: Mode, label: string, minutes: number | null) =>
    invoke<View>('start_stint', { mode, label, minutes }),
  action: (action: string) => invoke<View>('timer_action', { action }),
  save: (label: string, description: string) =>
    invoke<Stint>('save_stint', { label, description }),
  startSession: (input: SessionInput) =>
    invoke<View>('start_session', { input }),
  sessionAction: (action: string) => invoke<View>('session_action', { action }),
  saveSession: (input: SessionInput) =>
    invoke<Session>('save_session', { input }),
  list: () => invoke<Stint[]>('list_stints'),
  listSessions: () => invoke<Session[]>('list_sessions'),
  summaries: () => invoke<Record<string, DaySummary>>('daily_summaries'),
  edit: (id: string | null, input: EntryInput) =>
    invoke<Stint>('edit_stint', { id, input }),
  delete: (id: string) => invoke<void>('delete_stint', { id }),
  editSession: (id: string, input: SessionInput) =>
    invoke<Session>('edit_session', { id, input }),
  deleteSession: (id: string) => invoke<void>('delete_session', { id }),
  onState: (fn: (view: View) => void) =>
    listen<View>('timer-state', (event) => fn(event.payload)),
  onError: (fn: (message: string) => void) =>
    listen<string>('cadence-error', (event) => fn(event.payload)),
  async export(format: 'csv' | 'json') {
    const path = await save({
      defaultPath: `cadence-${new Date().toISOString().slice(0, 10)}.${format}`,
      filters: [
        {
          name: format === 'csv' ? 'CSV spreadsheet' : 'cadence backup',
          extensions: [format],
        },
      ],
    });
    if (!path) return false;
    await invoke('export_file', { path, format });
    return true;
  },
  async preview() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'cadence backup', extensions: ['json'] }],
    });
    return typeof path === 'string'
      ? invoke<ImportPreview>('preview_import', { path })
      : null;
  },
  import: () => invoke<ImportPreview>('confirm_import'),
};
