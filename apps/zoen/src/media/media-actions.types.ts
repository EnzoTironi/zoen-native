export type MediaPickKind = 'document' | 'photo' | 'camera' | 'audio';

export type MediaActionsProps = Readonly<{
  visible: boolean;
  hasVoiceEditor: boolean;
  onClose: () => void;
  onChoose: (kind: MediaPickKind) => void;
}>;
