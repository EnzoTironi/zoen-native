// SPDX-License-Identifier: GPL-2.0-or-later
// Source-observed Telegram-iOS PeerInfo header/list/pane contracts, contributors.
// Revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPLv3 option selected.
export const peerInfo = Object.freeze({
  avatar: 100,
  avatarTop: 22,
  titleGap: 9,
  titleFont: 28,
  titleLine: 34,
  subtitleGap: 1,
  subtitleFont: 17,
  subtitleLine: 21,
  edge: 16,
  actionGap: 8,
  actionHeight: 58,
  actionIconFrame: 40,
  actionFont: 11,
  row: 52,
  corner: 10,
  tabs: 40,
});

/** Non-expanded narrow PeerInfoHeaderNode title lock; safe-top cancels from both terms. */
export function profileTitleCollapseOffset(hasSubtitle: boolean, titleHeight = peerInfo.titleLine) {
  return (
    peerInfo.avatarTop +
    peerInfo.avatar +
    peerInfo.titleGap +
    (hasSubtitle ? 0 : 11) +
    titleHeight / 2 -
    (16 + (hasSubtitle ? 0 : 8))
  );
}
