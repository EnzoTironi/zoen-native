import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ZoenShell, ShellIcon, type ShellTheme } from '@zoen/ui/shell';
import {
  CommunityResources,
  InboxFilters,
  conversationLabels,
  filterInbox,
  navigation,
  sampleChats,
  type Destination,
  type InboxFilter,
} from './inbox';
import { SampleActivity } from './activity';
import '@zoen/ui/shell.css';
import './preview.css';

const preferenceKey = 'zoen.shell.preview.theme';
const sampleMessages: readonly { author: 'Marina' | 'You'; text: string; time: string }[] = [
  { author: 'Marina', text: 'I collected a few references for the frame.', time: '10:18' },
  { author: 'You', text: "Let's start with the conversation itself.", time: '10:19' },
  { author: 'Marina', text: 'A place to read without so much around it.', time: '10:20' },
  {
    author: 'You',
    text: 'Keep the things we need close, and give the rest some room.',
    time: '10:21',
  },
  { author: 'Marina', text: 'The chat panel can stay small and easy to hide.', time: '10:23' },
  { author: 'You', text: 'And search is still one click away in the rail.', time: '10:24' },
  {
    author: 'Marina',
    text: 'I like seeing just the current chat name above the messages.',
    time: '10:26',
  },
  {
    author: 'You',
    text: 'The conversation should keep flowing behind that little header.',
    time: '10:28',
  },
  { author: 'Marina', text: 'That leaves more space for the actual words.', time: '10:30' },
  { author: 'You', text: 'Especially on a narrow screen.', time: '10:32' },
  { author: 'Marina', text: 'We can keep the appearance control quiet, too.', time: '10:34' },
  { author: 'You', text: 'One little control, right where it belongs.', time: '10:36' },
  {
    author: 'Marina',
    text: 'What if everything around the conversation was a little quieter?',
    time: '10:38',
  },
  { author: 'You', text: 'A small rail, and just the chats we need beside it.', time: '10:40' },
];

function readTheme(): ShellTheme {
  try {
    const stored = localStorage.getItem(preferenceKey);
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  } catch {
    /* Storage is optional in the preview. */
  }
  return 'system';
}

function DemoStore() {
  const [selected, setSelected] = useState('notes');
  const tools = [
    {
      id: 'notes',
      title: 'Shared notes',
      description: 'Keep ideas beside the conversation.',
      example: 'A calmer frame\nA compact chat panel\nOne place for context',
    },
    {
      id: 'polls',
      title: 'Quick poll',
      description: 'Find a time that works for everyone.',
      example: 'When should we meet?\nSaturday morning\nSaturday afternoon\nSunday morning',
    },
    {
      id: 'planner',
      title: 'Weekly plan',
      description: 'Make a little room for the week ahead.',
      example:
        'Monday: collect references\nWednesday: review the frame\nFriday: share the next draft',
    },
  ];
  const tool = tools.find((item) => item.id === selected);
  return (
    <>
      <span className="preview-eyebrow">Sample catalog</span>
      <h1>Tools for your conversations</h1>
      <p>Choose a card to see its local preview.</p>
      <div className="preview-cards">
        {tools.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={selected === item.id}
            onClick={() => setSelected(item.id)}
          >
            <span className="preview-card-icon">
              <ShellIcon name="store" />
            </span>
            <h2>{item.title}</h2>
            <p>{item.description}</p>
            <span>Preview tool</span>
          </button>
        ))}
      </div>
      <div className="preview-detail">
        <span className="preview-eyebrow">Local tool preview</span>
        <h2>{tool?.title}</h2>
        <pre>{tool?.example}</pre>
      </div>
    </>
  );
}

function DemoFiles() {
  const [selected, setSelected] = useState('brief');
  const files = [
    {
      id: 'brief',
      title: 'Design brief.txt',
      meta: 'Design studio · 1 KB',
      content:
        'Design brief\n\nGive the conversation more space.\nKeep navigation quiet and close at hand.\nMake the panel easy to hide and bring back.',
    },
    {
      id: 'weekend',
      title: 'Weekend plan.txt',
      meta: 'Saturday crew · 1 KB',
      content:
        'Weekend plan\n\n09:00 Meet at the park\n09:30 Walk by the lake\n11:00 Coffee with the crew',
    },
    {
      id: 'notes',
      title: 'Research notes.txt',
      meta: 'Research notes · 1 KB',
      content:
        'Research notes\n\nSmall frames can still hold useful context.\nKeep a visible path back to the conversation.\nLet appearance follow the person using the app.',
    },
  ];
  const file = files.find((item) => item.id === selected);
  return (
    <>
      <span className="preview-eyebrow">Sample files</span>
      <h1>Shared files</h1>
      <p>Select a file to read its sample contents.</p>
      <div className="preview-results">
        {files.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={selected === item.id}
            onClick={() => setSelected(item.id)}
          >
            <strong>
              <ShellIcon name="files" />
              {item.title}
            </strong>
            <span>{item.meta}</span>
          </button>
        ))}
      </div>
      <div className="preview-detail">
        <span className="preview-eyebrow">Local file preview</span>
        <h2>{file?.title}</h2>
        <pre>{file?.content}</pre>
      </div>
    </>
  );
}

function DemoAgents() {
  const [selected, setSelected] = useState('research');
  const agents = [
    {
      id: 'research',
      title: 'Research partner',
      description: 'Organize questions, references, and reading notes.',
      focus: 'A clear question, useful sources, and notes you can return to.',
    },
    {
      id: 'planner',
      title: 'Planning partner',
      description: 'Turn a busy week into a plan you can follow.',
      focus: 'Your priorities, available time, and the next small step.',
    },
  ];
  const agent = agents.find((item) => item.id === selected);
  return (
    <>
      <span className="preview-eyebrow">Sample agent profiles</span>
      <h1>Your helpers</h1>
      <p>Explore a profile. This preview does not start an agent run.</p>
      <div className="preview-cards">
        {agents.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={selected === item.id}
            onClick={() => setSelected(item.id)}
          >
            <span className="preview-card-icon">
              <ShellIcon name="agents" />
            </span>
            <h2>{item.title}</h2>
            <p>{item.description}</p>
            <span>View profile</span>
          </button>
        ))}
      </div>
      <div className="preview-detail">
        <span className="preview-eyebrow">Profile preview</span>
        <h2>{agent?.title}</h2>
        <p>{agent?.focus}</p>
      </div>
    </>
  );
}

function DemoContext() {
  const [name, setName] = useState('Marina');
  const [notes, setNotes] = useState('I like concise answers and a little space to think.');
  return (
    <>
      <span className="preview-eyebrow">Local context draft</span>
      <h1>A little context about you</h1>
      <p>Edit these sample details in this view. Nothing is sent or saved to an account.</p>
      <div className="preview-context-form">
        <label>
          Your name
          <input value={name} onChange={(event) => setName(event.currentTarget.value)} />
        </label>
        <label>
          What should your helpers know?
          <textarea
            rows={4}
            value={notes}
            onChange={(event) => setNotes(event.currentTarget.value)}
          />
        </label>
      </div>
      <div className="preview-detail">
        <span className="preview-eyebrow">Draft preview</span>
        <h2>{name || 'Your name'}</h2>
        <p>{notes || 'Add a little context above.'}</p>
      </div>
    </>
  );
}

function DestinationContent({
  destination,
  onSelectChat,
  onSearch,
}: {
  destination: Exclude<Destination, 'chats'>;
  onSelectChat: (id: string) => void;
  onSearch: (query: string) => void;
}) {
  switch (destination) {
    case 'activity':
      return <SampleActivity onSelectChat={onSelectChat} />;
    case 'store':
      return <DemoStore />;
    case 'files':
      return <DemoFiles />;
    case 'agents':
      return <DemoAgents />;
    case 'context':
      return <DemoContext />;
    case 'search':
      return (
        <>
          <span className="preview-eyebrow">Search the sample chats</span>
          <h1>Find what you need</h1>
          <p>Use the search field in Chats, or start with one of these suggestions.</p>
          <div className="preview-search-suggestions">
            {['Design', 'Marina', 'Saturday'].map((value) => (
              <button key={value} type="button" onClick={() => onSearch(value)}>
                <ShellIcon name="search" />
                {value}
              </button>
            ))}
          </div>
        </>
      );
    default: {
      const exhaustive: never = destination;
      return exhaustive;
    }
  }
}

function Preview() {
  const [destination, setDestination] = useState<Destination>('chats');
  const [chats, setChats] = useState(sampleChats);
  const [selectedId, setSelectedId] = useState('studio');
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<InboxFilter>('all');
  const [theme, setTheme] = useState<ShellTheme>(readTheme);
  const selected = chats.find((chat) => chat.id === selectedId);
  const filtered = filterInbox(chats, filter, query);

  function selectChat(id: string) {
    if (filter !== 'all' && chats.find((chat) => chat.id === id)?.kind !== filter) setFilter('all');
    setSelectedId(id);
    setDestination('chats');
    setQuery('');
  }
  function search(value: string) {
    setQuery(value);
    setDestination('search');
  }
  function changeTheme(value: ShellTheme) {
    setTheme(value);
    try {
      localStorage.setItem(preferenceKey, value);
    } catch {
      /* Persistence is optional. */
    }
  }
  function newChat() {
    const id = `draft-${chats.length}`;
    setChats((current) => [
      {
        id,
        kind: 'direct',
        title: 'New local draft',
        preview: 'Created in this preview',
        avatar: '+',
      },
      ...current,
    ]);
    selectChat(id);
  }

  return (
    <ZoenShell
      navigation={navigation}
      activeNavigationId={destination}
      onNavigate={(id) => {
        setDestination(id);
        setQuery('');
      }}
      chats={filtered.map((chat) => ({
        ...chat,
        preview: `${conversationLabels[chat.kind]} · ${chat.preview ?? ''}`,
      }))}
      activeChatId={selectedId}
      onSelectChat={selectChat}
      searchQuery={query}
      onSearchChange={search}
      searchNavigationId="search"
      globalSearchShortcut
      theme={theme}
      onThemeChange={changeTheme}
      title={destination === 'chats' ? (selected?.title ?? 'Chats') : ''}
      onNewChat={newChat}
      sidebarFilters={<InboxFilters selected={filter} onChange={setFilter} />}
      headerActions={
        destination === 'chats' ? (
          <span className="preview-label" aria-label="Local shell preview">
            Preview
          </span>
        ) : undefined
      }
      sidebarFooter={
        <>
          <strong>Local preview</strong>
          <br />
          Sample chats. No messages are sent.
        </>
      }
      emptySidebar="No sample chats match these filters."
    >
      {query.trim() ? (
        <section className="preview-page">
          <span className="preview-eyebrow">Search the sample chats</span>
          <h1>Results for &ldquo;{query.trim()}&rdquo;</h1>
          <p>
            {filtered.length} {filtered.length === 1 ? 'conversation' : 'conversations'}
            {filter === 'all'
              ? ' across all chats'
              : ` in ${conversationLabels[filter].toLowerCase()} chats`}
          </p>
          <div className="preview-results">
            {filtered.map((chat) => (
              <button key={chat.id} type="button" onClick={() => selectChat(chat.id)}>
                <strong>{chat.title}</strong>
                <span>
                  {conversationLabels[chat.kind]} · {chat.preview}
                </span>
              </button>
            ))}
          </div>
          {filtered.length === 0 && <p>Try another query or choose All in the Chats filters.</p>}
        </section>
      ) : destination === 'chats' ? (
        <section className="preview-conversation">
          <h1 className="zs-sr-only">{selected?.title ?? 'Choose a chat'}</h1>
          {selected?.id.startsWith('draft-') ? (
            <div className="preview-draft">
              <ShellIcon name="compose" />
              <h2>Your local draft</h2>
              <p>This draft exists only in this preview tab.</p>
            </div>
          ) : (
            <div className="preview-messages">
              <div className="preview-date">
                Today · Sample {selected ? conversationLabels[selected.kind].toLowerCase() : ''}{' '}
                conversation
              </div>
              {selected?.kind === 'community' && <CommunityResources />}
              {sampleMessages.map((message) => (
                <div
                  key={message.time}
                  className={`preview-message${message.author === 'You' ? ' is-own' : ''}`}
                >
                  {message.author !== 'You' && (
                    <span className="preview-author">{message.author}</span>
                  )}
                  <p>{message.text}</p>
                  <time>{message.time}</time>
                </div>
              ))}
              <div className="preview-message">
                <span className="preview-author">Marina</span>
                <p>{selected?.preview ?? 'Choose a chat from the sidebar.'}</p>
                <time>10:42</time>
              </div>
              <div className="preview-note">
                These are sample messages for reviewing the layout.
              </div>
            </div>
          )}
        </section>
      ) : (
        <section className="preview-page">
          <DestinationContent
            destination={destination}
            onSelectChat={selectChat}
            onSearch={search}
          />
        </section>
      )}
    </ZoenShell>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('The shell preview needs a root element.');
createRoot(root).render(
  <StrictMode>
    <Preview />
  </StrictMode>,
);
