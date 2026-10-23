[Índice](../repensado.md) · [Anterior](09-roadmap.md) · [Próximo](11-inconsistencias.md)

## 10. O que eu construiria primeiro (12 semanas)

### Marcos
| Semanas | Marco | Entregável |
|---|---|---|
| 1–2 | **Fundação** | Workspace Rust (tipos, cripto, log, store) com testes de propriedade; relay axum + WebSocket + Postgres; UniFFI gerando o `ClientCore` Swift; app iOS/Mac com a cápsula de vidro e Conversas lendo do núcleo; CI (GitHub Actions + Xcode Cloud) |
| 3–4 | **Agente na conversa** | Runtime de agentes v0 (gateway de modelos com streaming, Zoen como Membro que usa a mesma API); Itens com Loro e cartão vivo; editor nativo `AttributedString` ↔ Loro; Desfazer; offline e sync. **Demo da semana 4** |
| 5–6 | **Pessoas** | Link de convite + web de convite (`web-core`) + App Clip; grupos; push APNs; passkey progressiva com backup via PRF; app Mac com 3 colunas |
| 7–8 | **Confiança e privacidade** | Níveis de confiança, orçamento medido no gateway, Atividade com pedidos em lote, recibos; **MLS (OpenMLS)** nos Espaços privados; a NSE decifra; Live Activity de tarefa. **Demo da semana 8** |
| 9–10 | **Acabamento nativo** | Busca FTS5 + Spotlight, Share Extension, App Intents/Atalhos, widgets, Versões/Comparar básicos, key transparency v0 (AKD), telemetria (OTel, Sentry, PostHog sem conteúdo) |
| 11–12 | **Beta** | StoreKit 2 + Pix via processador alternativo, documentos de LGPD, revisão de segurança externa focada em cripto, teste de carga (100 mil conexões simuladas por gateway), **TestFlight com 300 pessoas** |

### Workspace Rust (`zoen-native/`)
```
zoen-native/
├─ crates/
│  ├─ types      # Identidade, Espaço, Membro, Item, Concessão, Evento (prost/protobuf)
│  ├─ crypto     # chaves, assinaturas, embrulho por PRF, hashing, padding
│  ├─ mls        # OpenMLS ↔ Espaço (épocas, folhas de aparelho e de agente)
│  ├─ event-log        # log append-only, cadeia de hash, validação, compactação
│  ├─ grants     # políticas Cedar, avaliação, orçamento
│  ├─ store      # SQLite/SQLCipher, migrações, FTS5, sqlite-vec
│  ├─ crdt       # Loro: Itens, versões, bifurcar, diff, juntar
│  ├─ sync       # cliente: outbox, cursores, WebSocket, retomada
│  ├─ client-core        # UniFFI → Swift Package ClientCore (e Kotlin no port Android de 2026)
│  └─ web-core       # web de convite
├─ services/
│  ├─ relay           # gateway WebSocket + dono de Espaço (axum/tokio)
│  ├─ push            # APNs/FCM/VoIP
│  ├─ agents          # runtime (workflows), MCP, gateway de modelos, medição
│  ├─ media           # pré-assinatura, cotas
│  └─ directory       # key transparency (AKD), usernames
└─ tools/  loadgen · xtask · spec (protocolo Zoen Sync, aberto)
```

### Módulos SwiftUI (`apple/`)
```
apple/
├─ Apps/      ZoeniOS · ZoenMac · ZoenClip
├─ Extensions/ NotificationService · Share · Widgets(+LiveActivities) · Intents
└─ Packages/
   ├─ ClientCore        # bindings UniFFI + atores e AsyncStreams
   ├─ AppModel       # stores @Observable sobre o núcleo
   ├─ DesignSystem    # GlassTabBar, Composer, AgentAvatar, RequestCard, UndoToast, BudgetRing, PrivacyPill
   ├─ Features/       Conversations · Space · Item · Activity · You · Onboarding · Invite
   ├─ System/         Passkeys · SecureEnclave · Push · Intents · Spotlight · LiveActivities
   └─ Commerce        # StoreKit 2 + processador alternativo (BR)
```

### As 5 primeiras telas
1. **Conversas:** lista única, cápsula de vidro, filtros, pedidos pendentes no topo e o anel de "trabalhando" nos agentes.
2. **Espaço:** conversa com pessoas e agentes; streaming; o Item nasce da mensagem; Desfazer; pílula de privacidade; Ao vivo (desabilitado até a fase 2).
3. **Item:** documento nativo editável com presença do agente, Versões e "Bifurcar".
4. **Atividade:** pedidos em lote com custo e Face ID, tarefas ao vivo e menções.
5. **Convite:** folha nativa com o link e a experiência do convidado (App Clip ou web) já dentro do Espaço.

### "Demo que impressiona"
- **Semana 4:** no iPhone, sem conta, digo por voz *"planeja meu fim de semana em Paraty com a Marina, até R$ 1.500"*.
  - Em menos de **10 s** o Zoen cria um plano vivo. Edito uma linha e ele recalcula; toco em Desfazer.
  - Em **modo avião** tudo continua funcionando e sincroniza ao voltar. O mesmo Espaço aparece no **Mac** em tempo real.
  - Meta: primeiro Item p50 < 10 s, nenhum spinner acima de 1 s, 60 fps no scroll.
- **Semana 8:** convido a Marina por link. Ela abre num **Android pelo navegador** e no iPhone pelo **App Clip**, já vendo o plano.
  - O agente Financeiro pede para "reservar a pousada (R$ 420)". O pedido aparece em lote no Atividade e numa **Live Activity**, e aprovo com Face ID.
  - O push chega **cifrado e decifrado na NSE**, e o cabeçalho mostra 🔒 Ponta a ponta com o agente declarado.
  - Meta: entrega p99 < 250 ms em São Paulo e 100 mil conexões simuladas por gateway.

---

