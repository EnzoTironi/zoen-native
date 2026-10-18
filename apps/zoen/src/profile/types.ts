export const MAX_PROFILE_PHOTO_BYTES = 5 * 1024 * 1024;
export type ProfileSave = {
  ownerId: string;
  deviceId: string;
  name: string;
  bio: string;
  photo: 'keep' | 'remove' | 'set';
  uri?: string;
};
export type LocalProfilePhoto = {
  uri: string;
  identityId: string;
  sha256: string;
  mime: string;
};
export interface ProfilePhotoDriver {
  save(profile: ProfileSave): Promise<void>;
  materialize(
    ownerId: string,
    deviceId: string,
    identityId: string,
    expectedSha256: string,
  ): Promise<LocalProfilePhoto | null>;
  release(uri: string): Promise<void>;
}
export type PhotoDraft = { uri: string; dispose(): void };
