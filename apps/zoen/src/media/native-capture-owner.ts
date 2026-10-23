export type CaptureLease = { owner: object; recover: () => Promise<void> };
let current: CaptureLease | null = null;
export const nativeCaptureOwner = {
  get: (): CaptureLease | null => current,
  set: (lease: CaptureLease | null): void => {
    current = lease;
  },
  stop: async (): Promise<void> => {
    await current?.recover();
  },
};
