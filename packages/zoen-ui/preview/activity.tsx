import { useState } from 'react';
import { conversationLabels, sampleChats } from './inbox';

type ActivityPresentation = 'cards' | 'list';
const updates = sampleChats.filter((chat) => (chat.unreadCount ?? 0) > 0);

export function SampleActivity({ onSelectChat }: { onSelectChat: (id: string) => void }) {
  const [presentation, setPresentation] = useState<ActivityPresentation>('cards');
  return (
    <ActivityUpdates
      presentation={presentation}
      onPresentationChange={setPresentation}
      onSelectChat={onSelectChat}
    />
  );
}

export function ActivityUpdates({
  presentation,
  onPresentationChange,
  onSelectChat,
}: {
  presentation: ActivityPresentation;
  onPresentationChange: (presentation: ActivityPresentation) => void;
  onSelectChat: (id: string) => void;
}) {
  const [index, setIndex] = useState(0);
  const current = updates[index];
  return (
    <>
      <span className="preview-eyebrow">Sample updates</span>
      <div className="preview-activity-heading">
        <h1>Recent activity</h1>
        <button
          type="button"
          className="preview-activity-view-button"
          aria-label={`Show sample updates as ${presentation === 'cards' ? 'a list' : 'cards'}`}
          onClick={() => onPresentationChange(presentation === 'cards' ? 'list' : 'cards')}
        >
          {presentation === 'cards' ? 'List' : 'Cards'}
        </button>
      </div>
      <p>Open a sample update to return to its conversation.</p>
      {presentation === 'list' ? (
        <div className="preview-results preview-activity-list">
          {updates.map((chat) => (
            <button key={chat.id} type="button" onClick={() => onSelectChat(chat.id)}>
              <strong>{chat.title}</strong>
              <span>
                {chat.unreadCount} sample unread {chat.unreadCount === 1 ? 'message' : 'messages'}
              </span>
            </button>
          ))}
        </div>
      ) : current ? (
        <div className="preview-activity-cards">
          <div className="preview-activity-deck">
            {index + 2 < updates.length && (
              <span className="preview-activity-ghost" data-depth="two" aria-hidden="true" />
            )}
            {index + 1 < updates.length && (
              <span className="preview-activity-ghost" data-depth="one" aria-hidden="true" />
            )}
            <article className="preview-activity-card" aria-label={`Update from ${current.title}`}>
              <div>
                <span className="preview-activity-avatar" aria-hidden="true">
                  {current.avatar}
                </span>
                <span className="preview-eyebrow">
                  Sample {conversationLabels[current.kind].toLowerCase()} update
                </span>
                <h2>{current.title}</h2>
                <p>{current.preview}</p>
                <small>
                  {current.unreadCount} sample unread{' '}
                  {current.unreadCount === 1 ? 'message' : 'messages'}
                </small>
              </div>
              <button
                type="button"
                className="preview-activity-open"
                aria-label={`Open ${current.title} conversation`}
                onClick={() => onSelectChat(current.id)}
              >
                Open chat
              </button>
            </article>
          </div>
          <div className="preview-activity-pagination">
            <button
              type="button"
              aria-label="Previous sample update"
              disabled={index === 0}
              onClick={() => setIndex((position) => position - 1)}
            >
              Previous
            </button>
            <span role="status" aria-live="polite" aria-atomic="true">
              {index + 1} of {updates.length}
            </span>
            <button
              type="button"
              aria-label="Next sample update"
              disabled={index === updates.length - 1}
              onClick={() => setIndex((position) => position + 1)}
            >
              Next
            </button>
          </div>
        </div>
      ) : (
        <p>No sample updates.</p>
      )}
    </>
  );
}
