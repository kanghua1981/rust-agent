import React, { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  /** Optional callback invoked after an error is caught. */
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * Error boundary that catches rendering errors in child components.
 * Prevents a single component crash from taking down the whole UI.
 */
export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    // Always log to console in dev mode
    console.error('[ErrorBoundary] caught:', error, errorInfo.componentStack);
    this.props.onError?.(error, errorInfo);
  }

  private handleReset = (): void => {
    this.setState({ hasError: false, error: null });
  };

  public render(): ReactNode {
    if (this.state.hasError) {
      // Fallback UI
      return (
        <div style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '40px',
          color: 'var(--text2)',
          gap: '12px',
        }}>
          <div style={{
            width: '56px', height: '56px', borderRadius: '50%',
            background: 'rgba(239,68,68,0.12)',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            fontSize: '26px',
          }}>
            ⚠️
          </div>
          <h3 style={{ fontSize: '16px', fontWeight: '600', color: 'var(--text)', margin: 0 }}>
            界面渲染出错
          </h3>
          <p style={{
            fontSize: '13px', color: 'var(--text3)',
            textAlign: 'center', maxWidth: '480px', lineHeight: '1.5',
            margin: 0,
          }}>
            发生了意外的渲染错误。这通常不影响后端运行，您可以尝试重试。
          </p>
          {this.state.error && (
            <pre style={{
              maxWidth: '95%', maxHeight: '120px', overflow: 'auto',
              background: 'var(--bg3)', border: '1px solid var(--border)',
              borderRadius: '8px', padding: '10px 14px',
              fontSize: '11px', color: 'var(--text3)',
              fontFamily: 'monospace',
              whiteSpace: 'pre-wrap', wordBreak: 'break-word',
            }}>
              {this.state.error.message}
            </pre>
          )}
          <button
            onClick={this.handleReset}
            style={{
              marginTop: '8px',
              padding: '8px 20px',
              background: 'var(--accent)',
              color: '#fff',
              border: 'none',
              borderRadius: '8px',
              fontSize: '13px',
              fontWeight: '500',
              cursor: 'pointer',
            }}
          >
            重试
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
