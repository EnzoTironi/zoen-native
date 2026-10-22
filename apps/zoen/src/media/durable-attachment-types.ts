import { type AttachmentAsset } from './attachment-draft';

export type Preview = AttachmentAsset & { release: () => Promise<void> };
export type ApprovalPresentation = { closing: boolean };
