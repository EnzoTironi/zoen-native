import type { NormalDraftChannel } from '../core/normal-message-draft-service';
import {
  stageReviewedAttachment,
  type ReviewedAttachmentStage,
} from './stage-reviewed-attachment.ts';
import { locationGeoJSON, LOCATION_FILE } from './location-file.ts';
import type { StaticLocation } from './types';
import { articleText, contactVCard, type SharedContact } from './attachment-content.ts';

/** A native capture updates the existing canonical text draft; this never sends. */
export async function captureArticleDraft(
  channel: NormalDraftChannel,
  capture: () => Promise<string>,
  current: () => boolean,
): Promise<void> {
  if (!current()) throw new Error('retired');
  const text = articleText(await capture());
  if (!current()) throw new Error('retired');
  channel.controller.setText(channel.scope, text);
  await channel.controller.flush(channel.scope);
  if (!current()) throw new Error('retired');
}
/** Contact and Location share the existing file-ingress transaction. */
export function stageContactAttachment(
  options: ReviewedAttachmentStage & { contact: SharedContact },
): Promise<void> {
  return stageReviewedAttachment({
    ...options,
    text: contactVCard(options.contact),
    name: 'contact.vcf',
    mime: 'text/vcard',
  });
}
export function stageLocationAttachment(
  options: ReviewedAttachmentStage & { point: StaticLocation },
): Promise<void> {
  return stageReviewedAttachment({
    ...options,
    text: locationGeoJSON(options.point),
    ...LOCATION_FILE,
  });
}
