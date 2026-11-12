import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button, EmptyState } from "@/ui";

/** Last line of defence: a calm message instead of a blank window. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Margin hit an error it could not recover from", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="nt-crash">
        <EmptyState
          art="note"
          title="Margin hit a problem"
          actions={
            <Button variant="primary" onClick={() => window.location.reload()}>
              Reload Margin
            </Button>
          }
        >
          {`Your notes are safe. Reload to carry on. (${this.state.error.message})`}
        </EmptyState>
      </div>
    );
  }
}
