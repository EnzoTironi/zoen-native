export type ComposerFittingSnapshot = Readonly<{
  text: string;
  label: string;
  placeholder: string;
  width: number;
  fontSize: number;
  context: string;
  tag: number | null;
}>;
export type ComposerFittingRequest = Readonly<
  Omit<ComposerFittingSnapshot, 'tag'> & { tag: number; nonce: number }
>;
export type ComposerFittingMeasurement = Readonly<{
  tag: number;
  nonce: number;
  width: number;
  height: number;
  fontSize: number;
}>;

/** Native fitting changes height only. Each request owns its exact immutable
 * snapshot; a content-size notification requests it again without trusting the
 * notification's numeric height. Missing native layout gets at most two frames. */
export function createComposerFitting(
  measure: (request: ComposerFittingRequest) => Promise<ComposerFittingMeasurement | null>,
  apply: (height: number, snapshot: ComposerFittingSnapshot) => void,
  scheduleRetry: (work: () => void) => () => void,
) {
  let revision = 0;
  let nonce = 0;
  let disposed = false;
  let cancelRetry: (() => void) | null = null;
  const current = (captured: number) => !disposed && captured === revision;
  const cancel = () => {
    ++revision;
    cancelRetry?.();
    cancelRetry = null;
  };
  async function attempt(
    snapshot: ComposerFittingSnapshot,
    values: Omit<ComposerFittingRequest, 'nonce'>,
    captured: number,
    retries: number,
  ) {
    if (!current(captured)) return;
    const request = { ...values, nonce: ++nonce };
    let result: ComposerFittingMeasurement | null;
    try {
      result = await measure(request);
    } catch {
      result = null;
    }
    if (!current(captured)) return;
    if (
      result &&
      result.tag === request.tag &&
      result.nonce === request.nonce &&
      Number.isFinite(result.width) &&
      result.width > 0 &&
      Number.isFinite(result.fontSize) &&
      result.fontSize > 0 &&
      Number.isFinite(result.height) &&
      result.height > 0 &&
      Math.abs(result.width - request.width) <= 1 &&
      Math.abs(result.fontSize - request.fontSize) <= 0.5
    ) {
      apply(Math.max(52, Math.min(160, result.height)), snapshot);
      return;
    }
    if (retries === 2) return;
    cancelRetry = scheduleRetry(() => {
      if (!current(captured)) return;
      cancelRetry = null;
      void attempt(snapshot, values, captured, retries + 1).catch(() => {});
    });
  }
  return {
    request(snapshot: ComposerFittingSnapshot) {
      cancel();
      if (
        disposed ||
        snapshot.tag === null ||
        !Number.isSafeInteger(snapshot.tag) ||
        snapshot.tag <= 0 ||
        !Number.isFinite(snapshot.width) ||
        snapshot.width <= 0 ||
        !Number.isFinite(snapshot.fontSize) ||
        snapshot.fontSize <= 0 ||
        !snapshot.label
      )
        return;
      const values = { ...snapshot, tag: snapshot.tag };
      void attempt(snapshot, values, revision, 0).catch(() => {});
    },
    cancel,
    dispose() {
      cancel();
      disposed = true;
    },
  };
}
