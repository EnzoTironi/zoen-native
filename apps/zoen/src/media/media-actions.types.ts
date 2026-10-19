export type MediaPickKind = 'document' | 'photo' | 'camera' | 'audio';

export type MediaActionsProps = Readonly<{
  disabled: boolean;
  hasVoiceEditor: boolean;
  onChoose: (kind: MediaPickKind) => void;
}>;
