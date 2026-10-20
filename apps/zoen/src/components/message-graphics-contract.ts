/** UI-only geometry observed at Telegram iOS f1dd7a2; no message or delivery authority. */
export type MessageBubbleGraphicProps = {
  ownerKey: string;
  incoming: boolean;
  above: boolean;
  below: boolean;
  fill: string;
  /** Only an admitted parent layout pass may enable the pinned contents transition. */
  animateChanges?: boolean;
};
export type MessageReceiptGraphicProps = {
  ownerKey?: string;
  shape: 'clock' | 'single' | 'double';
  color: string;
};

export function messageBubbleGeometry({
  incoming,
  above,
  below,
}: Pick<MessageBubbleGraphicProps, 'incoming' | 'above' | 'below'>) {
  return {
    topLeft: incoming && above ? 8 : 16,
    topRight: !incoming && above ? 8 : 16,
    bottomLeft: incoming && below ? 8 : 16,
    bottomRight: !incoming && below ? 8 : 16,
    tail: !below,
  };
}

export const MESSAGE_GRAPHICS_MOTION = {
  contents: { duration: 0.42, curve: 'easeInEaseOut' },
  acknowledgement: { from: 1.3, to: 1, duration: 0.1, curve: 'easeInEaseOut' },
  clock: { frameDuration: 6, handDuration: 1, curve: 'linear' },
} as const;
