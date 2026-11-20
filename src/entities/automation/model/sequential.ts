import { invoke } from "../../../shared/api/invoke";

export type SequentialConfig = {
  start: number;
  end: number;
  url: string;
  wait_seconds: number;
};

export type SequentialRecord = {
  name: string;
  status: "completed" | "failed" | "cancelled";
  error?: string | null;
};

export type SequentialState = {
  running: boolean;
  phase: string;
  current?: string | null;
  current_id?: string | null;
  total: number;
  completed: number;
  failed: number;
  results: SequentialRecord[];
  error?: string | null;
};

export const sequentialStart = (config: SequentialConfig) =>
  invoke<void>("sequential_start", { config });
export const sequentialStatus = () =>
  invoke<SequentialState>("sequential_status");
export const sequentialStop = () => invoke<void>("sequential_stop");
