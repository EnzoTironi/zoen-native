[Signal reference index](signal-reliability-reference.md).

## Fontes e limites

Foram inspecionados os repositórios oficiais em versões imutáveis:

| Projeto | Commit |
| --- | --- |
| Signal-iOS | `d13b9da9e4622e16297b111243755c4c2972091a` |
| Signal-Android | `c56038f39c4cc0280e49a346ebabbc15edb2dadb` |
| Signal-Desktop | `f3040f3e13b6dca5021c3db149320e0f7a771f0a` |
| Signal-Server | `48a5fa529d641827f1cca3998e2cbea1de48c953` |
| libsignal | `1738ead8a12ffe539f6309958123dadb16dc3b95` |
| RingRTC | `84c5b2aff246392b365d73d6628529a149d6aed2` |

A leitura inicial do Zoen usa a base `dca975960619dacc658d9e748b7ece790a6b6028`.
O lote integrado até `fc81d9a1` acrescenta
recuperação de texto, revisão nativa de anexos, reprodução com escopo de sessão
e transações de recebimento e retenção do texto cifrado. A atualização seguinte
limita releituras e invalidações às conversas conhecidas e permite salvar cópias
editadas de fotos e vídeos compatíveis no rascunho cifrado antes do envio.
Não foram executados o Signal, seu backend ou benchmarks comparativos. Os números
de concorrência encontrados no código são escolhas dessa versão, não metas para
copiar. Ausência em um caminho inspecionado não prova ausência em todo o produto.

## Seis contratos obrigatórios

1. **Operação durável antes do efeito visual confirmado.** O outbox SQLite e os
   identificadores existentes continuam sendo a autoridade. Distinguir gravação
   local, aceitação durável pelo relay, processamento no destinatário e leitura;
   conexão aberta ou escrita no socket não comprovam entrega. Definir o modelo de
   falhas de cada confirmação, incluindo a diferença entre encerramento do
   processo e perda de energia.
2. **Continuidade da conversa.** Preservar a mensagem visível e seu deslocamento
   durante paginação, edição, exclusão, busca, teclado e menus. Alterações devem
   invalidar apenas seus consumidores e entrar em momentos seguros para a
   interação. Medir antes de substituir a lista ou prometer melhora de fluidez.
3. **Mídia independente da bolha.** Gravação, edição, preparação, upload, download
   e reprodução têm responsáveis com identidade estável. Reciclar uma célula não
   perde trabalho, progresso ou destino. Revogação e apagamento encerram esses
   responsáveis e liberam os arquivos decifrados.
4. **Três autorizações distintas.** Login Apple/Google autentica o provedor;
   vínculo autoriza um dispositivo; recuperação dá acesso ao histórico conforme
   suas chaves. Nenhum sucesso de OAuth, isoladamente, substitui as outras duas
   autorizações. Progresso, cancelamento e retomada devem corresponder a fases
   persistidas.
5. **Privacidade por camada.** Declarar separadamente proteção do conteúdo,
   banco local, cache, backups, diretório, grupos, metadados, notificações e
   agentes. Proteger alguns segredos não equivale a cifrar todo o banco; MLS não
   esconde automaticamente membros do relay.
6. **Segundo plano faz parte do produto.** Push acorda trabalho durável; sua
   perda não perde mensagens. Recebimento em segundo plano, extensão nativa,
   permissão negada, sistema encerrando trabalho e chamadas precisam de uma
   arquitetura de ciclo de vida e provas no dispositivo físico.

## O que preservar e o que mudar

| Área | Evidência no Zoen | Aprendizado e próximo trabalho |
| --- | --- | --- |
| Envio e recebimento | `sync.rs` confirma evento, estado de sincronização e remoção do outbox na mesma transação. `mls.rs` também retém texto cifrado e avanço do ratchet juntos e confirma o escopo dentro da transação de recebimento. | Seis regressões de falha SQLite com reabertura cobrem essas fronteiras. Ainda qualificar perda de resposta e entrega ponta a ponta; testes de transação não comprovam tolerância à perda de energia. |
| Backend | `crates/zoen-relay/src/log/`: log/head/deduplicação/membros em FDB; PostgreSQL para diretório e autoridade; NATS acelera entrega com retomada por cursor. | Manter os papéis atuais. Exercitar commit sem resposta, reinício antes do fanout e perda de wake. Uma mailbox por aparelho não substitui diretamente o log ordenado de um Space. |
| Atualização da conversa | `apps/zoen/src/core/store.ts` anuncia mudanças por conversa antes da leitura; eventos conhecidos de outra conversa preservam a janela ativa. Escopos enfileirados são unidos, e uma invalidação global prevalece. O snapshot de diretório e a assinatura ampla de `provider.tsx` permanecem. | Seis novas regressões cobrem escopo, coalescimento e troca de sessão. Falta medir custo, âncora de leitura e assinaturas por mensagem. Eventos relevantes ou sem escopo ainda pausam e reverificam o áudio; não há prova de continuidade audível sem interrupções. |
| Reprodução | No iOS autenticado, `message-audio-session.ts` possui player e arquivo, com identidade de conta/aparelho/sessão/mensagem/anexo; as bolhas assinam controles. Posição e velocidade ficam em cache volátil de até 128 mensagens. | Dezenove testes cobrem posse e descarte; o simulador confirma pausa ao sair, posição preservada sem retomada automática e fim sem reinício por atualização do chat. Reciclagem real da lista, interrupções do sistema e troca de saída em aparelho físico continuam pendentes. Android/Web e rascunhos mantêm seus responsáveis anteriores. |
| Revisão de anexos | QuickLook oferece edição de cópia em formatos de foto/vídeo compatíveis. A cópia é capturada e validada antes da substituição por revisão no diário Rust; legenda, ordem e destino são preservados. Outros arquivos continuam somente leitura. | No simulador, uma marcação na foto voltou ao rascunho com imagem e tamanho alterados e a legenda intacta. O usuário confirmou um envio manual posterior; não foi um envio automático da revisão. Falta qualificar rascunho editado ainda não enviado após reinício, Cancel, trim de vídeo e recepção independente. Não equivale a um editor completo de mídia. |
| Voz | `voice-session.ts` e os serviços de rascunho já separam gravação de envio. | Qualificar interrupção → sair da conversa → reabrir → editar → enviar, mantendo a experiência compacta e o mesmo destino. |
| Push iOS | `crates/zoen-relay/src/push.rs`: APNs background, prioridade 5; wake limitado e best effort. | Caminho urgente de alerta com extensão, progresso pendente por dispositivo e reconciliação de novas tentativas. A configuração e entrega reais ainda exigem prova. |
| Banco local | `crates/store/src/lib.rs`: SQLite, WAL, synchronous NORMAL e locking EXCLUSIVE; estados/segredos específicos são selados. | Avaliar cifragem integral e seu custo, chaves, WAL/temp/cache e recuperação. Extensão não pode abrir esse banco concorrentemente sem resolver o contrato de um processo. |
| Confiança e recuperação | `device-link`, `provider-auth.md` e `client-core/src/backup.rs` já separam vínculo, provedor e recuperação. | Fechar a jornada completa, formato portátil de histórico/artefatos, mídia ausente e restauração sem aparelhos sobreviventes; preservar as cercas de revogação. |
| Metadados e criptografia | MLS atual usa X25519/ChaCha20-Poly1305; relay conhece autores, aparelhos e membros. | Documentar diferenças em relação a PQXDH/Triple Ratchet, sealed sender e grupos privados. Pós-quântico, transparência e autorização anônima são decisões próprias de protocolo. |

Fontes da comparação: [fila FDB e transição backlog/live](https://github.com/signalapp/Signal-Server/blob/48a5fa529d641827f1cca3998e2cbea1de48c953/service/src/main/java/org/whispersystems/textsecuregcm/storage/foundationdb/FoundationDbMessageStream.java#L115-L242),
[limites e confirmação de mensagens](https://github.com/signalapp/Signal-Server/blob/48a5fa529d641827f1cca3998e2cbea1de48c953/service/src/main/java/org/whispersystems/textsecuregcm/grpc/MessageDispatcher.java#L42-L113),
[atualizações seguras da conversa](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/Signal/ConversationView/Loading/CVLoadCoordinator.swift#L594-L670),
[responsável pela reprodução](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/Signal/ConversationView/CVAudioPlayback.swift#L10-L145),
[preparação da mídia aprovada](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/SignalUI/AttachmentApproval/AttachmentApprovalViewController.swift#L889-L944),
[APNs por urgência](https://github.com/signalapp/Signal-Server/blob/48a5fa529d641827f1cca3998e2cbea1de48c953/service/src/main/java/org/whispersystems/textsecuregcm/push/APNSender.java#L82-L142) e
[ciclo da extensão](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/SignalNSE/NotificationService.swift#L209-L242).

## Outros aprendizados a incorporar

- **Transferências com limites.** Persistir a intenção/sessão antes de transferir;
  distinguir erro transitório, espera por conexão, prazo do servidor e falha
  terminal. Serializar o mesmo anexo, limitar concorrência global e reservar
  espaço antes da preparação. Reusar a fila de mídia existente do Zoen; selecionar
  upload resumível apenas onde os caminhos atuais não resolvem o problema.
- **Backups e mídia têm ciclos diferentes.** O formato portátil precisa incluir
  Páginas e Apps, sem restaurar credenciais de um aparelho antigo. Permitir estado
  explícito de anexo ausente e recuperação posterior. O Signal distingue as
  propriedades de backups hospedados e locais; não prometer as mesmas garantias
  para ambos. [Atualização de setembro de 2026](https://signal.org/blog/backup-improvements/).
- **Verificação de chaves verificável.** Fingerprint/QR e mudança de chave precisam
  de UX clara. Transparência exige monitoramento e auditor independente;
  assinatura do próprio diretório não basta e não prova identidade civil.
  [Verificação automática](https://signal.org/blog/automatic-key-verification/).
- **Uma ação, várias formas de acesso.** Menu, gesto, teclado e VoiceOver devem
  usar as mesmas ações elegíveis. Incluir RTL na arbitragem e conservar os
  controles acessíveis e o relógio de teclado compartilhado já existentes.
- **Orçamento de recursos e descarte.** Caches limitados, liberação em segundo
  plano, proteção de arquivos temporários e diagnóstico sem conteúdo são parte
  do mesmo ciclo de privacidade, inclusive após falhas.
- **Arquivos e banco têm um protocolo de posse.** Reservar um arquivo antes de
  criá-lo, vincular sua referência na transação e só liberar a reserva após o
  resultado durável. A limpeza deve poder repetir após falha sem apagar um anexo
  referenciado ou ainda aberto. [Reservas e limpeza do Signal](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/SignalServiceKit/Messages/Attachments/V2/OrphanedAttachments/OrphanedAttachmentCleaner.swift#L24-L63).
- **Diagnósticos com prévia.** Campos seguros e redução de dados em cada saída de
  log, retenção limitada e exportação que a pessoa pode revisar antes de enviar.
  A filtragem de texto para agentes não prova a proteção de todos os logs.
  [Prévia e envio explícito](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/Signal/Debugging/DebugLogs.swift#L86-L137).
- **Migração não é recuperação de corrupção.** Manter uma única autoridade Rust,
  preservar o original danificado, registrar restauração parcial e recusar um
  esquema futuro desconhecido. Prelaunch não exige carregar compatibilidade
  artificial; dados reais já retidos exigem um caminho explícito e verificável.
  [Separação no migrador](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/SignalServiceKit/Storage/Database/GRDBSchemaMigrator.swift#L15-L77).
- **Identidade não é descoberta.** Provedor de login, identidade criptográfica,
  perfil público e convite revogável têm funções distintas. Definir busca exata,
  enumeração, visibilidade e descoberta de pessoas separadamente dos diretórios
  de comunidades e agentes. [Racional dos usernames](https://signal.org/blog/phone-number-privacy-usernames/).
- **Testar estados e interrupções.** Modelar transições e testar cancelamento,
  repetição, perda de resposta e restauração; usar simulações e testes de
  propriedades onde há muitas combinações. [Raciocínio e simulações do SPQR](https://signal.org/blog/spqr/).
  O [fuzzer de sessões](https://github.com/signalapp/libsignal/blob/1738ead8a12ffe539f6309958123dadb16dc3b95/rust/protocol/fuzz/fuzz_targets/session_management.rs#L21-L104)
  também é referência para sequências de ações, além de bytes malformados.

## Reusar sem criar outra base de código

Avaliar primeiro bibliotecas mantidas e os recursos nativos do Expo. Reutilização
direta exige licença compatível com a distribuição pretendida, suporte à versão
atual, manutenção, custo de integração e uma autoridade única para cada estado.

O [libsignal](https://github.com/signalapp/libsignal/blob/1738ead8a12ffe539f6309958123dadb16dc3b95/README.md)
tem núcleo Rust e interfaces Swift/Java/TypeScript; seu uso externo é declarado
sem suporte e as APIs podem mudar. Seu pacote Node não estabelece suporte a Expo
ou navegador. Ele e [RingRTC](https://github.com/signalapp/ringrtc/blob/84c5b2aff246392b365d73d6628529a149d6aed2/README.md)
usam AGPLv3. RingRTC é middleware de chamadas, não sinalização, TURN, integração
de sistema e operação completas. Não foram adotados como dependências neste estudo.

SQLCipher é um candidato a avaliar pelo armazenamento Rust existente; acrescentar
GRDB como outro armazenamento Swift duplicaria a autoridade. Uma solução já
existente no Signal não deve ser copiada com seus limites sem avaliar o contexto.
O Zoen mantém Rust/Loro para artefatos, a autorização dos agentes e o isolamento
de MCP Apps. Um agente que lê conteúdo é um destinatário autorizado, e publicar um
Site continua sendo uma exportação explícita de uma versão escolhida.
