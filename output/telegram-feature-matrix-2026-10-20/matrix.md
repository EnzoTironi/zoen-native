# Telegram feature reference matrix

20 October 2026. Inspiration catalog; no implementation selection or Telegram integration is implied.

The catalog covers user-visible behavior and platform integrations. Premium, Business, boosts and payments are separate gates. `Standard` means no subscription gate was stated in the cited feature description; it is not a live entitlement guarantee. Dated sources may describe a launch gate that later changed.

**Telegram iOS is the primary UI reference for implementation.** Android references are secondary. For the searchable view open [index.html](index.html). For full identifier registers and pinned frontend entrypoints use [matrix.json](matrix.json).

## Messaging and composition

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Cloud conversations | Synchronized conversations across authorized sessions. | Standard; cloud trust model | [Source](https://core.telegram.org/api/updates) |
| Text messages | Compose and send messages in private chats, groups and channels. | Standard; chat permissions | [Source](https://core.telegram.org/api) |
| Replies | Reply to a message and navigate back to its context. | Standard | [Source](https://telegram.org/blog/replies-mentions-hashtags#replies) |
| Selected-text quotes | Quote a specific fragment instead of the full message. | Standard | [Source](https://telegram.org/blog/reply-revolution#reply-revolution) |
| Reply in another chat | Carry a reply into a different destination. | Standard; destination permissions | [Source](https://telegram.org/blog/reply-revolution#reply-revolution) |
| Edit sent messages | Change sent text or captions within applicable rules. | Standard; time and role rules | [Source](https://telegram.org/blog/edit) |
| Replace sent media | Replace the media attached to an existing message. | Standard; message edit rules | [Source](https://telegram.org/blog/unread-replace-2x#replace-media-and-add-captions) |
| Add media after sending | Attach media when editing a sent message. | Standard; message edit rules | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#add-media-to-sent-messages) |
| Edit timestamps | Inspect when a message was last edited. | Standard | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#last-edit-timestamps) |
| Delete for both participants | Remove a private-chat message for both sides. | Standard; chat-specific rules | [Source](https://telegram.org/blog/unsend-privacy-emoji#unsend-anything) |
| Delete by date | Remove messages within a selected date range. | Standard; chat permissions | [Source](https://telegram.org/blog/protected-content-delete-by-date-and-more#delete-messages-by-date) |
| Undo chat deletion | Offer a brief opportunity to undo destructive chat actions. | Standard | [Source](https://telegram.org/blog/permissions-groups-undo#undo-clearing-history-and-deleting-chats) |
| Message forwarding | Forward messages with source attribution. | Standard; protected-content restrictions | [Source](https://telegram.org/blog/live-streams-forwarding-next-channel#flexible-forwarding) |
| Forward without sender or caption | Review forwarding options before sending. | Standard; protected-content restrictions | [Source](https://telegram.org/blog/live-streams-forwarding-next-channel#flexible-forwarding) |
| Forward to several destinations | Send selected messages to multiple chats. | Standard; recipient restrictions | [Source](https://telegram.org/blog/video-1000#more-for-ios) |
| Change forwarding destination | Switch the recipient during forwarding review. | Standard | [Source](https://telegram.org/blog/voice-chats-on-steroids#and-more) |
| Multi-message selection | Select several messages for shared actions. | Standard | [Source](https://telegram.org/blog/verifiable-apps-and-more#multi-sharing-from-other-apps) |
| Selective text copying | Copy part of a message rather than its entire body. | Standard; client-specific controls | [Source](https://telegram.org/blog/scheduled-reminders-themes#redesigned-message-options) |
| Cloud drafts | Resume a draft from another authorized session. | Standard; cloud chats | [Source](https://telegram.org/blog/drafts) |
| Scheduled messages | Choose a future delivery time. | Standard; destination rules | [Source](https://telegram.org/blog/scheduled-reminders-themes) |
| Send when online | Wait until the recipient is online before delivery. | Standard; visibility conditions | [Source](https://telegram.org/blog/verifiable-apps-and-more#send-when-online) |
| Silent delivery | Send without an audible notification. | Standard | [Source](https://telegram.org/blog/silent-messages-slow-mode) |
| Silent scheduled delivery | Combine scheduling with silent delivery. | Standard | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#silent-scheduled-messages) |
| Repeating scheduled messages | Repeat a scheduled message at an interval. | No subscription gate stated in cited announcement | [Source](https://telegram.org/blog/live-stories-gift-auctions#repeated-messages) |
| Saved-message reminders | Schedule reminders in your personal saved chat. | Standard | [Source](https://telegram.org/blog/scheduled-reminders-themes) |
| Presence and activity | Show online, typing, recording and related transient states. | Standard; privacy settings | [Source](https://core.telegram.org/api/updates) |
| Read state and message checks | Distinguish sent and read states in the conversation. | Standard; chat-type differences | [Source](https://core.telegram.org/api) |
| Read timestamps | Inspect read time where supported and permitted. | Chat type, privacy and retention rules | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#read-time-in-private-chats) |
| Small-group read receipts | See which participants read a message. | Group-size and retention rules | [Source](https://telegram.org/blog/chat-themes-interactive-emoji-read-receipts#read-receipts-in-small-groups) |
| Topic read time | Inspect reading information within topics. | Topic and retention rules | [Source](https://telegram.org/blog/shareable-folders-custom-wallpapers#read-time-in-topics) |
| Message links | Link directly to messages where the audience has access. | Standard; public/private access | [Source](https://telegram.org/blog/archive-and-new-design#easier-forwarding-links-to-messages-online-badges) |
| Phone-number actions | Open contextual actions for a number in a message. | Standard | [Source](https://telegram.org/blog/message-effects-and-more#quick-actions-for-phone-numbers) |
| Date and time entities | Render times in the reader's local timezone. | Standard | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#time-and-date-formatting) |
| Interactive buttons inside messages | Add embedded actions to rich messages. | Developer integration; client support | [Source](https://telegram.org/blog/welcome-messages-buttons-TG-13#buttons-inside-messages) |
| Inline document presentation | Present supported documents directly inside a message. | Client and document support | [Source](https://telegram.org/blog/welcome-messages-buttons-TG-13#inline-documents-in-messages) |
| App update placeholders | Show a temporary placeholder while an embedded app updates. | App and client support | [Source](https://telegram.org/blog/welcome-messages-buttons-TG-13#placeholders-for-app-updates) |

## Formatting and rich text

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Message formatting | Apply bold, italic, underline, strikeout and links. | Standard | [Source](https://core.telegram.org/api/entities) |
| Spoiler text | Conceal text until the reader reveals it. | Standard | [Source](https://telegram.org/blog/reactions-spoilers-translations#spoilers) |
| Code blocks and highlighting | Display source code with language-aware presentation. | Standard; client support | [Source](https://telegram.org/blog/giveaways#more-boosts-for-users) |
| Block quotes | Format quoted paragraphs inside a message. | Standard | [Source](https://telegram.org/blog/reply-revolution#quote-formatting) |
| Collapsible quotes | Collapse long quotations to reduce message height. | Standard | [Source](https://telegram.org/blog/my-profile-and-15-more#share-location-indefinitely) |
| Link preview controls | Adjust preview placement, size and which link is previewed. | Standard | [Source](https://telegram.org/blog/reply-revolution#adjustable-link-previews) |
| Visual rich-text editor | Compose longer structured and styled content. | Premium feature catalog; client rollout | [Source](https://telegram.org/blog/communities-editor-invisible-messages#rich-text-editor) |
| Rich bot messages | Render more structured output from bots. | Bot and client support | [Source](https://telegram.org/blog/watch-apps-and-more#obscenely-rich-text-formatting-for-bots) |
| Markdown document viewing | Read supported Markdown documents in the client. | Client support | [Source](https://telegram.org/blog/watch-apps-and-more#support-for-md-files) |

## Reactions, stickers and emoji

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Message reactions | React from a compact picker or quick gesture. | Standard; chat reaction policy | [Source](https://telegram.org/blog/reactions-spoilers-translations#reactions) |
| Reaction participants | Inspect reaction counts and participating users. | Chat privacy and type rules | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#exact-reaction-counters) |
| Reaction moderation | Admins remove reactions from selected users. | Moderator permission | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#removing-reactions) |
| Service-message reactions | React to supported service events. | Standard; supported event types | [Source](https://telegram.org/blog/collectible-gifts-and-more#reactions-for-service-messages) |
| Premium reaction choices | Use additional reaction options. | Premium sender; free participation rules | [Source](https://telegram.org/blog/700-million-and-premium#unique-reactions) |
| Multiple reactions | Add more than one reaction within the account limit. | Premium/configuration | [Source](https://telegram.org/blog/infinite-reactions-statuses#infinite-reactions) |
| Interactive emoji | Trigger synchronized effects from supported emoji. | Standard; supported emoji | [Source](https://telegram.org/blog/chat-themes-interactive-emoji-read-receipts#interactive-emoji) |
| Custom emoji | Insert user-created emoji into messages. | Premium creation/use rules; receiving differs | [Source](https://telegram.org/blog/custom-emoji#custom-animated-emoji) |
| Emoji categories and search | Find emoji by category or keyword. | Standard | [Source](https://telegram.org/blog/profile-pics-emoji-translations#emoji-categories) |
| AI sticker and emoji search | Search large public sticker and emoji catalogs semantically. | Service and client availability | [Source](https://telegram.org/blog/affiliate-programs-ai-sticker-search#ai-powered-sticker-search) |
| Sticker packs | Install and share reusable sticker sets. | Standard | [Source](https://telegram.org/blog/stickers-revolution) |
| Animated stickers | Send lightweight animated stickers. | Standard | [Source](https://telegram.org/blog/animated-stickers) |
| Video stickers | Send supported video-based stickers. | Standard | [Source](https://telegram.org/blog/video-stickers-better-reactions#video-stickers) |
| Premium stickers | Send stickers with additional effects. | Premium sender; recipients can view | [Source](https://telegram.org/blog/700-million-and-premium#unique-stickers) |
| Favorite stickers | Keep frequently used stickers in a favorites list. | Standard; expanded Premium limit | [Source](https://telegram.org/blog/replies-mentions-stickers#favorite-stickers) |
| Recent and trending stickers | Browse recently used and promoted sticker sets. | Standard | [Source](https://telegram.org/blog/masks#trending-stickers) |
| Sticker pack ordering | Arrange packs and optionally reorder them dynamically. | Standard | [Source](https://telegram.org/blog/power-saving#dynamic-order-for-sticker-packs) |
| Sticker previews | Preview stickers before selecting or sending them. | Standard; client support | [Source](https://telegram.org/blog/share-preview#preview-sticker-packs-and-stickers) |
| Sticker editor | Create stickers using cutouts, text and drawings. | Standard | [Source](https://telegram.org/blog/sticker-maker#make-a-sticker) |
| Photo-to-sticker conversion | Turn a viewed photo into an editable sticker. | Standard | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#instant-stickers-from-photos) |
| Mixed sticker packs | Combine supported sticker formats in one pack. | Standard | [Source](https://telegram.org/blog/sticker-maker#expand-your-set) |
| GIF search and saved GIFs | Find GIFs and keep reusable favorites. | Standard; expanded Premium limit | [Source](https://telegram.org/blog/android-gif) |
| GIF captions | Send GIFs with captions above or below them. | Standard | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#captions-for-gifs) |
| GIF and sticker editing | Edit drawings, text or duration before sending. | Standard; format-dependent tools | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#editing-gifs-and-stickers) |
| Message effects | Attach an animated effect to a sent message. | Some standard; additional Premium effects | [Source](https://telegram.org/blog/message-effects-and-more#message-effects) |
| Choosing-sticker activity | Show a transient sticker-selection status. | Standard | [Source](https://telegram.org/blog/live-streams-forwarding-next-channel#choosing-a-sticker) |

## Photos, video and attachments

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Attachment picker | Select media, files and other attachment types. | Standard; device permissions | [Source](https://telegram.org/blog/downloads-attachments-streaming#new-attachment-menu) |
| Attachment preview and ordering | Review selected items and arrange their send order. | Standard | [Source](https://telegram.org/blog/albums-saved-messages#photo-order) |
| Media albums | Present grouped photos and videos as an album. | Standard; album constraints | [Source](https://telegram.org/blog/albums-saved-messages) |
| File albums | Group related document attachments. | Standard | [Source](https://telegram.org/blog/pinned-messages-locations-playlists#playlists) |
| Captions | Add formatted text to supported attachments. | Standard; expanded Premium length | [Source](https://telegram.org/blog/captions-places#photo-captions) |
| Captions above media | Choose caption placement around media. | Standard | [Source](https://telegram.org/blog/message-effects-and-more#captions-above-media) |
| Original-file sending | Send a file instead of a compressed media rendition. | Standard; transfer limits | [Source](https://core.telegram.org/api/files) |
| Large uploads | Transfer large files with account-dependent limits. | Standard/Premium; configurable size | [Source](https://telegram.org/blog/700-million-and-premium#4-gb-uploads) |
| Download manager | Track, pause and prioritize downloads. | Standard | [Source](https://telegram.org/blog/downloads-attachments-streaming#download-manager) |
| Download priorities | Bring selected transfers ahead of other downloads. | Standard | [Source](https://telegram.org/blog/infinite-reactions-statuses#prioritize-downloads-on-android) |
| Faster downloads | Remove Telegram's Premium-specific download throttling. | Premium; network/device still constrain speed | [Source](https://telegram.org/blog/700-million-and-premium#faster-downloads) |
| HD photos | Choose a higher-quality photo sending mode. | Standard; client support | [Source](https://telegram.org/blog/direct-to-channel-trim-voice-and-more#send-photos-in-higher-quality) |
| Photo editor | Adjust photos before sending. | Standard | [Source](https://telegram.org/blog/photo-editor-and-passcodes#photo-editor) |
| Crop and rotation | Reframe images using native media controls. | Standard; media type | [Source](https://telegram.org/blog/video-1000#precision-drawing) |
| Markup and precision drawing | Annotate media with drawing tools. | Standard | [Source](https://telegram.org/blog/video-1000#precision-drawing) |
| Photo cutouts | Extract subjects for media composition. | Standard; supported client | [Source](https://telegram.org/blog/posts-in-stories-and-more#photo-cutouts-for-stories) |
| Video trimming | Select the video segment to send. | Standard | [Source](https://telegram.org/blog/files-on-steroids#share-multiple-files-and-trim-videos) |
| Compression control | Choose the tradeoff between transfer size and quality. | Standard; supported media | [Source](https://telegram.org/blog/calls#one-more-thing) |
| Video-to-GIF | Send a silent looping rendition of a video. | Standard | [Source](https://telegram.org/blog/masks#gifs-democratized) |
| Media spoiler | Conceal photo or video previews until revealed. | Standard | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#hidden-media) |
| View-once media | Open supported media for a single viewing. | Standard; destination restrictions | [Source](https://telegram.org/blog/channel-stories#view-once-media) |
| Disappearing media | Apply an expiration policy to supported media. | Standard; destination restrictions | [Source](https://telegram.org/blog/now-you-see-me) |
| Live and Motion Photos | Send and play supported animated photo formats. | Supported devices and clients | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#live-and-motion-photos) |
| Document scanner | Capture paper pages as a scanned document. | iOS-specific published feature | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#document-scanner-on-ios) |
| Camera capture | Capture photos or video from the attachment flow. | Standard; camera permission | [Source](https://telegram.org/blog/instant-camera) |
| Video-message flash | Illuminate capture when recording video messages. | Supported camera/client | [Source](https://telegram.org/blog/w3-browser-mini-app-store#flash-when-recording-video-messages) |
| Document viewer | Read supported documents within Telegram. | Format and client support | [Source](https://telegram.org/blog/superchannels-star-reactions-subscriptions#document-viewer-on-ios) |
| Gallery viewer | Browse shared photos and videos with gestures. | Standard; client-specific gestures | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#hyper-speed-scrolling-for-shared-media) |
| Shared-media calendar | Navigate media by date. | Standard | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#calendar-view-for-shared-media) |
| Media type filters | Filter shared photos, video, documents and links. | Standard | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#calendar-view-for-shared-media) |
| Video streaming | Start playback before the complete download finishes. | Supported formats and network | [Source](https://telegram.org/blog/android-streaming) |
| Adaptive video quality | Select or adapt the playback rendition. | Supported channels and encodings | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#faster-video-loading-better-quality) |
| Video covers | Choose a thumbnail for a published video. | Supported channel publishing | [Source](https://telegram.org/blog/AI-sticker-search-video-improvements#video-covers-in-channels) |
| Playback progress restoration | Resume a previously watched video. | Supported client/media | [Source](https://telegram.org/blog/AI-sticker-search-video-improvements#saved-progress-for-videos) |
| Timestamp sharing | Share a link starting at a specific playback time. | Access and media support | [Source](https://telegram.org/blog/AI-sticker-search-video-improvements#copy-video-link-at-current-time) |
| Picture in picture | Continue watching in a floating player. | OS and client support | [Source](https://telegram.org/blog/drafts#picture-in-picture) |
| Playback speed | Adjust audio or video playback speed. | Standard; supported media | [Source](https://telegram.org/blog/power-saving#granular-playback-speed) |
| Chromecast output | Play supported video on a casting device. | Android-specific published feature | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#stream-to-chromecast-on-android) |

## Voice, audio and video messages

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Voice recording | Record a voice message from the composer. | Standard; microphone permission | [Source](https://telegram.org/blog/voice-2-secret-3) |
| Hands-free recording | Lock recording without holding the button. | Standard; client support | [Source](https://telegram.org/blog/video-1000#video-messages-2-0) |
| Pause and resume recording | Pause a recording before completing it. | Standard | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#pause-and-resume-recording) |
| Voice review | Listen to a recording before sending it. | Standard; client support | [Source](https://telegram.org/blog/direct-to-channel-trim-voice-and-more#trim-voice-messages) |
| Voice trimming | Remove unwanted recording edges before sending. | Standard | [Source](https://telegram.org/blog/direct-to-channel-trim-voice-and-more#trim-voice-messages) |
| Waveform scrubbing | Seek through a voice message visually. | Standard; client support | [Source](https://telegram.org/blog/move-history#improved-audio-player) |
| Voice playback speed | Listen to recordings at a chosen speed. | Standard | [Source](https://telegram.org/blog/unread-replace-2x#double-time-playback-for-voice-and-video-messages) |
| Raise to listen | Switch voice playback behavior using proximity. | Standard; device support | [Source](https://telegram.org/blog/voice-2-secret-3) |
| Round video messages | Record and send short circular video messages. | Standard; camera/microphone permission | [Source](https://telegram.org/blog/video-messages-and-telescope) |
| One-time voice and video notes | Send notes that disappear after one playback. | Standard; supported destinations | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#one-time-voice-and-video-messages) |
| Voice transcription | Read text generated from a voice recording. | Premium; limited free/boost variants | [Source](https://telegram.org/blog/similar-channels/#voice-to-text-for-everyone) |
| Video-note transcription | Transcribe speech from round video messages. | Premium; service availability | [Source](https://telegram.org/blog/topics-in-groups-collectible-usernames#voice-to-text-for-video-messages) |
| Music playlists | Play related audio files as a queue. | Standard | [Source](https://telegram.org/blog/pinned-messages-locations-playlists#playlists) |
| Podcast and audiobook controls | Resume and navigate long audio recordings. | Standard; client support | [Source](https://telegram.org/blog/verifiable-apps-and-more#podcast-and-audiobook-support) |
| Search audio by artist | Find an audio file through artist metadata. | Standard; indexed metadata | [Source](https://telegram.org/blog/move-history#improved-audio-player) |

## Calls, video chats and live streams

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Private voice calls | Place an end-to-end encrypted audio call. | Standard; calling privacy rules | [Source](https://telegram.org/blog/calls#secure) |
| Private video calls | Place an end-to-end encrypted video call. | Standard; camera permission | [Source](https://telegram.org/blog/video-calls) |
| Secure ad-hoc group calls | Start a group call without creating a group chat. | Standard; current participant limits | [Source](https://telegram.org/blog/group-calls-made-easy#secure-group-calls-made-easy) |
| Group voice chats | Run a voice session attached to a group or channel. | Host/participant permissions | [Source](https://telegram.org/blog/voice-chats) |
| Group video chats | Add video to a hosted group session. | Host/participant permissions | [Source](https://telegram.org/blog/group-video-calls#group-video-calls) |
| Screen sharing | Share a screen during supported calls. | OS permission; supported clients | [Source](https://telegram.org/blog/group-video-calls#screen-sharing) |
| Screen sharing with audio | Include supported system sound with screen sharing. | OS and client support | [Source](https://telegram.org/blog/video-1000#screen-sharing-with-sound) |
| Selective screen sharing | Choose a window or surface to share. | Desktop/client support | [Source](https://telegram.org/blog/group-video-calls#tablet-and-desktop-support) |
| Noise suppression | Reduce background noise during voice sessions. | Supported client/device | [Source](https://telegram.org/blog/group-video-calls#noise-suppression-options) |
| Per-participant volume | Adjust a participant's playback volume. | Supported group-call client | [Source](https://telegram.org/blog/move-history#improved-voice-chats) |
| Raise hand | Request permission to speak in a hosted session. | Listener/host roles | [Source](https://telegram.org/blog/voice-chats-on-steroids#raise-hand) |
| Speaker and listener links | Invite participants with distinct speaking roles. | Host permission | [Source](https://telegram.org/blog/voice-chats-on-steroids#speaker-and-listener-links) |
| Scheduled voice sessions | Announce a voice session before it begins. | Host permission | [Source](https://telegram.org/blog/payments-2-0-scheduled-voice-chats#scheduled-voice-chats) |
| Call recording | Record supported hosted sessions. | Host permission; recording disclosure | [Source](https://telegram.org/blog/chat-themes-interactive-emoji-read-receipts#record-live-streams-and-video-chats) |
| RTMP streaming | Broadcast through external production software. | Channel/group host; client support | [Source](https://telegram.org/blog/gifts-verification-platform/#rtmp-streams-on-android) |
| Call comments and reactions | Interact through comments and reactions during a session. | Supported session/client | [Source](https://telegram.org/blog/comments-in-video-chats-threads-for-bots#comments-and-reactions-in-group-calls) |
| Join as a channel | Participate using an eligible channel identity. | Channel administration permission | [Source](https://telegram.org/blog/voice-chats-on-steroids#join-as) |
| Group-call verification | Compare verification information for encrypted group calls. | Supported secure-call protocol | [Source](https://core.telegram.org/api/group-calls) |

## Inbox, folders and Saved Messages

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Chat folders | Organize chats into custom filtered tabs. | Standard; expanded Premium limits | [Source](https://telegram.org/blog/folders) |
| Default folder | Open a preferred folder when the app starts. | Premium | [Source](https://telegram.org/blog/700-million-and-premium#chat-management) |
| Folder tags | Show colored chat labels based on folder membership. | Premium/Business | [Source](https://telegram.org/blog/telegram-business#tags-for-chats) |
| Emoji folder names | Include custom emoji in folder labels. | Account/custom-emoji rules | [Source](https://telegram.org/blog/collectible-gifts-and-more#custom-emoji-in-folder-names) |
| Shared folders | Share a set of chats through an invitation link. | Standard; expanded Premium limits | [Source](https://telegram.org/blog/shareable-folders-custom-wallpapers#shareable-chat-folders) |
| Pinned chats | Keep chosen conversations at the top. | Standard; expanded Premium limit | [Source](https://telegram.org/blog/pin-and-ifttt) |
| Archived chats | Move less active conversations out of the main inbox. | Standard | [Source](https://telegram.org/blog/archive-and-new-design#everything-in-its-place) |
| Automatic archive controls | Keep new non-contact conversations out of the main inbox. | Account/privacy configuration | [Source](https://telegram.org/blog/profile-videos-people-nearby-and-more#filter-new-chats-from-non-contacts) |
| Mark read or unread | Change the inbox's local attention state. | Standard | [Source](https://telegram.org/blog/unread-replace-2x#mark-as-read-or-unread) |
| Archive bulk-read | Mark archived conversations as read together. | Standard | [Source](https://telegram.org/blog/verifiable-apps-and-more#mark-archive-as-read) |
| Chat list swipe actions | Configure available row gestures. | Client-specific | [Source](https://telegram.org/blog/voice-chats-on-steroids#and-more) |
| Chat previews | Inspect a conversation preview from the chat list. | Client-specific | [Source](https://telegram.org/blog/700-million-and-premium#improved-chat-previews-on-android) |
| Recent-chat navigation | Move quickly among recently opened conversations. | Client-specific | [Source](https://telegram.org/blog/video-stickers-better-reactions#navigating-recent-chats) |
| Unread and mention counters | Jump to unread replies, mentions or messages. | Standard | [Source](https://telegram.org/blog/replies-mentions-stickers) |
| Saved Messages | Keep messages and files in a personal chat. | Standard; cloud storage model | [Source](https://telegram.org/blog/albums-saved-messages#saved-messages) |
| Saved Messages by source | Browse saved items grouped by their source chat. | Standard/Premium features differ | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#view-saved-messages-by-chat) |
| Saved-message tags | Label saved items with reaction-based filters. | Premium | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#tags-for-saved-messages) |
| Saved-message shared-media entry | Find saved content from another chat's media view. | Standard; source access | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#saved-messages-in-shared-media) |
| Pinned saved dialogs | Pin source-specific saved conversations. | Account-dependent limit | [Source](https://core.telegram.org/api/saved-messages) |

## Search, discovery and reading

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Conversation search | Search within the current conversation. | Standard | [Source](https://telegram.org/blog/search-and-media) |
| Global message search | Search accessible conversations from one place. | Standard | [Source](https://telegram.org/blog/android-2-0#instant-full-text-search) |
| Search filters | Narrow results by content type, source or time. | Standard | [Source](https://telegram.org/blog/filters-anonymous-admins-comments#search-filters) |
| Author and chat filters | Refine message searches with supported fields. | Standard; supported query fields | [Source](https://telegram.org/blog/collectible-gifts-and-more#message-search-filters) |
| Jump to date | Navigate to a selected day in a conversation. | Standard | [Source](https://telegram.org/blog/instant-view#jump-to-date) |
| Search result list | Inspect matching messages as a result list. | Standard; client presentation | [Source](https://telegram.org/blog/verifiable-apps-and-more#view-search-results-as-a-list) |
| Public post search | Discover matching posts from public channels. | Initially Premium; current gate configurable | [Source](https://telegram.org/blog/post-search-story-albums-and-more#public-post-search) |
| Global hashtag search | Find public content sharing a hashtag. | Scope and account restrictions | [Source](https://telegram.org/blog/message-effects-and-more#hashtag-search) |
| Chat-scoped hashtags | Scope a hashtag query to a particular chat. | Standard | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#chat-specific-hashtags) |
| Channel recommendations | Discover related public channels. | Standard; expanded Premium results | [Source](https://telegram.org/blog/similar-channels#similar-channels) |
| Bot recommendations | Discover similar bots. | Service availability | [Source](https://telegram.org/blog/AI-sticker-search-video-improvements#similar-bots) |
| In-app browser | Open web content without leaving Telegram. | Client support | [Source](https://telegram.org/blog/w3-browser-mini-app-store/pt-br?setln=en#telegram-browser) |
| Browser choice | Select a preferred external browser. | Client/OS support | [Source](https://telegram.org/blog/watch-apps-and-more#quick-browser-selection) |
| Instant View | Read supported web articles in a native reading layout. | Site/template support | [Source](https://telegram.org/blog/instant-view) |
| Instant View tables and RTL | Read richer article structures and right-to-left layouts. | Site/template support | [Source](https://telegram.org/blog/translations-iv2#instant-view-2-0) |
| Search settings | Find application settings and help entries. | Client support | [Source](https://telegram.org/blog/unsend-privacy-emoji#settings-search) |
| QR scanner | Open supported links from a QR code. | Camera permission; client support | [Source](https://telegram.org/blog/collectible-gifts-and-more#qr-code-scanner) |

## Groups, topics and communities

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Groups and supergroups | Run many-person conversations with administration. | Standard; configured size limits | [Source](https://core.telegram.org/api/channel) |
| Public and private groups | Choose discoverability and invitation access. | Standard; public-link limits | [Source](https://telegram.org/blog/supergroups5k) |
| Forum topics | Divide a group into named discussion topics. | Group configuration and role | [Source](https://telegram.org/blog/topics-in-groups-collectible-usernames#topics-in-groups) |
| Custom topic icons | Set a custom emoji as a forum topic icon. | Premium; topic-management permission | [Source](https://core.telegram.org/api/premium#forum-topic-icon) |
| Topic tabs | Navigate topics through a compact tab interface. | Client/group support | [Source](https://telegram.org/blog/direct-to-channel-trim-voice-and-more#topic-tabs) |
| Topic presentation controls | Switch supported topic layouts and manage the general topic. | Forum permissions | [Source](https://core.telegram.org/api/forum) |
| Communities | Link related groups, channels and bots in one community. | Community roles and configuration | [Source](https://telegram.org/blog/communities-editor-invisible-messages#telegram-communities) |
| Community navigation | Move between related chats from a shared directory. | Membership and community roles | [Source](https://telegram.org/blog/communities-editor-invisible-messages#telegram-communities) |
| Invite links | Invite participants through shareable links. | Invitation permission | [Source](https://telegram.org/blog/invite-links#invite-links) |
| Expiring invitation links | Limit the lifetime or use of an invite. | Administrator permission | [Source](https://telegram.org/blog/autodelete-inv2#expiring-invite-links) |
| Named invite links | Identify distinct invite campaigns. | Administrator permission | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#unique-names-for-invite-links) |
| Join requests | Review membership requests before admission. | Administrator approval | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#join-requests-for-groups-and-channels) |
| Public-group join approval | Require approval for joining a public group. | Administrator configuration | [Source](https://telegram.org/blog/700-million-and-premium#join-requests-for-public-groups) |
| Welcome messages | Deliver configured content to newly joining members. | Chat owner/admin; current configuration | [Source](https://telegram.org/blog/welcome-messages-buttons-TG-13#welcome-messages) |
| Member tags | Show a participant's role or self-description. | Group permission; admin distinction | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#member-tags) |
| Admin titles and badges | Identify moderators and their roles. | Administrator permission | [Source](https://telegram.org/blog/silent-messages-slow-mode#admin-titles) |
| Granular admin rights | Delegate selected administrative powers. | Owner/admin role | [Source](https://telegram.org/blog/admin-revolution#admin-rights) |
| Per-member restrictions | Restrict particular participant capabilities. | Moderator permission | [Source](https://telegram.org/blog/admin-revolution#partial-bans) |
| Granular media permissions | Control which media types members may send. | Administrator permission | [Source](https://telegram.org/blog/profile-pics-emoji-translations#granular-media-permissions) |
| Slow mode | Require a delay between member messages. | Administrator configuration | [Source](https://telegram.org/blog/silent-messages-slow-mode#slow-mode) |
| Anti-spam moderation | Filter suspicious activity in eligible groups. | Group eligibility and admin configuration | [Source](https://telegram.org/blog/ultimate-privacy-topics-2-0#aggressive-anti-spam) |
| Guardian bots | Screen join requests with an automation or AI flow. | Admin setup; authorized bot | [Source](https://telegram.org/blog/watch-apps-and-more#guardian-bots-for-group-chats) |
| Hidden member list | Restrict who can inspect group members. | Eligible group/admin settings | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#hiding-group-members) |
| Anonymous administrators | Post without exposing the admin's personal account. | Owner/admin setting | [Source](https://telegram.org/blog/filters-anonymous-admins-comments#anonymous-group-admins) |
| Posting as a channel | Post in a group using an authorized channel identity. | Channel rights and destination rules | [Source](https://telegram.org/blog/protected-content-delete-by-date-and-more#anonymous-posting-in-public-groups) |
| Moderation audit log | Review recent administrative actions. | Administrator permission | [Source](https://telegram.org/blog/admin-revolution#recent-admin-actions) |
| Bulk moderation | Apply selected moderation actions to several users. | Moderator permission | [Source](https://telegram.org/blog/my-profile-and-15-more#mass-moderation-for-groups) |
| Ownership transfer | Transfer a group's ownership to another eligible user. | Owner; security requirements | [Source](https://telegram.org/blog/contacts-local-groups#transfer-group-chats) |
| Ownership transfer on departure | Choose a successor while leaving a group. | Owner; eligibility conditions | [Source](https://telegram.org/blog/crafting-android-design-and-more#leaving-groups-to-a-new-admin) |
| Group statistics | Inspect group activity and audience analytics. | Eligibility and admin rights | [Source](https://telegram.org/blog/profile-videos-people-nearby-and-more#group-stats) |
| Group sticker and emoji sets | Configure reusable community expression packs. | Administrator; emoji boosts may apply | [Source](https://telegram.org/blog/february2024#group-emoji-packs) |
| Group appearance | Customize supported group identity and wallpaper. | Role and boost-level gates | [Source](https://telegram.org/blog/february2024#emoji-status-covers-wallpapers-and-link-styles) |

## Channels and publishing

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Broadcast channels | Publish posts to subscribers. | Standard; administrative rights | [Source](https://telegram.org/blog/channels) |
| Channel discussion groups | Attach a group for comments and discussion. | Channel/group admin rights | [Source](https://telegram.org/blog/privacy-discussions-web-bots#broadcasts-meet-group-chats) |
| Channel comments | Reply to channel posts in linked discussion. | Access and comment policy | [Source](https://telegram.org/blog/filters-anonymous-admins-comments#channel-comments) |
| Unread comment counters | Track new discussion activity on a post. | Discussion/client support | [Source](https://telegram.org/blog/live-streams-forwarding-next-channel#unread-comment-counters) |
| Channel direct messages | Receive messages addressed to the channel. | Channel configuration; optional price | [Source](https://telegram.org/blog/direct-to-channel-trim-voice-and-more#send-direct-messages-to-channels) |
| Suggested posts | Submit content for a channel's review. | Channel direct messages and permissions | [Source](https://telegram.org/blog/checklists-suggested-posts#suggest-posts-in-channels) |
| Post negotiation and scheduling | Revise suggested content, price and publishing time. | Mutual approval; payment rules | [Source](https://telegram.org/blog/checklists-suggested-posts#suggest-posts-in-channels) |
| Channel signatures | Attribute channel posts to administrators. | Channel admin configuration | [Source](https://telegram.org/blog/channels-2-0#admin-signatures) |
| Author profiles in channels | Publish with more visible author identity. | Channel admin configuration | [Source](https://telegram.org/blog/superchannels-star-reactions-subscriptions#super-channels) |
| Pinned channel posts | Keep selected posts prominent. | Channel admin permission | [Source](https://telegram.org/blog/albums-saved-messages#pinned-messages-in-channels) |
| Post statistics | Review views, forwards and audience performance. | Channel eligibility/admin rights | [Source](https://telegram.org/blog/pinned-messages-locations-playlists#channel-post-stats) |
| Channel statistics | Inspect audience and distribution metrics. | Channel eligibility/admin rights | [Source](https://telegram.org/blog/folders#channel-stats) |
| Jump to next channel | Continue through unread channel posts. | Client support | [Source](https://telegram.org/blog/live-streams-forwarding-next-channel#jump-to-next-channel) |
| Channel content protection | Restrict forwarding and supported saving/capture actions. | Channel admin configuration | [Source](https://telegram.org/blog/protected-content-delete-by-date-and-more#protected-content-in-groups-and-channels) |
| Default channel translation | Choose automatic translation for eligible channel readers. | Channel/client/service gates | [Source](https://telegram.org/blog/gift-marketplace-and-more#set-your-channel-to-auto-translate) |

## Polls and checklists

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Anonymous polls | Collect votes without exposing voter identity. | Poll creation permission | [Source](https://telegram.org/blog/polls) |
| Public-vote polls | Display who selected an answer. | Group/poll policy | [Source](https://telegram.org/blog/polls-2-0-vmq#visible-votes) |
| Multiple-answer polls | Permit more than one selected option. | Poll configuration | [Source](https://telegram.org/blog/polls-2-0-vmq#multiple-answers) |
| Quiz polls | Mark correct choices and show explanations. | Poll configuration | [Source](https://telegram.org/blog/polls-2-0-vmq#quiz-mode) |
| Poll media and locations | Add supported media or location to questions and answers. | Poll/client support | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Poll descriptions | Add context below a poll's question. | Poll/client support | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Suggested poll options | Let participants propose additional choices. | Creator configuration | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Revoting controls | Decide whether participants may change a vote. | Creator configuration | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Shuffled poll choices | Randomize the order of displayed answers. | Creator configuration | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Poll deadlines | Close a poll automatically at a chosen time. | Creator configuration | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Hidden poll results | Withhold the tally until the configured closing condition. | Creator configuration | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Poll voter timestamps | Inspect when public votes were submitted. | Visible-vote polls | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#timestamps-for-polls) |
| Poll statistics | Track vote counts over time. | Admin/creator; minimum activity gate | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#statistics-for-polls) |
| Poll audience limits | Restrict voting by subscription or supported geography rules. | Channel admin configuration | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#custom-limits-for-polls) |
| Links in poll answers | Attach supported links to poll options. | Client/poll support | [Source](https://telegram.org/blog/watch-apps-and-more#links-in-poll-options) |
| Poll history tab | Find active and past polls from a chat profile. | Group/channel/client support | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#10-new-features-for-polls) |
| Collaborative checklists | Share an editable list of tasks within a chat. | Premium creation; participation permissions | [Source](https://telegram.org/blog/checklists-suggested-posts#checklists) |
| Checklist item permissions | Control who may add tasks or mark them complete. | Creator configuration | [Source](https://telegram.org/blog/checklists-suggested-posts#checklists) |

## Stories

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Story publishing | Share time-limited photo or video posts. | Account quotas and configuration | [Source](https://telegram.org/blog/stories#telegram-stories) |
| Story audience controls | Choose who can view a story. | Standard; story privacy rules | [Source](https://telegram.org/blog/stories#telegram-stories) |
| Stories on profiles | Keep selected stories visible on a profile. | Account/profile configuration | [Source](https://telegram.org/blog/stories#stories-in-profiles) |
| Story albums | Organize profile stories into named collections. | Account/channel publishing rules | [Source](https://telegram.org/blog/post-search-story-albums-and-more#story-albums) |
| Story editing | Change supported story content after publishing. | Story author | [Source](https://telegram.org/blog/stories#editing-stories) |
| Dual-camera stories | Capture using supported front and rear cameras. | Device/client support | [Source](https://telegram.org/blog/stories#dual-camera-mode) |
| Story collage | Combine multiple images in a story. | Client support | [Source](https://telegram.org/blog/affiliate-programs-ai-sticker-search#collages-in-stories) |
| Story music | Add audio to a story. | Source and client availability | [Source](https://telegram.org/blog/channel-stories#your-music-in-stories) |
| Saved profile audio in stories | Reuse saved profile music in a story. | Account/media availability | [Source](https://telegram.org/blog/passkeys-and-gift-offers#audio-in-stories) |
| Story captions and links | Add styled captions and supported link areas. | Premium/configuration for extended tools | [Source](https://telegram.org/blog/mini-app-bar-paid-media-and-more#link-widget-in-stories) |
| Story location and hashtag discovery | Discover public stories by associated place or tag. | Public stories and service rules | [Source](https://telegram.org/blog/mini-app-bar-paid-media-and-more#search-stories-by-location) |
| Story reactions | React to a story or use reaction stickers. | Story policy; Premium limits differ | [Source](https://telegram.org/blog/channel-stories#reaction-stickers-in-stories) |
| Story view statistics | Inspect interactions and viewers within applicable windows. | Author/role; Premium retention differs | [Source](https://telegram.org/blog/stories#detailed-view-statistics) |
| Story reposting | Repost eligible stories with additional presentation. | Source privacy and sharing policy | [Source](https://telegram.org/blog/similar-channels#reposting-stories) |
| Channel posts in stories | Share a channel post inside a story. | Post access and protection rules | [Source](https://telegram.org/blog/posts-in-stories-and-more#share-posts-in-stories) |
| Video-note overlays | Add a video-message layer to a story. | Client support | [Source](https://telegram.org/blog/similar-channels#video-messages-on-stories) |
| Story covers | Choose a preview image for a story. | Author; client support | [Source](https://telegram.org/blog/w3-browser-mini-app-store#story-covers) |
| Weather widget | Insert weather information into a story. | Location/client/service support | [Source](https://telegram.org/blog/w3-browser-mini-app-store/pt-br?setln=en#weather-widget-in-stories) |
| Several stories at once | Upload several selected stories together. | Account quotas | [Source](https://telegram.org/blog/gift-marketplace-and-more#post-several-stories-at-once) |
| Split long story video | Divide a longer video into sequential stories. | Account quotas; client support | [Source](https://telegram.org/blog/gift-marketplace-and-more#split-long-videos-into-several-stories) |
| Live stories | Host a live broadcast through a story surface. | Eligibility and live-stream rules | [Source](https://telegram.org/blog/live-stories-gift-auctions#live-stories) |
| Channel and group stories | Publish stories using a chat identity. | Admin rights and boost levels | [Source](https://telegram.org/blog/february2024#stories-from-groups) |
| Premium story priority | Give eligible stories priority in the viewer list. | Premium | [Source](https://core.telegram.org/api/premium#stories) |
| Premium stealth viewing | Hide eligible viewing activity through the story feature. | Premium; timing rules | [Source](https://telegram.org/blog/my-profile-and-15-more#open-stories-anonymously) |
| Longer viewer-history access | Retain access to eligible expired-story viewer lists. | Premium; privacy rules | [Source](https://core.telegram.org/api/premium#stories) |
| Custom story expiry | Choose among additional story lifetime options. | Premium | [Source](https://core.telegram.org/api/premium#stories) |
| Save eligible stories | Save stories when the publisher permits saving. | Premium; content protection | [Source](https://core.telegram.org/api/premium#stories) |
| Higher-quality stories | Access eligible higher-quality story media. | Premium | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#faster-loading-time-and-high-quality-stories) |

## Profiles and contacts

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Names, usernames and biography | Manage a public-facing account identity. | Standard; username availability | [Source](https://telegram.org/blog/now-you-see-me#who-are-you-again) |
| Profile photos | Set and inspect account avatars. | Standard | [Source](https://telegram.org/blog/new-profiles-people-nearby#redesigned-profile-pages) |
| Profile videos | Use a short video as profile media. | Standard; Premium animation surfaces differ | [Source](https://telegram.org/blog/profile-videos-people-nearby-and-more#profile-videos) |
| Animated avatars across surfaces | Animate eligible avatars in more application surfaces. | Premium | [Source](https://telegram.org/blog/700-million-and-premium#animated-profile-pictures) |
| Personal contact photos | Assign a local-facing profile image for a contact. | Standard | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#profile-pictures-for-your-contacts) |
| Suggested contact photos | Suggest a replacement avatar to another user. | Standard; recipient approval | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#profile-pictures-for-your-contacts) |
| Public fallback avatar | Show a separate avatar to users outside the private audience. | Privacy configuration | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#public-profile-pictures) |
| Avatar creator | Compose a profile picture using supported visual elements. | Standard; asset availability | [Source](https://telegram.org/blog/profile-pics-emoji-translations#profile-picture-maker) |
| Contact discovery | Find existing Telegram contacts with privacy-aware matching. | Consent and privacy settings | [Source](https://core.telegram.org/api/contacts) |
| Add contacts without a phone number | Save an identifiable person without their number. | Standard; profile visibility | [Source](https://telegram.org/blog/contacts-local-groups) |
| Contact cards | Share selected contact fields. | Sender choice and recipient access | [Source](https://telegram.org/blog/unread-replace-2x#share-detailed-contact-info) |
| Contact notes | Keep private notes about a contact. | Standard; account storage | [Source](https://telegram.org/blog/comments-in-video-chats-threads-for-bots#notes-for-contacts) |
| Birthdays | Display a birthday and its configured audience. | Profile privacy settings | [Source](https://telegram.org/blog/my-profile-and-15-more#birthdays) |
| Suggested birthdays | Propose birthday information to a contact. | Recipient approval | [Source](https://telegram.org/blog/comments-in-video-chats-threads-for-bots#suggested-birthdays) |
| Profile music | Save audio tracks on a profile. | Account/media availability | [Source](https://telegram.org/blog/profile-music-gift-themes#music-on-your-profile) |
| Default profile tab | Choose the initial profile section. | Client/profile support | [Source](https://telegram.org/blog/profile-music-gift-themes#default-profile-tab) |
| Channel on profile | Feature an owned channel on a personal profile. | Channel ownership/eligibility | [Source](https://telegram.org/blog/my-profile-and-15-more#channels-on-your-profile) |
| Premium badge | Display subscription identity beside a name. | Premium | [Source](https://telegram.org/blog/700-million-and-premium#premium-badges) |
| Emoji status | Display an emoji status with optional duration. | Premium/eligible collectible rules | [Source](https://telegram.org/blog/infinite-reactions-statuses#emoji-statuses) |
| Profile and name colors | Customize supported identity colors. | Premium; chat boosts differ | [Source](https://telegram.org/blog/similar-channels#profile-colors) |
| Verification and impersonation signals | Show supported verified, scam and fake indicators. | Platform/verification eligibility | [Source](https://telegram.org/blog/gifts-verification-platform#verification-platform) |
| Third-party verification | Display supported independent verification badges. | Authorized verifier | [Source](https://telegram.org/blog/collectible-gifts-and-more#verification-from-third-parties) |
| Contact confirmation | Present additional context about an unfamiliar sender. | Account/client support | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#know-whos-messaging-you-with-contact-confirmation) |
| Stars profile rating | Display a transaction-derived profile rating. | Eligible Stars activity | [Source](https://telegram.org/blog/post-search-story-albums-and-more#profile-rating) |
| Collectible usernames | Acquire and manage transferable usernames. | Paid marketplace and ownership rules | [Source](https://telegram.org/blog/topics-in-groups-collectible-usernames#collectible-usernames) |
| Anonymous-number accounts | Register using supported collectible numbers. | Paid third-party ecosystem; eligibility | [Source](https://telegram.org/blog/ultimate-privacy-topics-2-0#sign-up-without-a-sim-card) |

## Privacy, security and account lifecycle

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Phone-number visibility | Control who can see a phone number. | Standard; exceptions | [Source](https://telegram.org/blog/privacy-discussions-web-bots#who-can-see-my-phone-number) |
| Phone-number discoverability | Control who can find the account by number. | Standard; exceptions | [Source](https://telegram.org/blog/scheduled-reminders-themes#new-privacy-settings) |
| Last-seen audience | Control presence visibility with exceptions. | Standard; reciprocity rules | [Source](https://telegram.org/blog/android-2-0#hiding-last-seen-time) |
| One-way presence and read-time visibility | Apply Premium visibility exceptions within Telegram's rules. | Premium; other users' privacy still applies | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#one-way-last-seen-and-read-times) |
| Voice and video-note privacy | Restrict who can send voice or video notes. | Premium; privacy exceptions | [Source](https://telegram.org/blog/custom-emoji#privacy-settings-for-voice-messages) |
| Private-message eligibility | Restrict first contact from eligible non-contacts. | Premium/configuration | [Source](https://telegram.org/blog/new-saved-messages-and-9-more#private-message-permissions) |
| Forward attribution privacy | Limit linking a forwarded message to a profile. | Standard | [Source](https://telegram.org/blog/unsend-privacy-emoji#anonymous-forwarding) |
| Profile photo privacy | Choose who can see profile media. | Standard; exceptions | [Source](https://telegram.org/blog/privacy-revolution) |
| Group-invitation privacy | Choose who may add the account to chats. | Standard; exceptions | [Source](https://telegram.org/blog/voice-2-secret-3#and) |
| Call privacy | Choose who may call and applicable connection privacy. | Standard; exceptions | [Source](https://core.telegram.org/api/privacy) |
| Block and report | Block a sender and report abusive content. | Standard; reporting flow | [Source](https://telegram.org/blog/gifts-verification-platform) |
| Private-chat sharing restriction | Restrict supported forwarding, saving and capture actions. | Premium creation; platform enforcement differs | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#disable-sharing-in-private-chats) |
| Secret Chats | Use device-bound end-to-end encrypted conversations. | Supported native clients; no cloud-history sync | [Source](https://core.telegram.org/api/end-to-end) |
| Secret-chat key verification | Compare keys for a Secret Chat. | Secret Chat participants | [Source](https://core.telegram.org/api/end-to-end) |
| Secret-chat expiration | Expire messages under a Secret Chat timer. | Secret Chat configuration | [Source](https://core.telegram.org/api/end-to-end) |
| Secret-chat screenshot notifications | Receive supported capture notifications. | OS-dependent; no absolute capture guarantee | [Source](https://telegram.org/faq#secret-chats) |
| Cloud-chat auto-delete | Apply an automatic message retention timer. | Chat permissions and policy | [Source](https://telegram.org/blog/autodelete-inv2#auto-delete-messages) |
| Global auto-delete | Set a default expiration for new eligible chats. | Account configuration | [Source](https://telegram.org/blog/ultimate-privacy-topics-2-0#auto-delete-all-chats) |
| Local app passcode | Protect entry to the local application. | OS/client support | [Source](https://telegram.org/blog/photo-editor-and-passcodes#passcode-lock) |
| Biometric app unlock | Unlock protected local application access. | Supported OS/device | [Source](https://telegram.org/blog/archive-and-new-design#you-shall-not-pass) |
| Two-step verification | Add an account password to login protection. | Account setup and recovery rules | [Source](https://telegram.org/blog/sessions-and-2-step-verification#two-step-verification) |
| Passkeys | Authenticate through a supported device credential. | Supported client/credential provider | [Source](https://telegram.org/blog/passkeys-and-gift-offers#passkeys) |
| QR login | Authorize another supported session with a QR flow. | Existing authorized session | [Source](https://core.telegram.org/api/auth) |
| Login alerts | Notify the account about a new session. | Standard | [Source](https://telegram.org/blog/channel-stories#new-login-alerts) |
| Active sessions | Inspect and revoke linked account sessions. | Account authorization | [Source](https://telegram.org/blog/sessions-and-2-step-verification) |
| Connected websites | Inspect and revoke supported web login permissions. | Account authorization | [Source](https://telegram.org/blog/login#stay-in-control) |
| Multiple accounts | Switch between isolated accounts in the client. | Standard; expanded Premium account limit | [Source](https://telegram.org/blog/themes-accounts#multiple-accounts) |
| Phone-number changes | Move an existing account to another number. | Account verification | [Source](https://telegram.org/blog/telegram-me-change-number-and-pfs#changing-your-phone-number) |
| Account inactivity deletion | Configure deletion after an inactivity period. | Account configuration | [Source](https://telegram.org/blog/android-2-0#account-self-destruction) |
| Delete account | Delete an account through the supported lifecycle flow. | Account verification; consequences apply | [Source](https://telegram.org/faq#your-account) |
| Account restriction appeal | Review restrictions and submit an appeal when offered. | Restricted-account eligibility | [Source](https://telegram.org/blog/group-calls-made-easy#appeal-account-restrictions-with-frozen-accounts) |
| Recovery and login reminders | Configure recovery information and receive security reminders. | Account/client configuration | [Source](https://telegram.org/blog/video-1000#password-recovery-and-reminders) |

## Business and chat automation

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Business location | Show a business's map location on its profile. | Premium/Business | [Source](https://telegram.org/blog/telegram-business#hours-and-location) |
| Business opening hours | Display opening hours using supported timezone data. | Premium/Business | [Source](https://core.telegram.org/api/business#opening-hours) |
| Business start page | Customize the introduction shown before the first message. | Premium/Business | [Source](https://telegram.org/blog/telegram-business#start-page) |
| Quick-reply shortcuts | Insert reusable text, media or multi-message responses. | Premium/Business; private chats | [Source](https://telegram.org/blog/telegram-business#quick-replies) |
| Greeting automation | Send a configured greeting to eligible new contacts. | Premium/Business; recipient scope | [Source](https://telegram.org/blog/telegram-business#greeting-messages) |
| Away automation | Respond during configured absence or closing periods. | Premium/Business; recipient scope | [Source](https://telegram.org/blog/telegram-business#away-messages) |
| Automation exclusions | Exclude chosen chats or audience categories from automation. | Feature-specific rights and scope | [Source](https://telegram.org/blog/group-calls-made-easy#automated-business-accounts) |
| Business chat links | Open a chat with a prepared message and track link usage. | Premium/Business | [Source](https://telegram.org/blog/telegram-business#links-to-chat) |
| Connected profile bots | Delegate selected account operations to a connected bot. | Available beyond Premium; explicit rights | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#chat-automation-in-profiles) |
| Pause or revoke automation | Stop a connected bot for a chat or the whole account. | Account owner | [Source](https://core.telegram.org/api/bots/connected-business-bots) |
| Scoped business bot actions | Allow only approved messaging or account operations. | Account owner grants; feature-specific rights | [Source](https://core.telegram.org/api/bots/connected-business-bots) |

## Bots and agent experiences

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Bot conversations | Talk to a service through a bot account. | Standard; service may charge | [Source](https://telegram.org/blog/bot-revolution) |
| Slash commands | Discover and invoke named bot actions. | Bot configuration | [Source](https://core.telegram.org/bots/features#commands) |
| Reply keyboards | Choose actions from a bot-provided keyboard. | Bot/client support | [Source](https://core.telegram.org/bots/features#keyboards) |
| Inline action buttons | Interact through buttons attached to a bot message. | Bot/client support | [Source](https://core.telegram.org/bots/features#inline-keyboards) |
| Colored bot buttons | Use supported visual button styles. | Bot/client support | [Source](https://telegram.org/blog/crafting-android-design-and-more#colored-buttons-for-bots) |
| Bot menu | Open commands or an app from the bot conversation. | Bot configuration | [Source](https://telegram.org/blog/animated-backgrounds#bot-menu) |
| Inline bots | Retrieve and insert bot content from another chat's composer. | Bot inline mode | [Source](https://telegram.org/blog/inline-bots) |
| Guest bots | Invoke an assistant with only the invocation's permitted context. | Guest-enabled bot; no ambient history grant | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#guest-bots) |
| Bot-to-bot conversations | Let enabled bots coordinate with other bots. | Bot modes; loop and rate protections | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#bot-to-bot-communication) |
| Managed bots | Create and manage a bot through an authorized managing bot. | Owner confirmation and manager grants | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#managed-bots) |
| Streaming responses | Show incremental bot output and supported cancellation. | Bot/client support | [Source](https://telegram.org/blog/comments-in-video-chats-threads-for-bots#threads-and-streaming-responses-for-ai-bots) |
| Bot conversation topics | Organize a private bot conversation into topics. | Bot feature configuration | [Source](https://telegram.org/blog/comments-in-video-chats-threads-for-bots#threads-and-streaming-responses-for-ai-bots) |
| Ephemeral group commands | Send supported commands privately within a group. | Bot command configuration | [Source](https://telegram.org/blog/communities-editor-invisible-messages#ephemeral-bot-messages) |
| Ephemeral group responses | Show a bot response only to its intended recipient. | Bot/client support; separate data scope | [Source](https://telegram.org/blog/communities-editor-invisible-messages#ephemeral-bot-messages) |
| Bot reaction support | Receive or emit supported reaction interactions. | Bot permissions and update selection | [Source](https://telegram.org/blog/calls-and-bots#epic-bot-update-reactions-and-much-more) |
| Bot chat and user selection | Ask the user to choose a chat or person explicitly. | User confirmation and eligibility | [Source](https://telegram.org/blog/profile-pics-emoji-translations#chat-selection-for-bots) |
| Bot localization | Present names, descriptions and commands by language. | Bot configuration | [Source](https://core.telegram.org/bots/features#language-support) |
| Bot deep links | Start a bot flow with a supported parameter. | Bot/client support | [Source](https://core.telegram.org/bots/features#deep-linking) |
| Bot group privacy | Limit received group content through privacy mode. | Bot/admin configuration | [Source](https://core.telegram.org/bots/features#privacy-mode) |
| Bot administration tools | Moderate eligible groups using scoped bot permissions. | Group admin grants | [Source](https://core.telegram.org/bots/features) |
| BotFather management | Configure bot identity, commands and supported features. | Bot owner | [Source](https://telegram.org/blog/post-search-story-albums-and-more#botfather-mini-app) |
| Bot payments | Present supported payment or invoice flows. | Merchant, region and payment rules | [Source](https://telegram.org/blog/payments#check-it-out-now) |
| HTML games | Launch supported games with chat-linked results. | Bot/game/client support | [Source](https://telegram.org/blog/games#ready-to-play) |
| Web login | Authenticate to an external service using Telegram identity. | User authorization; not inbox access | [Source](https://telegram.org/blog/member-tags-disable-sharing-and-more#log-in-with-telegram) |
| Verification-code delivery | Receive supported login codes through Telegram Gateway. | External service integration and delivery pricing | [Source](https://core.telegram.org/gateway) |

## Apps and embedded experiences

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| App launch points | Open from profiles, buttons, links or eligible menus. | Launch eligibility | [Source](https://core.telegram.org/bots/webapps) |
| App directory | Discover supported apps through Telegram's discovery surfaces. | Service/client availability | [Source](https://telegram.org/blog/w3-browser-mini-app-store) |
| Fullscreen apps | Expand an app into a full-screen experience. | Client/API version support | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#full-screen-mode) |
| Portrait and landscape apps | Use supported full-screen orientation modes. | Client/API version; orientation controls | [Source](https://core.telegram.org/bots/webapps) |
| App safe areas | Respect system and host insets. | Host/client support | [Source](https://core.telegram.org/bots/webapps) |
| App theme and viewport events | Adapt to host appearance and layout. | Host/client support | [Source](https://core.telegram.org/bots/webapps) |
| App back and close controls | Coordinate navigation and unsaved-work confirmation. | Host/client support | [Source](https://core.telegram.org/bots/webapps) |
| App primary and secondary actions | Use host action buttons. | Host/client support | [Source](https://core.telegram.org/bots/webapps) |
| App haptics | Request tactile feedback. | Device/client support | [Source](https://core.telegram.org/bots/webapps) |
| App home-screen shortcuts | Add an approved shortcut on the device home screen. | OS/client support; user action | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#home-screen-shortcuts) |
| App switching bar | Keep and return to open embedded apps. | Client support | [Source](https://telegram.org/blog/mini-app-bar-paid-media-and-more#mini-app-bar) |
| App loading identity | Customize supported loading icon and theme colors. | Bot/app configuration | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#loading-screen-customization) |
| App geolocation | Request location through the host permission flow. | Explicit permission; device support | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#geolocation-access) |
| App motion sensors | Use supported accelerometer, gyroscope and orientation events. | Device/client support | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#device-motion-tracking) |
| App orientation lock | Request or release a supported orientation lock. | Client/API version support | [Source](https://core.telegram.org/bots/webapps) |
| App local storage | Store bounded device-local data. | Host quotas; device-local durability | [Source](https://core.telegram.org/bots/webapps) |
| App secure storage | Use host-supported secure local storage. | OS/client/API support | [Source](https://core.telegram.org/bots/webapps) |
| App cloud key-value storage | Store supported small data through the platform. | Host API quotas | [Source](https://core.telegram.org/bots/webapps) |
| App biometric authorization | Request supported biometric interaction through the host. | User permission and device support | [Source](https://core.telegram.org/bots/webapps) |
| App sharing to chats | Let the user share prepared app content to a chat. | User action and destination access | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#media-sharing) |
| App sharing to stories | Offer prepared content for the story composer. | User action and story permissions | [Source](https://telegram.org/blog/w3-browser-mini-app-store#sharing-from-mini-apps-to-stories) |
| App document downloads | Prompt the user to save an app-produced document. | User confirmation; supported file delivery | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#creating-documents) |
| App subscriptions | Offer supported recurring paid access. | Stars/payment and platform rules | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#subscription-plans) |
| App gifts and emoji status | Request supported account actions from an app. | Explicit user grants and account eligibility | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#gifts-from-apps) |
| App hardware hints | Adapt performance to host-provided device information. | Android-specific API support | [Source](https://telegram.org/blog/fullscreen-miniapps-and-more#device-hardware-info) |
| App QR scanner and clipboard | Use supported host-mediated input capabilities. | User interaction and host restrictions | [Source](https://core.telegram.org/bots/webapps) |
| App contact and write access | Ask for contact information or permission to message. | Explicit user approval | [Source](https://core.telegram.org/bots/webapps) |
| App chat picker | Select an eligible destination. | User confirmation; API support | [Source](https://core.telegram.org/bots/webapps) |
| App initialization verification | Verify signed initialization data. | Developer implementation; no chat-history grant | [Source](https://core.telegram.org/bots/webapps) |

## AI, translation and text assistance

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Selected-message translation | Translate a selected message. | Standard; supported languages | [Source](https://telegram.org/blog/reactions-spoilers-translations#message-translation) |
| Whole-chat translation | Translate an eligible conversation as it is read. | Premium; service and language support | [Source](https://telegram.org/blog/profile-pics-emoji-translations#translating-entire-chats) |
| Channel and article summaries | Show supported generated summaries. | Service, client and content scope | [Source](https://telegram.org/blog/new-design-ai-summaries#ai-summaries) |
| Composer grammar correction | Ask the built-in text editor to revise draft wording. | AI feature quotas and availability | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#ai-text-editor) |
| Composer style rewriting | Transform a draft into a selected writing style. | AI feature quotas and availability | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#ai-text-editor) |
| Composer translation | Translate a draft before sending. | AI feature quotas and availability | [Source](https://telegram.org/blog/ai-editor-mighty-polls-and-more#ai-text-editor) |
| Custom AI styles | Create and share reusable text-editor prompts. | Account-dependent saved-style limits | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#custom-ai-styles) |
| AI style previews | Preview a shared style before adding it. | Service and account limits | [Source](https://telegram.org/blog/ai-bot-revolution-11-new-features#custom-ai-styles) |
| Confidential AI execution | Use Telegram's advertised confidential AI processing service. | Telegram service; client code alone insufficient | [Source](https://core.telegram.org/api/ai) |

## Premium subscriptions and benefits

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Premium subscription management | Purchase, restore or manage supported subscriptions. | Paid; store, region and billing channel | [Source](https://core.telegram.org/api/premium) |
| Premium gifts | Give an eligible subscription to another person. | Paid; recipient and store rules | [Source](https://telegram.org/blog/custom-emoji#gifting-telegram-premium) |
| Premium gifting with Stars | Purchase eligible subscription gifts using Stars. | Stars balance and offer rules | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#gift-premium-with-stars) |
| Bulk subscription gifts | Give subscriptions to several recipients. | Paid; purchase limits | [Source](https://telegram.org/blog/posts-in-stories-and-more#massive-premium-gifts) |
| Sponsored-message removal | Hide Telegram's supported sponsored messages. | Premium; not ordinary creator promotions | [Source](https://telegram.org/faq_premium#q-why-do-i-still-see-advertisements-in-channels) |
| Expanded account limits | Increase selected account quotas. | Premium; each limit configured separately | [Source](https://core.telegram.org/api/premium#double-limits) |
| Premium app icons | Choose additional application icons. | Premium; OS/client support | [Source](https://telegram.org/blog/700-million-and-premium#premium-app-icons) |
| Premium account capacity | Add an extra account where supported. | Premium; client/config limit | [Source](https://telegram.org/blog/themes-accounts#multiple-accounts) |
| Expanded AI text allowance | Receive the larger daily text-transformation allowance advertised by the API. | Premium; currently advertised as 50x; live quota not queried | [Source](https://core.telegram.org/api/premium#ai-compose) |
| Premium-gated limited gifts | Buy selected limited gift releases. | Premium plus purchase cap; availability varies | [Source](https://telegram.org/blog/post-search-story-albums-and-more#premium-gifts) |

## Boosts and community entitlements

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Boost allocation | Assign eligible boost slots to a group or channel. | Premium/gift/giveaway slot ownership | [Source](https://core.telegram.org/api/boost) |
| Boost reassignment | Move a boost under the platform's timing rules. | Slot owner; cooldown rules | [Source](https://core.telegram.org/api/boost) |
| Chat levels | Aggregate boosts to unlock chat capabilities. | Chat-specific boost thresholds | [Source](https://core.telegram.org/api/boost) |
| Boosted channel stories | Unlock channel story publishing capacity. | Chat level and admin rights | [Source](https://telegram.org/blog/channel-stories#stories-for-channels) |
| Boosted group stories | Unlock group story publishing capacity. | Chat level and admin rights | [Source](https://telegram.org/blog/february2024#stories-from-groups) |
| Boosted chat colors | Unlock additional chat identity colors. | Chat level and admin rights | [Source](https://telegram.org/blog/posts-in-stories-and-more#customize-channel-appearance) |
| Boosted chat wallpaper | Unlock supported custom chat wallpaper features. | Chat level and admin rights | [Source](https://telegram.org/blog/posts-in-stories-and-more#customize-channel-appearance) |
| Boosted chat emoji status | Set an eligible group or channel emoji status. | Chat level and admin rights | [Source](https://telegram.org/blog/posts-in-stories-and-more#customize-channel-appearance) |
| Boosted custom reactions | Expand permitted channel reactions. | Chat level and admin rights | [Source](https://telegram.org/faq_premium#premium-features-for-channels) |
| Group custom emoji packs | Make an eligible emoji pack available to group members. | Chat level and admin rights | [Source](https://telegram.org/blog/february2024#group-emoji-packs) |
| Group transcription allowance | Unlock eligible voice transcription for members. | Chat level; service limits | [Source](https://telegram.org/blog/february2024#voice-to-text-and-more) |
| Booster permissions | Exempt eligible boosters from configured restrictions. | Admin settings and boost entitlement | [Source](https://telegram.org/blog/february2024#permissions-for-boosters) |
| Channel sponsored-message controls | Unlock eligible channel-wide ad suppression. | Chat level; current policy | [Source](https://core.telegram.org/api/boost) |
| Giveaway boost grants | Gain boosts from eligible subscription giveaways. | Paid giveaway and claim rules | [Source](https://telegram.org/blog/giveaways#more-boosts-for-users) |

## Stars, commerce, gifts and creator economy

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Stars balance and transaction history | Inspect a virtual-credit balance and its transactions. | Account, region and billing rules | [Source](https://core.telegram.org/api/stars) |
| Stars purchases and gifts | Acquire or give eligible Stars balances. | Paid; store and region rules | [Source](https://telegram.org/blog/w3-browser-mini-app-store#gifting-stars-to-friends) |
| Paid messages | Charge eligible non-contacts to initiate messages. | Account eligibility, price and exceptions | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#stay-in-control-of-your-inbox-with-star-messages) |
| Paid media | Unlock supported photos or videos using Stars. | Publisher pricing and payment rules | [Source](https://telegram.org/blog/mini-app-bar-paid-media-and-more#pay-for-content-with-telegram-stars) |
| Paid reactions | Support a channel using Stars reactions. | Paid; optional sender visibility | [Source](https://telegram.org/blog/superchannels-star-reactions-subscriptions#star-reactions) |
| Paid channel subscriptions | Join an eligible paid channel through a subscription. | Stars and channel eligibility | [Source](https://telegram.org/blog/superchannels-star-reactions-subscriptions#star-subscriptions) |
| Paid bot and app content | Purchase digital access from bots or embedded apps. | Stars and platform rules | [Source](https://telegram.org/blog/superchannels-star-reactions-subscriptions#paid-media-for-bots) |
| Funded suggested posts | Offer compensation for an approved channel post. | Paid; acceptance and settlement rules | [Source](https://telegram.org/blog/checklists-suggested-posts#suggest-posts-in-channels) |
| Creator ad revenue | Participate in an eligible revenue-sharing program. | Eligibility, region and payout rules | [Source](https://telegram.org/blog/monetization-for-channels#rewards-for-creators) |
| Bot ad revenue | Participate in eligible bot advertising revenue. | Bot eligibility and payout rules | [Source](https://telegram.org/blog/dynamic-video-quality-and-more#telegram-ads-in-bots) |
| Telegram advertising | Run supported sponsored campaigns. | Paid; advertising policy | [Source](https://telegram.org/blog/monetization-for-channels#ton-based-ads) |
| Creator and developer rewards | Access eligible reward and advertising programs. | Program-specific terms | [Source](https://telegram.org/blog/mini-app-bar-paid-media-and-more#rewards-for-content-creators-and-developers) |
| Affiliate programs | Receive eligible rewards for app referrals. | Program, purchase and payout rules | [Source](https://telegram.org/blog/affiliate-programs-ai-sticker-search#affiliate-programs) |
| Premium giveaways | Distribute subscription prizes through a giveaway. | Paid; eligibility and selection rules | [Source](https://telegram.org/blog/giveaways#free-telegram-premium) |
| Star giveaways | Distribute Stars through eligible giveaways. | Paid; eligibility and selection rules | [Source](https://telegram.org/blog/star-giveaways-iv-in-browser#star-giveaways) |
| Custom giveaway prizes | Attach descriptions and supported prize information. | Giveaway owner; policy | [Source](https://telegram.org/blog/posts-in-stories-and-more#custom-prizes-and-winner-lists-in-giveaways) |
| Gifts | Send supported gifts to users or channels. | Paid/Stars; supply and recipient rules | [Source](https://telegram.org/blog/gifts-verification-platform#gifts) |
| Collectible gifts | Upgrade or acquire uniquely attributed gift variants. | Paid and collectible rules | [Source](https://telegram.org/blog/collectible-gifts-and-more#collectible-gifts) |
| Gift marketplace | Buy or sell eligible gifts through supported markets. | Ownership, pricing and fees | [Source](https://telegram.org/blog/gift-marketplace-and-more#buy-rare-gifts-in-the-marketplace) |
| Gift auctions | Bid for eligible gift releases. | Paid; auction rules | [Source](https://telegram.org/blog/live-stories-gift-auctions#auctions-for-new-gifts) |
| Gift purchase offers | Make a supported offer for another person's gift. | Funds, owner approval and settlement | [Source](https://telegram.org/blog/passkeys-and-gift-offers#gift-purchase-offers) |
| Gift crafting | Combine eligible collectibles to create another variant. | Ownership and crafting rules | [Source](https://telegram.org/blog/crafting-android-design-and-more#gift-crafting) |
| Gift display and pinning | Feature eligible gifts on a profile. | Owner and display permissions | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#pin-gifts-on-your-profile) |
| Gift collections | Organize gifts into named collections. | Owner/profile rights | [Source](https://telegram.org/blog/post-search-story-albums-and-more#gift-collections) |
| Gift-based themes | Apply an eligible gift's theme to a conversation. | Gift ownership and client support | [Source](https://telegram.org/blog/profile-music-gift-themes#gift-themes) |
| Gift signatures | Include or remove supported sender attribution. | Purchase and owner permissions | [Source](https://telegram.org/blog/welcome-messages-buttons-TG-13#signed-gifts-from-the-marketplace) |
| Gift upgrades for others | Fund an eligible upgrade for another user's gift. | Paid; gift eligibility | [Source](https://telegram.org/blog/profile-music-gift-themes#upgrading-gifts-for-other-users) |
| Gift value information | Inspect available valuation information. | Marketplace/service data availability | [Source](https://telegram.org/blog/profile-music-gift-themes#gift-value) |
| Gift reception controls | Choose which gift types the account accepts. | Account settings and eligibility | [Source](https://telegram.org/blog/group-calls-made-easy#get-more-gifts-with-these-new-settings) |
| Collectible transfers | Transfer eligible gift ownership. | Owner, fee and holding rules | [Source](https://core.telegram.org/api/gifts) |
| Blockchain gift export | Move an eligible collectible through a supported blockchain flow. | Ownership, fees and third-party rules | [Source](https://telegram.org/blog/wear-gifts-blockchain-and-more#move-gifts-to-the-blockchain) |
| Stars withdrawal | Withdraw eligible earnings through the supported provider. | Eligibility, delays and payout policy | [Source](https://telegram.org/blog/star-messages-gateway-2-0-and-more#withdraw-stars-via-fragment) |

## Notifications, appearance and accessibility

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Notification exceptions | Configure alerts for selected chats or people. | OS permission and account settings | [Source](https://telegram.org/blog/export-and-more#exceptions-in-notifications) |
| Custom notification sounds | Assign supported audio as an alert. | OS/client support | [Source](https://telegram.org/blog/notifications-bots#custom-notification-sounds) |
| Custom mute duration | Silence a chat for a chosen interval. | Standard | [Source](https://telegram.org/blog/notifications-bots#custom-mute-durations) |
| Reaction notification controls | Choose which reaction alerts appear. | Client/account settings | [Source](https://telegram.org/blog/my-profile-and-15-more#reaction-notification-settings) |
| Unread badge policy | Count chats or messages and include chosen categories. | OS/client support | [Source](https://telegram.org/blog/telegram-5-ios#make-every-chat-count) |
| Expanded notifications | Read or interact with supported notification previews. | OS/privacy/client support | [Source](https://telegram.org/blog/telegram-5-ios#step-aside-and-back) |
| Siri message announcements | Announce eligible messages through Siri. | iOS settings and compatible device | [Source](https://telegram.org/blog/voice-chats#announce-messages-on-ios) |
| Cloud themes | Share and synchronize supported appearance themes. | Client/theme support | [Source](https://telegram.org/blog/scheduled-reminders-themes#custom-cloud-themes) |
| Theme editor | Customize supported palette and appearance parameters. | Client-specific tools | [Source](https://telegram.org/blog/verifiable-apps-and-more#theme-editor-2-0) |
| Light, dark and automatic themes | Adapt the application appearance manually or automatically. | OS/client support | [Source](https://telegram.org/blog/discover-stickers-and-more#auto-night-mode-and-connected-websites) |
| Animated chat backgrounds | Animate supported gradient or wallpaper backgrounds. | Client/power settings | [Source](https://telegram.org/blog/animated-backgrounds#animated-backgrounds) |
| Photo and patterned wallpapers | Use images, colors or patterns behind a conversation. | Standard; client support | [Source](https://telegram.org/blog/backgrounds-2-0) |
| Wallpaper motion and blur | Apply supported effects to a background. | Client/power settings | [Source](https://telegram.org/blog/backgrounds-2-0) |
| Shared wallpaper links | Share wallpaper configuration through a link. | Client support | [Source](https://telegram.org/blog/backgrounds-2-0#share-links) |
| Wallpapers for both sides | Propose or set supported shared private-chat appearance. | Premium; counterpart controls | [Source](https://telegram.org/blog/similar-channels#wallpapers-for-both-sides) |
| Chat-specific themes | Use a distinct appearance in a chosen conversation. | Client/chat support | [Source](https://telegram.org/blog/chat-themes-interactive-emoji-read-receipts#chat-themes) |
| Font scaling | Adjust supported interface and message text sizes. | OS/client support | [Source](https://telegram.org/blog/verifiable-apps-and-more#text-size-all-the-way) |
| VoiceOver and TalkBack | Navigate with screen readers. | Platform/client implementation | [Source](https://telegram.org/blog/unsend-privacy-emoji#voiceover-and-talkback) |
| RTL and localization | Use supported languages and right-to-left layouts. | Client/language-pack support | [Source](https://telegram.org/blog/translations-iv2) |
| Power-saving controls | Reduce animations, autoplay or background activity. | Client/device support | [Source](https://telegram.org/blog/power-saving#power-saving-mode) |
| High-refresh-rate UI | Render supported interface transitions at higher refresh rates. | iOS/device support | [Source](https://telegram.org/blog/700-million-and-premium#faster-and-stronger) |

## Storage, connectivity and platform integration

| Feature | Behavior | Availability / gate | Official reference |
|---|---|---|---|
| Cache management | Inspect and clear cached media without deleting cloud messages. | Client support; cloud/Secret Chat distinction | [Source](https://telegram.org/blog/cache-and-stickers) |
| Per-chat cache controls | Clear or inspect cached media for a chosen chat. | Client support | [Source](https://telegram.org/blog/cache-and-stickers) |
| Media auto-removal | Apply different local retention policies to cached media. | Client settings | [Source](https://telegram.org/blog/hidden-media-zero-storage-profile-pics#zero-storage-usage) |
| Storage usage breakdown | Inspect storage by media category or chat. | Client support | [Source](https://telegram.org/blog/ultimate-privacy-topics-2-0#detailed-storage-usage-and-more-on-android) |
| Automatic download presets | Choose automatic download behavior by connection and media. | Client/network settings | [Source](https://telegram.org/blog/autoplay) |
| Save-to-gallery controls | Choose which received media is saved to the OS library. | OS permission and media/chat settings | [Source](https://telegram.org/blog/profile-pics-emoji-translations#auto-save-incoming-media) |
| Network usage charts | Inspect traffic usage by category. | Client support | [Source](https://telegram.org/blog/profile-pics-emoji-translations#network-usage) |
| Proxy configuration | Connect through supported proxy protocols. | Client/network support | [Source](https://telegram.org/blog/admin-revolution#free-speech) |
| Chat history import | Import supported history exports from other messengers. | Supported import format; destination rules | [Source](https://telegram.org/blog/move-history) |
| Account and chat export | Export available data through a supported desktop client. | Desktop-specific; authorization rules | [Source](https://telegram.org/blog/export-and-more) |
| Operating-system share integration | Share into Telegram using supported system surfaces. | OS/client support | [Source](https://telegram.org/blog/verifiable-apps-and-more#share-sheet-settings) |
| Home-screen widgets | Open selected conversations from a device widget. | OS/client support | [Source](https://telegram.org/blog/autodelete-inv2#home-screen-widgets) |
| Tablet and desktop layouts | Use supported multi-column or compact layouts. | Client/window size support | [Source](https://telegram.org/blog/desktop-compact) |
| Web clients | Access Telegram through supported browser clients. | Browser/client support; feature parity varies | [Source](https://telegram.org/blog/payments-2-0-scheduled-voice-chats#new-web-versions) |
| Watch applications | Read and interact through supported watch clients. | Watch platform/client support | [Source](https://telegram.org/blog/watch-apps-and-more#watch-apps-for-everyone) |
| Location messages | Send a selected location or venue. | Sender choice; device permission if needed | [Source](https://telegram.org/blog/verifiable-apps-and-more#improved-venue-sharing) |
| Live location | Share continuously updated location for a chosen period. | Explicit consent and background location rules | [Source](https://telegram.org/blog/live-locations) |
| Extended live-location duration | Extend or maintain a shared live location. | Explicit sender choice | [Source](https://telegram.org/blog/my-profile-and-15-more#share-location-indefinitely) |
| Location proximity alerts | Receive a notification at a configured distance. | Active location sharing and permissions | [Source](https://telegram.org/blog/pinned-messages-locations-playlists#improved-live-locations) |
| External map integration | Open a shared location in a supported map app. | OS/app availability | [Source](https://telegram.org/blog/ultimate-privacy-topics-2-0#detailed-storage-usage-and-more-on-android) |
| Travel time estimates | Show supported travel-time information for a place. | Map/service/client availability | [Source](https://telegram.org/blog/shared-media-scrolling-calendar-join-requests-and-more#transit-times-for-shared-locations-on-ios) |
| Public-message embeds | Embed supported public messages on a website. | Public content and widget rules | [Source](https://telegram.org/blog/themes-accounts#in-case-you-missed-it) |
| Discussion widgets | Embed an eligible discussion surface on a website. | Website/widget configuration | [Source](https://telegram.org/blog/silent-messages-slow-mode#comments-widget) |
| Telegraph publishing | Publish standalone articles using the related service. | Separate service and content rules | [Source](https://telegram.org/blog/instant-view#telegraph) |
| Instant View publishing tools | Define supported article-reading templates. | Template and platform requirements | [Source](https://telegram.org/blog/instant-view-contest-200K#instant-what) |
| Telegram Passport | Share supported identity documents through the identity service. | Separate identity flow; not ordinary chat storage | [Source](https://telegram.org/blog/passport) |

