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
        <div className="err-state">
          <div className="err-icon">⚠️</div>
          <h3 className="err-title">界面渲染出错</h3>
          <p className="err-text">发生了意外的渲染错误。这通常不影响后端运行，您可以尝试重试。</p>
          {this.state.error && (
            <pre className="err-detail">{this.state.error.message}</pre>
          )}
          <button className="err-retry" onClick={this.handleReset}>重试</button>
        </div>
      );
    }

    return this.props.children;
  }
}
