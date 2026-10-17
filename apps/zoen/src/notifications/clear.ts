import { validLocalNotification } from './links.ts';

type Request = { identifier: string; content: { data?: unknown } };
export type NotificationCleanupPort = {
  presented(): Promise<Request[]>;
  scheduled(): Promise<Request[]>;
  dismiss(identifier: string): Promise<void>;
  cancel(identifier: string): Promise<void>;
  clearBadge(): Promise<unknown>;
  clearResponse(): Promise<void>;
};

export async function clearDeviceNotificationState(
  deviceId: string,
  port: NotificationCleanupPort,
) {
  const clear = async (read: () => Promise<Request[]>, remove: (id: string) => Promise<void>) => {
    const requests = await read();
    const results = await Promise.allSettled(
      requests
        .filter((request) => validLocalNotification(request.content.data, deviceId))
        .map((request) => Promise.resolve().then(() => remove(request.identifier))),
    );
    if (results.some((result) => result.status === 'rejected')) throw new Error('cleanup failed');
  };
  const clearRequests = async () => {
    let failed = false;
    // A pending request may deliver while cancellation is running. Read the
    // presented list only after every matching cancellation has settled.
    for (const [read, remove] of [
      [port.scheduled, port.cancel],
      [port.presented, port.dismiss],
    ] as const) {
      try {
        await clear(read, remove);
      } catch {
        failed = true;
      }
    }
    if (failed) throw new Error('cleanup failed');
  };
  const results = await Promise.allSettled([
    clearRequests(),
    Promise.resolve().then(port.clearBadge),
    Promise.resolve().then(port.clearResponse),
  ]);
  if (results.some((result) => result.status === 'rejected'))
    throw new Error(
      'Não foi possível apagar todas as notificações deste aparelho. Tente novamente.',
    );
}
