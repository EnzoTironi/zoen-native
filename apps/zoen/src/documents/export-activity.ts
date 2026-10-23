/** Ephemeral route focus and one in-flight share; never durable document state. */
export class DocumentExportActivity {
  private focused: object | null = null;
  private flight: object | null = null;

  focus(): () => void {
    const lease = {};
    this.focused = lease;
    this.flight = null;
    return () => {
      if (this.focused === lease) {
        this.focused = null;
        this.flight = null;
      }
    };
  }

  begin(): { current(): boolean; finish(): boolean } | null {
    if (!this.focused || this.flight) return null;
    const lease = this.focused;
    const operation = {};
    this.flight = operation;
    const current = () => this.focused === lease && this.flight === operation;
    return {
      current,
      finish: () => {
        if (!current()) return false;
        this.flight = null;
        return true;
      },
    };
  }
}
