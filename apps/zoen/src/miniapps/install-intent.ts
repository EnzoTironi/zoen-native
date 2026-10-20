/** One mount retains only the immutable creation intent's idempotency token.
 * Canonical creation and duplicate prevention remain in the Rust journal. */
export function installIntent() {
  let pending: { key: string; id: Promise<string> } | null = null;
  return (key: string, createId: () => Promise<string>): Promise<string> => {
    if (pending?.key === key) return pending.id;
    const current = { key, id: Promise.resolve().then(createId) };
    pending = current;
    void current.id.catch(() => {
      if (pending === current) pending = null;
    });
    return current.id;
  };
}
