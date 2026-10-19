import type { ComponentProps } from 'react';
import { Avatar } from '../components/primitives';
import type { Persona } from '../core/generated/Persona';
import { useProfilePhoto } from './use-profile-photo';

type Props = Omit<ComponentProps<typeof Avatar>, 'uri'> & { person?: Persona | null };
export function ProfileAvatar({ person, ...props }: Props) {
  const uri = useProfilePhoto(person?.id, person?.photo_sha256, person?.photo_ready ?? false);
  return <Avatar {...props} uri={uri} />;
}
