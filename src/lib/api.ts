import { invoke } from "@tauri-apps/api/core";

/** Status snapshot returned by the Rust backend's `app_status` command. */
export interface AppStatus {
  app_version: string;
  data_dir: string;
  config_path: string;
  database_path: string;
  webview_dir: string;
  database_ready: boolean;
  schema_version: number;
  runtime_ready: boolean;
  config_version: number;
  window_width: number;
  window_height: number;
  log_level: string;
}

/** Fetch the backend status snapshot for the overview page. */
export function fetchAppStatus(): Promise<AppStatus> {
  return invoke<AppStatus>("app_status");
}
