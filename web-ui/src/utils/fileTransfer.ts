/**
 * 「点击一个服务器上的文件」的本地动作。
 *
 * 文件永远在**服务器**上,所以「打开」只有三种互相独立的做法,与运行环境无关:
 *   1. 在应用内查看   → WS `read_file_content`(FileViewer 组件)
 *   2. 让服务器打开   → WS `open_file_external`(服务器进程执行编辑器)
 *   3. 取到本地       → 本文件:下载(GET /file)
 *
 * 只有「文件确实位于客户端同一个文件系统上」时,才谈得上「用本机程序直接打开」。
 * 那需要三个条件同时成立:
 *   - 跑在 Tauri 里(浏览器没有本地文件系统权限)
 *   - 服务器在本机(远端机器上的路径,本地没有)
 *   - 隔离模式是 normal(容器/沙盒下服务器报的是容器内路径,本机并不存在)
 *
 * 后端配套:agent server 的 `GET /file?path=...&token=...` HTTP 端点(server.rs)。
 */
import { tauriInvoke } from './export';

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

/** 服务器是否与客户端同机。 */
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

/** 触发浏览器原生下载(Tauri webview 同样适用)。 */
export function downloadFile(serverUrl: string, path: string, token?: string): void {
  const a = document.createElement('a');
  a.href = buildFileUrl(serverUrl, path, token);
  a.download = basename(path);
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

/**
 * 「用本机程序直接打开」在此环境下的可用性。
 *
 *   - `unsupported`:运行时根本没有本地文件系统权限(纯浏览器)→ 调用方应直接隐藏该动作
 *   - `blocked`:当前不可用,但原因是用户**可以改变**的(换本机服务器 / 切隔离模式)
 *                 → 调用方应显示但禁用,并把 reason 说清楚
 *   - `available`:文件就在客户端同一个文件系统上
 *
 * 判断依据是「真的拿得到 invoke」而不是某个运行时标记:Tauri v2 只在
 * withGlobalTauri 打开时才注入 window.__TAURI__。
 */
export type LocalOpenAvailability =
  | { kind: 'available' }
  | { kind: 'blocked'; reason: string }
  | { kind: 'unsupported' };

export function localOpenAvailability(
  serverUrl: string,
  isolation: string | undefined,
): LocalOpenAvailability {
  if (tauriInvoke() === null) return { kind: 'unsupported' };
  if (!isLocalServer(serverUrl)) {
    return { kind: 'blocked', reason: '文件在远端服务器上,本机没有这个路径 —— 用下载代替。' };
  }
  if (isolation !== 'normal') {
    const mode = isolation === 'sandbox' ? '沙盒' : '容器';
    return {
      kind: 'blocked',
      reason: `当前是${mode}隔离,服务器报的是容器内路径在本机不存在 —— 切到 normal 隔离可原地编辑。`,
    };
  }
  return { kind: 'available' };
}

/**
 * 用本机默认程序打开真实文件(零传输,编辑原地生效)。
 * 不可用时直接返回 false,不发起调用。
 */
export async function openFileLocally(
  serverUrl: string,
  path: string,
  workdir: string | undefined,
  isolation: string | undefined,
): Promise<boolean> {
  if (localOpenAvailability(serverUrl, isolation).kind !== 'available') return false;
  const invoke = tauriInvoke();
  if (!invoke) return false;

  const abs = path.startsWith('/')
    ? path
    : workdir
      ? `${workdir.replace(/\/+$/, '')}/${path}`
      : '';
  if (!abs) return false;

  try {
    await invoke('open_file_external', { path: abs });
    return true;
  } catch (e) {
    console.warn('[fileTransfer] 本机直接打开失败:', e);
    return false;
  }
}
