// 类型定义

export interface AppInfo {
  id: string;
  name: string;
  url: string;
  icon_url?: string;
  installed_at: number;
  display_mode: string;
}

export interface CommandResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface RunningPwa {
  appId: string;
  url: string;
  name: string;
  lastAccessed: number;
  scrollY?: number;
}

export interface PwaSnapshot {
  appId: string;
  url: string;
  name: string;
  scrollY: number;
  timestamp: number;
}

export interface ProxySettings {
  enabled: boolean;
  proxy_type: "http" | "https" | "socks5";
  host: string;
  port: number;
  username: string;
  password: string;
}

export interface BrowserHistoryItem {
  url: string;
  title: string;
  timestamp: number;
}

export type ViewMode = 'apps' | 'browser' | 'pwa';
