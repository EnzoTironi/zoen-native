import { surfaceLinkPath } from '../native-surfaces/links';
import { nativeLinkPath } from '../notifications/links';
import { nativeProviderReturnPath } from '../auth/native-links';
import { identityProviders } from '../runtime-config';

export function redirectSystemPath({ path }: { path: string; initial: boolean }) {
  return (
    nativeProviderReturnPath(path, identityProviders.google?.iosClientId) ??
    surfaceLinkPath(path) ??
    nativeLinkPath(path)
  );
}
