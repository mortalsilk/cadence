export type Mode = 'count_up' | 'countdown' | 'manual';
export type Phase = 'running' | 'paused' | 'review';

export interface Active {
  id: string;
  label: string;
  mode: Mode;
  target_ms: number | null;
  elapsed_ms: number;
  started_at: string;
  finished_at: string | null;
  date: string | null;
  phase: Phase;
  session_id: string | null;
}

export interface ActiveSession {
  id: string;
  label: string;
  description: string;
  elapsed_ms: number;
  started_at: string;
  finished_at: string | null;
  date: string | null;
  phase: Phase;
}

export interface Stint {
  id: string;
  label: string;
  description: string;
  mode: Mode;
  target_ms: number | null;
  elapsed_ms: number;
  started_at: string;
  finished_at: string;
  date: string;
  session_id: string | null;
}

export interface Session {
  id: string;
  label: string;
  description: string;
  elapsed_ms: number;
  started_at: string;
  finished_at: string;
  date: string;
}

export interface View {
  active: Active | null;
  active_session: ActiveSession | null;
  warning?: string | null;
}
export interface DaySummary {
  count: number;
  session_count: number;
  elapsed_ms: number;
}
export interface EntryInput {
  label: string;
  description: string;
  date: string;
  elapsed_ms: number;
  session_id: string | null;
}
export interface SessionInput {
  label: string;
  description: string;
}
export interface ImportPreview {
  total: number;
  added: number;
  skipped: number;
}
