import type { ConsentChoice, ConsentRequest } from './device-consent-queue';

export type DeviceConsentProps = {
  request: ConsentRequest | null;
  phase: 'idle' | 'presented' | 'dismissing';
  title: string;
  scope: string;
  onChoose(id: number, choice: ConsentChoice): void;
  onDismiss(id: number): void;
};
