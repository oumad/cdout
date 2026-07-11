import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  message: string;
}

/**
 * Top-level error boundary. Without this, any uncaught throw during render or
 * in a lifecycle/effect white-screens the whole webview — which is what a user
 * would perceive as the app "losing connection" / going blank. The boundary
 * catches the throw, logs it, and shows a dark-themed recovery screen with a
 * Reload button instead of an unrecoverable blank page.
 */
export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, message: "" };
  }

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return {
      hasError: true,
      message: error instanceof Error ? error.message : String(error),
    };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Surface the full stack + component trace to the console so a dev build
    // (or an attached devtools) can diagnose it. Kept console-only to avoid a
    // dependency on any Tauri command from the boundary itself.
    console.error("[ErrorBoundary] Uncaught UI error:", error, info.componentStack);
  }

  private handleReload = () => {
    // Full reload is the pragmatic recovery: it re-runs the app from a clean
    // slate. Session state is persisted server-side, so nothing is lost.
    window.location.reload();
  };

  render() {
    if (!this.state.hasError) return this.props.children;
    return (
      <div className="flex flex-col items-center justify-center h-screen bg-gray-950 text-gray-200 font-sans select-none px-8">
        <AlertTriangle size={40} className="text-amber-500 mb-4" />
        <h1 className="text-lg font-semibold mb-2">Something went wrong</h1>
        <p className="text-sm text-gray-400 max-w-md text-center mb-1">
          The interface hit an unexpected error and stopped rendering. Your
          sessions are saved — reloading starts fresh without losing them.
        </p>
        {this.state.message && (
          <p className="text-xs text-gray-600 font-mono max-w-md text-center mb-6 break-words">
            {this.state.message}
          </p>
        )}
        <button
          onClick={this.handleReload}
          className="flex items-center gap-2 px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg text-sm font-medium transition"
        >
          <RefreshCw size={14} />
          Reload
        </button>
      </div>
    );
  }
}
