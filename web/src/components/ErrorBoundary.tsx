import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  label?: string;
}

interface State {
  error: Error | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error(`[ErrorBoundary: ${this.props.label ?? "unknown"}]`, error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          height: "100%", gap: 12, color: "var(--text-muted)", padding: 32,
        }}>
          <span style={{ fontSize: 32 }}>⚠️</span>
          <p style={{ fontWeight: 600, color: "var(--text)" }}>
            {this.props.label ?? "模块"}加载失败
          </p>
          <pre style={{
            fontSize: 11, background: "var(--bg-muted)", padding: "8px 12px",
            borderRadius: 6, maxWidth: 480, overflow: "auto", color: "var(--error)",
          }}>
            {this.state.error.message}
          </pre>
          <button
            style={{ fontSize: 12, padding: "5px 14px", background: "var(--accent)", color: "#fff", borderRadius: 6 }}
            onClick={() => this.setState({ error: null })}
          >
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
