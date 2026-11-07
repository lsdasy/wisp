// Типы повторяют сериализацию из crates/lumen-core (serde, camelCase).
// Если меняете структуру в Rust — поменяйте и здесь.

import { invoke } from "@tauri-apps/api/core";

export type ItemKind = "app" | "command" | "file" | "folder" | "calc" | "shell" | "plugin";

export interface Tile {
  text: string;
  bg: string;
  fg: string;
}

export interface ItemView {
  id: string;
  kind: ItemKind;
  title: string;
  subtitle: string;
  tag: string;
  actionLabel: string;
  danger: boolean;
  favorite: boolean;
  tile: Tile | null;
}

export interface SectionView {
  title: string;
  items: ItemView[];
}

export interface CalcView {
  id: string;
  expression: string;
  display: string;
  words: string;
}

export interface SearchResponse {
  requestId: number;
  cancelled: boolean;
  calc: CalcView | null;
  sections: SectionView[];
}

export type ExecOutcome =
  | { type: "done"; message: string; hide: boolean }
  | { type: "needsConfirmation"; id: string; title: string }
  | { type: "output"; title: string; stdout: string; stderr: string; code: number | null; timedOut: boolean }
  | { type: "items"; title: string; items: ItemView[] };

export type ThemeSetting = "system" | "dark" | "light";

export interface UserCommand {
  keyword: string;
  title: string;
  command: string;
  shell: boolean;
  confirm: boolean;
}

export interface PluginGrant {
  enabled: boolean;
  allowExec: boolean;
  allowOpen: boolean;
}

export interface Settings {
  hotkey: string;
  theme: ThemeSetting;
  autostart: boolean;
  animations: boolean;
  hideOnBlur: boolean;
  recordHistory: boolean;
  confirmDangerous: boolean;
  showCommandOutput: boolean;
  maxResultsPerSection: number;
  indexRoots: string[];
  indexExcludes: string[];
  providers: { apps: boolean; files: boolean; commands: boolean; calculator: boolean; plugins: boolean };
  aliases: UserCommand[];
  plugins: Record<string, PluginGrant>;
}

export interface Status {
  apps: number;
  files: { indexing: boolean; files: number; truncated: boolean };
  pluginsDir: string;
}

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  permissions: ("exec" | "open")[];
  commands: string[];
  error: string | null;
  grant: PluginGrant;
}

export interface AppInfo {
  platform: "windows" | "macos" | "linux" | string;
  version: string;
  vibrancy: boolean;
}

export const api = {
  search: (query: string, requestId: number) => invoke<SearchResponse>("search", { query, requestId }),
  execute: (id: string, confirmed: boolean, query: string) => invoke<ExecOutcome>("execute", { id, confirmed, query }),
  reveal: (id: string) => invoke<void>("reveal", { id }),
  toggleFavorite: (id: string) => invoke<boolean>("toggle_favorite", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  getStatus: () => invoke<Status>("get_status"),
  reindex: () => invoke<void>("reindex"),
  listPlugins: () => invoke<PluginInfo[]>("list_plugins"),
  reloadPlugins: () => invoke<PluginInfo[]>("reload_plugins"),
  openPluginsDir: () => invoke<void>("open_plugins_dir"),
  clearHistory: () => invoke<void>("clear_history"),
  hideWindow: () => invoke<void>("hide_window"),
  openSettings: () => invoke<void>("open_settings"),
  appInfo: () => invoke<AppInfo>("app_info"),
  setEffectiveTheme: (dark: boolean) => invoke<boolean>("set_effective_theme", { dark }),
};

export const EVENTS = {
  shown: "lumen://shown",
  hideRequested: "lumen://hide-requested",
  settingsChanged: "lumen://settings-changed",
} as const;

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
