[Índice](../repensado.md) · [Anterior](06-comunidades.md) · [Próximo](08-signal.md)

## 7. Protocolos: existe algum que resolva tudo?

**Resposta curta: nenhum resolve tudo.** O desenho certo é em camadas, cada uma com o melhor protocolo para o seu problema.

| | Chat privado E2EE | Comunidades grandes | Feed público | Identidade/portabilidade | Agentes | Escala | Maturidade | Licença |
|---|---|---|---|---|---|---|---|---|
| **MLS (RFC 9420)** | ✅ Excelente, com PCS em grupo e O(log n) | ⚠️ Mira milhares; caro perto de 50–100 mil | — | Credenciais plugáveis (traga sua identidade) | ✅ Agente = membro com folha | ✅ | Padrão IETF (2023). OpenMLS; em produção no Discord DAVE, Webex e Wire | OpenMLS **MIT** |
| **Signal Protocol** | ✅ Estado da arte no 1:1 (PQXDH 2023, SPQR/Triple Ratchet 2025) | ⚠️ Sender Keys, grupos até ~1.000 | — | Conta central (telefone/username) | ❌ Não pensado | ✅ Bilhões (WhatsApp) | Muito maduro | libsignal **AGPL-3.0** |
| **Matrix** | ✅ Olm/Megolm. ⚠️ MLS ainda em proposta (MSC4256, 2026) | ⚠️ Federação de salas; salas enormes são pesadas | — | ⚠️ ID preso ao servidor (`@user:server`) | Bots como usuários | ⚠️ Federação custosa | Maduro (Element) | Spec Apache-2.0; Synapse AGPL |
| **Nostr** | ⚠️ NIP-17 (gift wrap, até ~10 pessoas) · **Marmot** = MLS sobre Nostr (experimental) | ⚠️ NIP-29: grupos no relay, **sem E2EE** | ✅ Simples, com zaps (Lightning) | ✅ Chave = identidade (perder a chave = perder tudo) | Agente = chave | ⚠️ Relays sem garantia de entrega e ordem | Specs informais, ecossistema pequeno | Diversas abertas |
| **AT Protocol (Bluesky)** | ❌ Sem E2EE nativo (o Germ e o XMTP fazem MLS por fora) | — | ✅ **O melhor desenho:** feeds customizados, labelers (moderação componível), firehose | ✅ DID + PDS migrável | Bots como contas | ✅ Rede de dezenas de milhões *(ordem de grandeza)* | Em maturação, com adoção real | Abertas (MIT/Apache) |
| **XMTP** | ✅ MLS | ✅ Grupos MLS | — | Carteiras e identidades ATProto | ✅ Foco declarado em agentes | ⚠️ Ordem de commits via blockchain L3 | Mainnet prevista para 2026 | MIT |
| **ActivityPub** | ❌ Sem E2EE padrão | Instâncias federadas | ✅ Federado (Mastodon, Threads) | ⚠️ Identidade presa à instância | Bots | Desigual | Recomendação W3C (2018) | W3C |

### Veredito para o nosso desenho em camadas
1. **Espaços privados → MLS via OpenMLS.** ✅ Confirmado. Não usamos Signal Protocol (AGPL e grupos mais fracos) nem Matrix (o MLS não foi entregue e a federação é complexa).
2. **Transporte e escala → protocolo próprio simples ("Zoen Sync").** ✅ Confirmado. São eventos assinados em protobuf sobre WebSocket, sequenciados por Espaço; as mensagens MLS viajam dentro do envelope. O formato pode seguir a forma de payload do Marmot onde for barato, para deixar a porta aberta.
3. **Camada pública → AT Protocol (fase 4)**, com Nostr como ponte opcional.
   - ⚠️ **Correção da visão "Nostr ou ATProto":** a pesquisa favorece o **ATProto** para o feed público. Ele tem feeds customizados, moderação componível (labelers), identidade migrável (DID/PDS) e público real. O Nostr ganha em simplicidade e em identidade por chave, mas perde em moderação e descoberta.
   - Fica assim: **ATProto para presença pública; Nostr/Marmot como "saída de soberania"** para grupos privados sem servidor, quando o Marmot amadurecer.
   - **XMTP:** não adotamos (a dependência de blockchain para ordem não combina com o produto).
   - **ActivityPub:** só via ponte de terceiros, se pedirem.

---

