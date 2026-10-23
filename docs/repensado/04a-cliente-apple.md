[Índice](../repensado.md) · [Anterior](04-arquitetura-tecnologias.md) · [Próximo](05-escala-seguranca.md)

## 4A. Cliente Apple nativo (iOS + macOS)

**Decisão:** 100% Swift 6 + SwiftUI, com alvo mínimo **iOS 26 e macOS 26**. São eles que trazem o Liquid Glass completo e os Foundation Models no aparelho. O núcleo Rust fica por baixo via **UniFFI** (Swift Package gerado). Não usamos Catalyst nem Flutter.

### Divisão de responsabilidades
| Fica em Rust (`client-core`) | Fica em Swift |
|---|---|
| Primitivos (Identidade, Espaço, Membro, Item, Concessão), log assinado e validação | Toda a UI, a navegação e o Liquid Glass |
| MLS (OpenMLS), cifra de mídia e verificação de key transparency | Secure Enclave (CryptoKit), passkeys (AuthenticationServices + PRF) e Keychain |
| Sync (WebSocket, outbox, cursores) e armazenamento SQLite/SQLCipher | CallKit, PushKit, LiveKit Swift SDK e áudio |
| CRDT (Loro): versões, bifurcações e diff | Editor rico: `TextEditor` + `AttributedString` (iOS 26), ligado ao Loro |
| Avaliador de Concessões (Cedar) e medição de orçamento local | StoreKit 2, App Intents, Spotlight, WidgetKit/ActivityKit e SpeechAnalyzer |
| Índice de busca FTS5 + sqlite-vec | Foundation Models (resumos e classificação no aparelho) |

**Fluxo de dados:** o núcleo é a única fonte de verdade (SQLite num **App Group** compartilhado com as extensões). Ele emite fluxos de mudança (callbacks UniFFI viram `AsyncStream`) para stores `@Observable`. Não usamos Core Data nem SwiftData, porque duas fontes de verdade são o caminho certo para bugs de sync.

### Estrutura do app
- **Shells por plataforma:** `ZoeniOS` (TabView com 4 abas + aba de busca) e `ZoenMac` (NavigationSplitView em 3 colunas: Espaços | conversa | Item/inspetor).
- **Código compartilhado:** cerca de 85% da UI em pacotes SwiftUI comuns.
- **Extensões:**
  - **Notification Service:** decifra pushes E2EE.
  - **Share:** "Mandar para o Zoen / para um Espaço".
  - **Widgets + Live Activities.**
  - **App Intents.**
  - **App Clip:** o convidado abre o link no iPhone sem instalar. Atenção ao tamanho do App Clip com o núcleo Rust *(a verificar)*.

### Componentes de Liquid Glass (sistema primeiro, custom só onde faz sentido)
- **Sistema:** `TabView` com `Tab(role: .search)` e `.tabBarMinimizeBehavior(.onScrollDown)`; toolbars e sheets de vidro; `.buttonStyle(.glass / .glassProminent)`; `GlassEffectContainer` + `.glassEffect()` para agrupar controles que se fundem.
- **Custom (DesignSystem):**
  - `Composer`, uma cápsula de vidro com anexos e voz.
  - `RequestCard`, o pedido de agente, flutuante.
  - `UndoToast`, que nasce do composer.
  - `AgentAvatar` (squircle + glifo + mini-avatar do dono + anéis de estado) e `BudgetRing`.
  - `PrivacyPill`, com o selo 🔒/🛡️/🌐 no cabeçalho.
- **Regra:** vidro só na camada de controle; o conteúdo é sólido. "Reduzir transparência" e "Aumentar contraste" são respeitados automaticamente.

### Integrações de sistema (o fosso de qualidade)
| Integração | Uso |
|---|---|
| **Notification Service Extension** | O push chega com o evento cifrado e a NSE decifra com o núcleo em "modo leve" (limite de ~24 MB de memória). Nome e avatar via Communication Notifications (`INSendMessageIntent`). Se falhar, mostra "Nova mensagem". |
| **CallKit + PushKit** | Chamadas "Ao vivo" tocam como chamada nativa e funcionam na tela bloqueada. |
| **Secure Enclave** | Chave de aparelho **P-256** não exportável (o Enclave só faz P-256). Ela assina o login no relay e a credencial do aparelho. |
| **Passkeys + PRF** | A passkey desbloqueia o backup da **chave raiz** (Ed25519, em software), cifrada com a chave derivada pelo PRF e sincronizada pelo iCloud Keychain. |
| **App Intents / Atalhos / Siri** | "Perguntar ao Zoen", "Criar tarefa em…", "Resumir conversa", "Mandar para Espaço". As entidades `SpaceEntity`, `ItemEntity` e `AgentEntity` ficam indexáveis e disponíveis para o Apple Intelligence. |
| **Live Activities / Dynamic Island** | Uma tarefa de agente em andamento mostra progresso, custo e o botão "Aprovar" (App Intent, com Face ID). |
| **Widgets / Controles** | "O que precisa de você" (pedidos pendentes), conversa fixada e o controle "Pergunte" no Centro de Controle. |
| **Share Extension** | Qualquer coisa vira Item num Espaço ou um pedido ao agente. |
| **Spotlight** | Índice local (Core Spotlight) com títulos decifrados **só no aparelho**. |
| **StoreKit 2** | Assinaturas (Plus/Max) e compras de criadores; no Brasil, ao lado do Pix via processador alternativo (D12). |
| **Foundation Models** | Resumo, título e classificação no aparelho. É também a base da opção "só agentes locais". |

### Especificidades do Mac
- Janela com 3 colunas e sidebar de vidro, com várias janelas (abrir um Item na própria janela).
- Paleta de comandos com ⌘K, atalhos para tudo e arrastar/soltar arquivos.
- **Menu bar extra** "Pergunte" com atalho global; "Ao vivo" em janela flutuante (PiP).
- Arquivos grandes e pastas como Itens, com sincronização em segundo plano.

### Web: só o visualizador de convite
Uma página leve (Cloudflare Pages + `web-core`) para quem recebe o link e ainda não tem o app. Ela permite ver e responder no Espaço convidado e editar Itens de forma simples, depois convida a instalar. No iPhone, o link abre o **App Clip**; no Android e no desktop, abre a web. O app web completo não está no plano antes de 2027.

---

