[Índice](../repensado.md) · [Anterior](04a-cliente-apple.md) · [Próximo](06-comunidades.md)

## 5. Arquitetura de escala e segurança

![Arquitetura de escala](../arquitetura-escala.png)

### 5.1 Escalar o chat

**Princípio:** a unidade de ordem, de shard e de privacidade é o **Espaço**. Não existe ordem global, igual ao Slack ("não há ordem global de eventos fora de um canal").

**Caminho de uma mensagem:**
1. O cliente assina e (se privado) cifra o evento com MLS. Ele vai para a outbox do SQLite e é enviado por WebSocket.
2. O **gateway** (sem estado de negócio) autentica o aparelho (assinatura P-256 + App Attest) e encaminha ao **dono do Espaço**.
3. O **dono do Espaço** (escolhido por hash consistente de `space_id`) checa as Concessões, atribui `seq` e grava.
4. Ele publica no NATS `space.<id>`.
5. Cada gateway com membros conectados recebe **uma cópia** e entrega localmente. Quem está offline recebe push e, ao reconectar, busca pelo cursor.

**Ordem e sequência**
- **v1:** a sequência mora no Postgres, numa linha por Espaço (`UPDATE … RETURNING seq` + `INSERT` na mesma transação). Aguenta milhares de eventos/s por Espaço.
- **Escala:** dono em memória por hash consistente, com batching de commits (os Channel Servers do Slack fazem exatamente isso, em Java, ~16 mi de canais por host no pico).
- **Estágio 2:** **versionstamps do FoundationDB** dão ordem transacional por Espaço sem serviço dedicado.

**Gateways e registro de conexões**
- Gateways WebSocket são stateless para o negócio e guardam só as conexões. Meta por nó (16 vCPU/32 GB, tokio): **100–250 mil conexões**, a validar em teste de carga. Para comparação, o WhatsApp rodava ~1 mi de conexões por servidor Erlang em 2014, depois de demonstrar 2 mi em 2012.
- O registro de quem está onde fica no **NATS KV**, com TTL e heartbeat. Ele é usado para push e presença, não para rotear mensagens.
- A **entrega é por interesse:** o gateway assina `space.<id>` com refcount dos usuários conectados. O NATS entrega uma vez por gateway, e o gateway multiplica localmente.

**Estratégias de fan-out**
| Tipo de Espaço | Estratégia |
|---|---|
| DM e grupos ≤ 1.000 (privados) | **Fan-out na escrita só de ponteiros:** atualiza `inbox(user) → (space, seq, unread)` de cada membro, enquanto o conteúdo fica uma vez só no log. Push para os offline. |
| Comunidades grandes (10 mil–1 mi) | **Fan-out na leitura:** nada de ponteiro por membro a cada mensagem. O cliente busca a "cabeça" dos canais abertos, e as notificações só vêm de menções, respostas e canais seguidos. |
| Espaços quentes (uma live com 100 mil) | Cache da cauda do canal no gateway (coalescência de leituras, como os serviços Rust do Discord), entregas em lote a cada 100–250 ms, presença só agregada ("12,4 mil aqui") e modo lento por política. |

**Presença e digitando**
- São eventos **efêmeros** (nunca vão para disco), enviados só para os Espaços visíveis na tela. O Slack faz o mesmo e só envia presença dos usuários visíveis.
- "Digitando" tem throttle de 1 a cada 3 s. Em comunidades grandes, aparece só como contagem.

**NATS JetStream**
- **Core NATS:** entrega ao vivo para os gateways. Não precisa persistir, porque a fonte da verdade é o log.
- **JetStream:** filas duráveis e idempotentes (push, gatilhos de agentes, mídia, indexação, webhooks). **KV:** registro de conexões e presença.
- **Superclusters:** multi-região mais tarde.

**Pipeline de push**
- O relay coloca o evento na fila JetStream `push`. Os workers em Rust (APNs HTTP/2, FCM v1) aplicam mudo, colapso e prioridade.
- O payload é o evento cifrado se couber em 4 KB; senão, vai só "acorde + id". A NSE decifra.
- Chamadas usam **VoIP push** (PushKit) e CallKit.

**Pipeline de mídia**
- Cifra AES-GCM no cliente, PUT pré-assinado no **R2** (multipart e retomável acima de 100 MB) e evento com ponteiro + chave + hash + blurhash.
- Miniaturas e transcodificação no aparelho. Mídia pública (🌐) é processada no servidor (Cloudflare Images/Stream) e passa por hash de CSAM.

**Armazenamento**
- **Estágio 1:** Postgres com a tabela `events(space_id, seq)`, particionada por hash de `space_id` (128 partições), com PITR e réplicas.
- **Estágio 2 (gatilhos D9):** **FoundationDB** para o log e os índices quentes, com segmentos frios compactados no object storage.
- **Por que FDB:** ordem transacional e versionstamps, e o CloudKit da Apple roda sobre o FDB Record Layer. O Signal está migrando seu armazenamento de mensagens para FDB em 2026, segundo os commits públicos do Signal-Server.
- **Alternativa:** ScyllaDB, o caminho do Discord. Eles migraram trilhões de mensagens de **177 nós Cassandra para 72 nós ScyllaDB**, com p99 de leitura caindo de 40–125 ms para ~15 ms e inserção em ~5 ms.

**Multi-região**
- **v1:** São Paulo (Fly GRU ou AWS sa-east-1), multi-AZ, com Postgres HA e backups com PITR.
- **Fase 2:** gateways de borda em us-east e eu-west (latência), mantendo **região-casa por Espaço** para o sequenciamento.
- **Fase 3+:** Espaços com casa em várias regiões (residência de dados) e NATS supercluster.

**Capacidade (estimativa)**

Premissas:
- DAU = 50% do MAU, com 40 eventos escritos por DAU por dia. O WhatsApp de 2014 fazia ~41 mensagens recebidas por MAU por dia (19 bi/dia para 465 mi).
- Pico = 3× a média e fan-out médio de 6.
- ~1 KB por evento cifrado.
- Conexões simultâneas = 30% do MAU (WhatsApp 2014: 147 mi para 465 mi ≈ 32%).

| | **10 mil MAU** | **1 mi MAU** | **100 mi MAU** |
|---|---|---|---|
| Eventos/dia | 200 mil | 20 mi | 2 bi |
| Escritas/s (média → pico) | 2 → 7 | 230 → 700 | 23 mil → 70 mil |
| Entregas/s no pico | ~40 | ~4 mil | ~420 mil |
| Conexões simultâneas | 3 mil | 300 mil | 30 mi |
| Log novo/dia (sem mídia) | 0,2 GB | 20 GB | 2 TB |
| Gateways | 2 (HA) | 3–6 | 150–300 |
| Banco | 1 Postgres pequeno HA | Postgres particionado + réplicas | FoundationDB (ou Scylla) + camada fria |
| Infra (sem IA nem Ao vivo) | ~US$ 500–1 mil/mês | ~US$ 10–25 mil/mês | dezenas de US$ milhões/ano *(ordem de grandeza)* |

O **custo dominante é a IA, não o chat**. Por isso o orçamento por agente é uma Concessão medida no gateway de modelos.

**O que os grandes fizeram, e o que pegamos**
| Quem | O que fizeram (fatos checados) | O que pegamos |
|---|---|---|
| **WhatsApp** | Erlang + FreeBSD. Em 2014: 465 mi de MAU, 147 mi de conexões simultâneas, pico de 342 mil msgs/s entrando e 712 mil saindo, ~550 servidores (~150 de chat, ~1 mi de conexões cada), ~10 pessoas no Erlang. A empresa tinha ~50 funcionários e ~25 engenheiros. Mensagens guardadas só até a entrega. | Obsessão por simplicidade, poucas peças e conexão barata. **Diferença:** guardamos o log (cifrado), porque multi-aparelho, Itens e agentes exigem histórico. |
| **Discord** | Elixir para tempo real; mensagens em ScyllaDB particionadas por canal + bucket de tempo; serviços de dados em Rust com coalescência de leituras; **DAVE**, E2EE de áudio e vídeo **usando MLS**. | Partição por Espaço, coalescência em Espaços quentes, Rust nos caminhos críticos e MLS também no Ao vivo. |
| **Slack** | Gateways regionais (WebSocket), Channel Servers com hash consistente por canal e ordem total por canal, Presence Servers só para usuários visíveis, MySQL sharded via **Vitess** por canal/usuário (2,3 mi QPS no pico). | Dono por Espaço com hash consistente, gateways regionais e presença só do que está visível. |
| **Signal** | Servidor em Java com Redis + DynamoDB (TTL de 7 dias), migrando para FoundationDB (commits de 2026); armazena para entregar e apaga. | Mínimo de metadados e de retenção, e FDB como estágio 2. |

**Por que Rust no servidor**
- Segurança de memória ao fazer parsing de entrada hostil: é a primeira linha de defesa de um mensageiro.
- Latência previsível, sem pausas de GC. Foi o motivo do Discord trocar Go por Rust no serviço de Read States, e de sofrer com o GC da JVM no Cassandra.
- **Os mesmos crates** de tipos, validação e Concessões no cliente e no servidor.
- O tokio aguenta centenas de milhares de conexões por nó.

**Tradeoff assumido:** o Erlang/Elixir tem a melhor história de "um milhão de conexões". O Rust ganha no compartilhamento de código e na segurança.

### 5.2 Segurança

**Modelo de ameaças** (de quem nos defendemos):
1. Rede passiva ou ativa.
2. **Nós mesmos**: operador curioso, servidor comprometido ou pedido legal. Não podemos ler conteúdo privado.
3. Membro malicioso.
4. Aparelho roubado.
5. **Agente ou provedor comprometido e prompt injection** vindo do conteúdo.
6. Spam e abuso.
7. Cadeia de suprimentos (dependências, CI).

**E2EE: MLS em vez de Signal Protocol**
- **MLS (RFC 9420, 2023) via OpenMLS:** um protocolo só para 1:1 e grupos, com atualizações em **O(log n)**, forward secrecy e **post-compromise security em grupo**. É padrão IETF e já está em produção no DAVE (chamadas do Discord), no Webex e no Wire.
- **Signal:** usa Double Ratchet no 1:1 (com **PQXDH** em 2023 e o **Triple Ratchet/SPQR** com ML-KEM em out/2025) e **Sender Keys** nos grupos. Isso dá distribuição O(n) e uma recuperação pós-comprometimento mais fraca em grupo.
- **Ciphersuite:** `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` no v1.
- **Pós-quântico:** suite **híbrida X-Wing (X25519 + ML-KEM-768)** assim que estiver estável no OpenMLS *(a verificar o status)*. Até lá, o transporte usa TLS 1.3 com troca híbrida de chaves PQ (padrão em Cloudflare e navegadores), contra "colher agora, decifrar depois" nos metadados.

**Key transparency**
- Diretório de chaves auditável com o **AKD** (biblioteca da Meta usada no WhatsApp desde 2023, em Rust, auditada pela NCC Group). Ele mapeia Identidade → chaves de aparelho.
- Os clientes verificam provas de inclusão automaticamente, e auditores externos verificam a consistência. Inspiração extra: o Contact Key Verification do iMessage, em que os próprios aparelhos checam a consistência.

**Verificação de aparelhos:** "Verificar pessoalmente" com QR e números de segurança na ficha do membro. Uma troca de chave gera aviso, que a key transparency deixa raro e confiável.

**Multi-aparelho**
- Cada aparelho tem **a própria folha MLS** e a própria chave (como o WhatsApp desde 2021), e a chave raiz assina a lista de aparelhos publicada no diretório.
- Vincular um aparelho = QR + confirmação, e o histórico vai por transferência cifrada entre aparelhos.

**Metadados**
- Nada de grafo de contatos no servidor; descoberta por username e link. Pushes, nomes de Espaços e Itens vão cifrados, e os envelopes têm padding.
- **Sealed sender** (inspirado no Signal) na fase 2: credenciais anônimas por Espaço, para o relay entregar sem saber quem enviou.
- **Tensão legal:** o Marco Civil exige guardar registros de acesso (IP + horário) por 6 meses. Guardamos isso e nada além.

**Backups:** a chave raiz e o arquivo de histórico são cifrados com a chave derivada do **PRF da passkey** e sincronizados pelo iCloud Keychain. Há um código de recuperação opcional e, depois, um cofre com HSM no estilo dos backups E2EE do WhatsApp.

**Agentes dentro de Espaços E2EE**
- O agente é membro MLS com folha própria e é **declarado na UI** ("Financeiro lê esta conversa · roda em Zoen Cloud").
- A chave do agente fica no runtime, embrulhada por KMS. A decifragem acontece em memória por tarefa, sem logs de conteúdo, e a memória do agente é gravada **como Itens cifrados no seu Espaço**.
- Provedores de modelo externos operam com contratos de retenção zero e lista permitida por Espaço.
- **2027:** runtime em **enclaves** (AWS Nitro + GPUs com computação confidencial, com atestação remota).
- **Já no v1:** a opção "só agentes locais" (Foundation Models no aparelho).
- **Contra prompt injection:** conteúdo de terceiros é sempre "não confiável", toda ferramenta passa pelo avaliador de Concessões e as linhas vermelhas pedem humano.

**Endurecimento do servidor**
- Rust, mTLS interno, segredos no KMS, IAM mínimo, Cloudflare WAF/DDoS e rate limit por identidade e por IP.
- **App Attest** contra bots e `cargo-deny`/`cargo-audit` + SBOM.
- Regra de 2 pessoas para produção, backups cifrados e builds reproduzíveis dos clientes (fase 3).

**Abuso com E2EE**
- **Message franking:** um compromisso criptográfico na mensagem permite que a denúncia prove o conteúdo sem o servidor ler nada antes.
- Pedidos de mensagem de desconhecidos (como o Signal), limites para contas novas e reputação pelo grafo de convites.
- Prévias de link geradas por quem envia.

**Auditorias e bounty:** auditoria externa de cripto e integração antes do lançamento público (NCC, Trail of Bits ou Cure53, por exemplo) e bug bounty aberto após o beta. Abrir a spec fica para depois (D14).

**LGPD:**
- Encarregado e RIPD (relatório de impacto) para o processamento de IA.
- Dados em São Paulo; transferência internacional com as cláusulas da ANPD (Res. 19/2024).
- "Você › Dados" para exportar e apagar; retenção como Concessão com prazo.
- ECA Digital e 18+ no v1 (D15).

---

