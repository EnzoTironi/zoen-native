export async function unavailableCapability(options:{onboarding?:boolean}={}): Promise<string | undefined> {
  if (!window.isSecureContext) return 'Open Zoen over HTTPS or on localhost to protect your browser device.';
  if (!options.onboarding&&(!window.crossOriginIsolated || typeof SharedArrayBuffer === 'undefined')) {
    return 'This host does not support the isolation required for encrypted browser messaging.';
  }
  if (!crypto.subtle || !window.indexedDB || !window.Worker || !navigator.locks) {
    return 'This browser does not support secure device storage and encrypted messaging. Open Zoen on your phone.';
  }
  try {
    const key = await crypto.subtle.generateKey('Ed25519', false, ['sign', 'verify']);
    if (!('privateKey' in key) || key.privateKey.extractable) return 'This browser cannot protect an Ed25519 device key.';
  } catch { return 'This browser does not support Ed25519 device keys. Open Zoen on your phone.'; }
  return undefined;
}
