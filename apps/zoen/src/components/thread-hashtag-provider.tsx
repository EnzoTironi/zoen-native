import { type ReactNode } from 'react';
import { randomUUID } from 'expo-crypto';
import { router } from 'expo-router';
import { useClient } from '../core/provider';
import { searchLaunches } from '../core/search-launch';
import { useConversationSearch } from '../core/use-conversation-search';
import { ConversationHashtagProvider } from './conversation-hashtag-provider';
export function ThreadHashtagProvider({ space, children }: { space: string; children: ReactNode }) {
  const { store } = useClient();
  const { search } = useConversationSearch(space);
  return (
    <ConversationHashtagProvider
      search={search}
      onActivate={() => {
        const ticket = randomUUID();
        if (!searchLaunches.prepare(ticket, space, search.getSnapshot().query, store)) return;
        try {
          router.push({ pathname: '/chat/[id]', params: { id: space, searchTicket: ticket } });
        } catch (reason) {
          searchLaunches.release(ticket);
          throw reason;
        }
      }}
    >
      {children}
    </ConversationHashtagProvider>
  );
}
