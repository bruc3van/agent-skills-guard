import { Component, type ErrorInfo, type ReactNode } from "react";
import i18next from "i18next";

interface Props {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  reset = () => this.setState({ error: null });

  render() {
    const { error } = this.state;
    if (error) {
      if (this.props.fallback) {
        return this.props.fallback(error, this.reset);
      }
      return (
        <div className="flex flex-col items-center justify-center p-8 text-center" style={{ minHeight: '200px' }}>
          <div className="text-4xl mb-4">&#x26A0;&#xFE0F;</div>
          <h3 className="text-lg font-semibold text-red-400 mb-2">
            {i18next.t('error_boundary.title', 'Something went wrong')}
          </h3>
          <p className="text-sm text-gray-400 mb-4 max-w-md">
            {i18next.t('error_boundary.message', 'An unexpected error occurred. Please try refreshing or restarting the app.')}
          </p>
          <details className="text-xs text-gray-500 mb-4 max-w-lg">
            <summary>{i18next.t('error_boundary.details', 'Error details')}</summary>
            <pre className="mt-2 p-2 bg-gray-800 rounded text-left overflow-auto">
              {error.message}
            </pre>
          </details>
          <button
            onClick={this.reset}
            className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700"
          >
            {i18next.t('error_boundary.retry', 'Retry')}
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
