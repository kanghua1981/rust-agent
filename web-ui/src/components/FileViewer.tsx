import React from 'react';
import type { OpenFileState } from '../types/agent';
import type { LocalOpenAvailability } from '../utils/fileTransfer';

interface Props {
  file: OpenFileState;
  /** Whether "open with a local program" is possible here, and why not if it is not. */
  localOpen: LocalOpenAvailability;
  onOpenLocally: (path: string) => void;
  onDownload: (path: string) => void;
  onClose: () => void;
}

/** Cap rendered text so the server's 2MB preview limit cannot lock up the panel. */
const MAX_RENDER_CHARS = 200_000;

function formatSize(bytes?: number): string {
  if (bytes === undefined) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * Shows a file that lives on the server. Viewing never needs a local copy, so
 * this works in a browser against a remote server just as well as in Tauri;
 * the two external actions are offered as extras rather than as the default.
 */
export const FileViewer: React.FC<Props> = ({
  file, localOpen, onOpenLocally, onDownload, onClose,
}) => {
  const name = file.path.split(/[\\/]/).filter(Boolean).pop() || file.path;
  const content = file.content ?? '';
  const clipped = content.length > MAX_RENDER_CHARS;

  return (
    <div className="file-view">
      <div className="file-view-head">
        <span className="file-view-name" title={file.path}>{name}</span>
        {file.size !== undefined && <span className="file-view-size">{formatSize(file.size)}</span>}
        <button className="icon-btn" onClick={onClose} title="关闭">✕</button>
      </div>

      <div className="file-view-actions">
        <button className="btn-ghost" onClick={() => onDownload(file.path)} title="下载到本地">⬇ 下载</button>
        {/* Hidden only when the runtime can never do it (a browser has no local
            filesystem access); shown disabled when the cause is fixable. */}
        {localOpen.kind !== 'unsupported' && (
          <button
            className="btn-ghost"
            disabled={localOpen.kind === 'blocked'}
            onClick={() => { if (localOpen.kind === 'available') onOpenLocally(file.path); }}
            title="用本机默认程序打开,编辑原地生效"
          >✎ 本机打开</button>
        )}
      </div>
      {localOpen.kind === 'blocked' && <div className="file-view-hint">{localOpen.reason}</div>}

      <div className="file-view-body">
        {file.loading ? (
          <div className="file-view-note">加载中…</div>
        ) : file.error && !file.content ? (
          <div className="file-view-error">{file.error}</div>
        ) : file.binary ? (
          <div className="file-view-note">
            二进制文件,无法预览{file.size !== undefined ? `(${formatSize(file.size)})` : ''} —— 请下载或在本机打开。
          </div>
        ) : (
          <>
            {file.truncated && <div className="file-view-note">文件超出服务端 2MB 预览上限。</div>}
            <pre className="file-view-pre">{clipped ? content.slice(0, MAX_RENDER_CHARS) : content}</pre>
            {clipped && <div className="file-view-note">仅显示前 {MAX_RENDER_CHARS.toLocaleString()} 个字符。</div>}
          </>
        )}
      </div>
    </div>
  );
};
