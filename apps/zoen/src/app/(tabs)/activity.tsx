import { useState } from 'react';
import { router } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { messageText, money } from '../../core/presentation';
import { useTheme } from '../../theme';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { RequestCard } from '../../agents/request-card';

type Section = 'approvals' | 'mentions' | 'tasks' | 'resolved';
export default function Activity() {
  const { snapshot } = useClient();
  const { colors } = useTheme();
  const [section, setSection] = useState<Section>('approvals');
  const requestRead = useCoreQuery({ op: 'requests' });
  const mentions = useCoreQuery(section === 'mentions' ? { op: 'mentions' } : null);
  const items = useCoreQuery(section === 'tasks' ? { op: 'items' } : null);
  const requests = requestRead.data ?? snapshot?.requests ?? [];
  const pending = requests.filter(
    (request) => request.status === 'Pending' || request.status === 'Stale',
  );
  const resolved = requests.filter(
    (request) => request.status === 'Approved' || request.status === 'Denied',
  );
  const tasks =
    items.data?.filter(
      (item) =>
        item.kind_id === 'task' ||
        item.plan?.sections.some((section) => section.lines.some((line) => !line.done)),
    ) ?? [];
  const choices: { id: Section; title: string; count?: number }[] = [
    { id: 'approvals', title: 'Aprovações', count: pending.length },
    { id: 'mentions', title: 'Menções', count: mentions.data?.length },
    { id: 'tasks', title: 'Tarefas', count: items.data ? tasks.length : undefined },
    { id: 'resolved', title: 'Decididos', count: resolved.length },
  ];
  const error =
    section === 'mentions' ? mentions.error : section === 'tasks' ? items.error : requestRead.error;
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        flexGrow: 1,
        padding: 24,
        paddingBottom: 48,
        gap: 18,
        backgroundColor: colors.background,
        maxWidth: 900,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Text
        accessibilityRole="header"
        style={{ fontSize: 34, fontWeight: '700', letterSpacing: -1, color: colors.text }}
      >
        Atividade
      </Text>
      <Text style={{ fontSize: 16, lineHeight: 23, color: colors.secondary }}>
        Revise os pedidos e acompanhe suas conversas.
      </Text>
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
        {choices.map((choice) => (
          <Pressable
            key={choice.id}
            accessibilityRole="tab"
            accessibilityState={{ selected: section === choice.id }}
            onPress={() => setSection(choice.id)}
            style={{
              paddingHorizontal: 16,
              minHeight: 44,
              justifyContent: 'center',
              backgroundColor: section === choice.id ? colors.accent : colors.surface,
              borderRadius: 22,
            }}
          >
            <Text
              style={{
                color: section === choice.id ? colors.onAccent : colors.text,
                fontWeight: '600',
              }}
            >
              {choice.title}
              {choice.count === undefined ? '' : ` · ${choice.count}`}
            </Text>
          </Pressable>
        ))}
      </View>
      {error ? (
        <Surface>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error}
          </Text>
          <NativeButton
            secondary
            label="Tentar novamente"
            onPress={() => {
              requestRead.retry();
              mentions.retry();
              items.retry();
            }}
          />
        </Surface>
      ) : null}
      {section === 'approvals' ? (
        <>
          {snapshot?.agents
            .filter((agent) => agent.near_limit && agent.persona.is_mine)
            .map((agent) => (
              <Surface key={agent.persona.id}>
                <Text style={{ color: colors.text, fontWeight: '600' }}>
                  {agent.persona.name} está perto do limite
                </Text>
                {agent.budget_spent_cents !== null && agent.budget_limit_cents !== null ? (
                  <Text style={{ color: colors.secondary }}>
                    {money(agent.budget_spent_cents)} de {money(agent.budget_limit_cents)}
                  </Text>
                ) : null}
                <NativeButton
                  secondary
                  label="Ver orçamento"
                  onPress={() =>
                    router.push({ pathname: '/agent/[id]', params: { id: agent.persona.id } })
                  }
                />
              </Surface>
            ))}
          {!requestRead.data && !snapshot ? (
            <Loading />
          ) : !pending.length && !error ? (
            <EmptyState
              title="Tudo em dia"
              detail="Pedidos dos seus agentes aparecem aqui, com o contexto de cada decisão."
              icon={{ ios: 'checkmark.circle', android: 'check_circle', web: 'check_circle' }}
            />
          ) : (
            pending.map((request) => (
              <RequestCard
                key={request.id}
                request={request}
                compact
                onResolved={requestRead.retry}
              />
            ))
          )}
        </>
      ) : section === 'resolved' ? (
        !resolved.length && !error ? (
          <EmptyState
            title="Nenhuma decisão ainda"
            detail="Suas aprovações e recusas aparecerão aqui."
          />
        ) : (
          resolved.map((request) => <RequestCard key={request.id} request={request} compact />)
        )
      ) : section === 'mentions' ? (
        !mentions.data ? (
          error ? null : (
            <Loading />
          )
        ) : !mentions.data.length ? (
          <EmptyState
            title="Ninguém mencionou você"
            detail="Quando alguém chamar você numa conversa, a mensagem aparecerá aqui."
          />
        ) : (
          mentions.data.map((mention) => (
            <Pressable
              key={`${mention.space_id}:${mention.entry.id}`}
              accessibilityRole="button"
              onPress={() =>
                router.push(
                  mention.entry.in_thread
                    ? {
                        pathname: '/thread/[space]/[root]',
                        params: { space: mention.space_id, root: mention.entry.in_thread },
                      }
                    : {
                        pathname: '/chat/[id]',
                        params: { id: mention.space_id, jump: mention.entry.id },
                      },
                )
              }
              style={{ padding: 20, gap: 8, backgroundColor: colors.surface, borderRadius: 20 }}
            >
              <Text style={{ color: colors.accent, fontWeight: '600' }}>
                {mention.entry.author.name} · {mention.space_title}
              </Text>
              <Text
                selectable
                numberOfLines={4}
                style={{ color: colors.text, fontSize: 17, lineHeight: 24 }}
              >
                {messageText(mention.entry)}
              </Text>
            </Pressable>
          ))
        )
      ) : !items.data ? (
        error ? null : (
          <Loading />
        )
      ) : !tasks.length ? (
        <EmptyState
          title="Nenhuma tarefa em aberto"
          detail="Tarefas e planos com linhas pendentes aparecem aqui."
        />
      ) : (
        tasks.map((item) => (
          <Pressable
            key={item.id}
            accessibilityRole="button"
            onPress={() => router.push({ pathname: '/item/[id]', params: { id: item.id } })}
            style={{ padding: 20, gap: 8, backgroundColor: colors.surface, borderRadius: 20 }}
          >
            <Text style={{ color: colors.accent, fontSize: 12 }}>
              {item.space_title} · {item.kind_label}
            </Text>
            <Text style={{ color: colors.text, fontSize: 21, fontWeight: '600' }}>
              {item.title}
            </Text>
          </Pressable>
        ))
      )}
      <NativeButton secondary label="Explorar miniapps" onPress={() => router.push('/miniapps')} />
    </ScrollView>
  );
}
