/** Public session projection for composer presentation ports, never a Core or vault mock. */
export function composerOwnerFixture() {
  return {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'me',
        device_id: 'device',
        relay_url: 'https://relay.example',
        unlocked: true,
        setup_pending: false,
      },
    },
    store: { sessionEpoch: 1, isSessionRetiring: false },
  };
}
