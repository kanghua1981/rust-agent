/**
 * 跨前端（web-ui / tauri）的「点击远端文件 → 本地动作」统一分派。
 *
 * 根据运行环境把点击文件变成正确的本地动作：
 *   - Tauri + 本地服务器  → `open_file_external` 零传输直接打开（保留原文件，编辑原地生效）
 *   - Tauri + 远端服务器  → `<a download>` 锚点下载（浏览器原生，服务端返回 attachment）
 *   - 纯浏览器 (web-ui)   → 锚点导航触发浏览器原生下载
 *
 * 后端配套：agent server 的 `GET /file?path=...&token=...` HTTP 端点（server.rs）。
 * 注意：远端打开依赖服务端已部署含 `/file` 端点的新版 agent。
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

/** 用浏览器原生下载打开文件（Tauri webview 同样适用）。 */
function anchorDownload(url: string, filename: string): void {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

/**
 * 打开一个位于「当前连接服务器」上的文件。
 *
 * @param serverUrl  服务器地址（ws://host:port[/...]）
 * @param path       文件路径（相对项目目录或绝对路径，与 list_dir 返回的 entry.path 一致）
 * @param token      集群 token（若有）
 * @param workdir    服务器项目目录（绝对路径，仅 Tauri 本地直开需要用来拼绝对路径）
 */
export async function openFileFromServer(
  serverUrl: string,
  path: string,
  token: string | undefined,
  workdir: string | undefined,
): Promise<void> {
  const filename = basename(path);
  const url = buildFileUrl(serverUrl, path, token);

  // Tauri + 本地服务器：直接打开真实文件（零传输，编辑原地生效）
  if (isTauri()) {
    const invoke = tauriInvoke();
    if (invoke && isLocalServer(serverUrl)) {
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
  }

  // 其余（Tauri 远端 / 纯浏览器）：锚点导航 → 浏览器原生下载
  anchorDownload(url, filename);
}
