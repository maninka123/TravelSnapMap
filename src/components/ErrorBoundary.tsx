import { Component, type ErrorInfo, type ReactNode } from "react";

/** Contains a rendering failure to one area of the app, with a way to recover. */
export class ErrorBoundary extends Component<{ children: ReactNode; label?: string }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("TravelSnapMap UI error", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="empty">
        <div className="empty-icon">⚠️</div>
        <h3>{this.props.label ?? "This view"} hit a problem</h3>
        <p className="muted small" style={{ maxWidth: 420 }}>{this.state.error.message}</p>
        <button className="btn primary" onClick={() => this.setState({ error: null })}>Try again</button>
      </div>
    );
  }
}
