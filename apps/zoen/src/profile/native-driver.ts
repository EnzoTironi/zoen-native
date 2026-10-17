import NativeCore from '../../modules/zoen-core/src/ZoenCoreModule';
import type { ProfilePhotoDriver } from './types';

export function createNativeProfilePhotoDriver(): ProfilePhotoDriver {
  function core() {
    if (!NativeCore) throw new Error('Abra o build de desenvolvimento do Zoen.');
    return NativeCore;
  }
  return {
    async save(profile) {
      await core().saveProfile(JSON.stringify(profile));
    },
    async materialize(ownerId, deviceId, identityId, expectedSha256) {
      return JSON.parse(
        await core().materializeProfilePhoto(ownerId, deviceId, identityId, expectedSha256),
      );
    },
    async release(uri) {
      await core().releaseProfilePhoto(uri);
    },
  };
}
