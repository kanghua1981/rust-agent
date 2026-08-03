/**
 * 跨前端（web-ui / tauri）的「点击远端文件 → 本地动作」统一分派。
 *
 * 根据运行环境把点击文件变成正确的本地动作：
 *   - Tauri + 本地服务器  → `open_file_external` 零传输直接打开（保留原文件，编辑原地生效）
 *   - Tauri + 远端服务器  → `open_remote_file`：Rust 流式下载到本地缓存 → 本地打开
 *   - 纯浏览器 (web-ui)   → 锚点导航触发浏览器原生下载（后端返回 Content-Disposition: attachment）
 *
 * 后端配套：agent server 的 `GET /file?path=...&token=...` HTTP 端点（server.rs）。
 * Tauri 配套：`open_remote_file` Rust 命令（src-tauri/src/main.rs）。
 */
import { isTauri, tauriInvoke } from './export';

/** `ws://host:port/...` → `http://host:port/file?path=...&token=...` */
export function buildFileUrl(serverUrl: string, path: string, token?: string): string {
  const http = serverUrl.replace(/^ws(s?):/, 'http$1:');
  const u = new URL(http);
  u.pathname = '/file';
  u.search = '';
  u.searchParams.set('path', path);
  if (token) u.searchParams.set('token', token);
  return u.toString();
}

/** 服务器是否在本机（Tauri 本地直开的前提，远端必须走下载）。 */
export function isLocalServer(serverUrl: string): boolean {
  try {
    const u = new URL(serverUrl.replace(/^ws(s?):/, 'http$1:'));
    const host = u.hostname;
    return host === 'localhost' || host === '127.0.0.1' || host === '::1';
  } catch {
    return false;
  }
}

function basename(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() || 'download';
}

/**
 * 打开一个位于「当前连接服务器」上的文件。
 *
 * @param serverUrl  服务器地址（ws://host:port[/...]）
 * @param path       文件路径（相对项目目录或绝对路径，与 list_dir 返回的 entry.path 一致）
 * @param token      集群 token（若有）
 * @param workdir    服务器项目目录（绝对路径，仅 Tauri 本地直开需要用来拼绝对路径）
 * @param fallbackWsOpen 兜底：让服务器端打开（老行为）
 */
export async function openFileFromServer(
  serverUrl: string,
  path: string,
  token: string | undefined,
  workdir: string | undefined,
  fallbackWsOpen: () => void,
): Promise<void> {
  const filename = basename(path);
  const url = buildFileUrl(serverUrl, path, token);

  if (isTauri()) {
    const invoke = tauriInvoke();
    if (!invoke) { fallbackWsOpen(); return; }

    // 1) 本地服务器：直接打开真实文件（零传输，编辑原地生效）
    if (isLocalServer(serverUrl)) {
      const abs = path.startsWith('/')
        ? path
        : workdir
          ? `${workdir.replace(/\/+$/, '')}/${path}`
          : '';
      if (abs) {
        try {
          await invoke('open_file_external', { path: abs });
          return;
        } catch (e) {
          console.warn('[fileTransfer] 本地直接打开失败，尝试下载方式:', e);
        }
      }
    }

    // 2) 远端服务器：Rust 流式下载到本地缓存 → 本地打开
    try {
      await invoke('open_remote_file', { url, filename });
      return;
    } catch (e) {
      console.warn('[fileTransfer] 下载打开失败，退回服务器端打开:', e);
    }

    // 3) 兜底：老行为——让服务器端打开
    fallbackWsOpen();
    return;
  }

  // 纯浏览器（web-ui）：锚点导航 → 浏览器原生下载
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}
