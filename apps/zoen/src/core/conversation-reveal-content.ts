import type { ConversationRevealItem } from './conversation-reveal';

export type ConversationContentLayout = Readonly<{
  y: number;
  width: number;
  height: number;
  anchor?: Readonly<{ y: number; width: number; height: number }>;
  anchorPending?: boolean;
}>;
type RowLayout = Readonly<{
  y: number;
  width: number;
  height: number;
  isHeightMeasured?: boolean;
  isWidthMeasured?: boolean;
}>;

/** The date separator belongs to the row, but is not the selected message.
 * A recycled or rotated row needs its own matching content measurement before
 * an estimated seek may be acknowledged as revealing the message. */
export function conversationRevealContent(
  row: RowLayout,
  content: ConversationContentLayout | null,
): Pick<ConversationRevealItem, 'y' | 'height' | 'measured' | 'anchor' | 'anchorPending'> {
  if (
    !content ||
    ![content.y, content.width, content.height, row.width, row.height].every(Number.isFinite) ||
    content.y < 0 ||
    content.height <= 0 ||
    content.width <= 0 ||
    Math.abs(content.width - row.width) > 0.5 ||
    Math.abs(content.y + content.height - row.height) > 0.5
  )
    return { y: row.y, height: row.height, measured: false };
  return {
    y: row.y + content.y,
    height: content.height,
    measured: row.isHeightMeasured === true && row.isWidthMeasured === true,
    ...(content.anchorPending ? { anchorPending: true } : {}),
    ...(content.anchor &&
    [content.anchor.y, content.anchor.width, content.anchor.height].every(Number.isFinite) &&
    content.anchor.y >= 0 &&
    content.anchor.height > 0 &&
    content.anchor.width > 0 &&
    content.anchor.width <= content.width + 0.5 &&
    content.anchor.y + content.anchor.height <= content.height + 0.5
      ? { anchor: { y: row.y + content.y + content.anchor.y, height: content.anchor.height } }
      : {}),
  };
}
