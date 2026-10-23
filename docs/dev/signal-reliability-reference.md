# Signal como referência de confiabilidade

Decisão do produto em 20 de outubro de 2026: Signal é a principal referência de
confiabilidade do Zoen, tanto no aplicativo quanto no backend. Notion e ChatGPT
Space continuam como referências da criação colaborativa. Esta análise de código
define trabalho e critérios; não aprova equivalência de segurança, desempenho ou
operação em produção.

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

## Aprender com as suítes públicas

Os levantamentos de 20 de outubro de 2026 estão no
[registro de contratos](signal-test-contracts.json). Há 51 mapeamentos baseados
em asserções efetivamente lidas, em seis repositórios: 24 de servidor/libsignal,
18 famílias dos três clientes e nove do RingRTC. Uma família pode conter vários
casos; os mapeamentos também podem compartilhar um invariante. Esses números não
são quantidade de testes executados nem uma extração completa das suítes. Os 42
registros originais conservam a base comparada `f3ab6fae`; cada entrada RingRTC
explicita `01de169b`. Subconjuntos implementados depois indicam seus próprios
commits, sem transformar a classificação histórica em aprovação atual.

O inventário identifica versões, caminhos, fixtures inválidas, parametrização e
alvos de fuzzing onde encontrados. Os 1.702 arquivos candidatos dos clientes
foram enumerados por padrões; apenas os trechos registrados de 19 arquivos
selecionados tiveram leitura semântica. Os contadores lexicais não expandem
parâmetros e podem incluir comentários/infraestrutura. No servidor e libsignal,
as áreas efetivamente inventariadas e trechos lidos também são explícitos.
No RingRTC, 351 caminhos rastreados resultam em 20 candidatos amplos, incluindo
15 fontes, dois executores e três arquivos de apoio/configuração. As 121 marcações
Rust, 34 Swift e 35 Node são contagens lexicais dos arquivos obtidos, sem expansão
ou execução. A maioria das asserções, todos os corpos Android e mecanismos
inline de fuzzing permanecem sem revisão. Os nove registros distinguem callbacks
ICE e mídia simulada, áudio de teste no Swift e HTTP simulado no Electron de
execução nativa real; não comprovam privacidade do microfone, continuidade sonora
ou liberação física dos recursos. Nenhuma suíte do Signal foi executada.

Cada entrada registra estado inicial, sequência, falha, resultado realmente
assertado, fonte imutável e equivalente/lacuna no Zoen. A classificação vale para
a fonte comparada, não aprova equivalência de produto. Priorizar perda de dados,
duplicação, autorização, recuperação e mídia. Testar os limites definidos pelo
Zoen, sem copiar números de retries, janelas ou políticas de outro protocolo.

O primeiro teste original integrado, em `crates/mls/src/tests.rs`, verifica que
um ciphertext adulterado não modifica as linhas de grupo OpenMLS nem seu
checkpoint. Após fechar/reabrir a conexão SQLite, a mensagem original continua
legível; outro reabrir recusa replay e aceita tráfego posterior. A biblioteca MLS
passou nos 19 testes e no Clippy estrito. O contrato é da camada criptográfica:
o Zoen preserva eventos externos autenticados como histórico opaco quando o
conteúdo interno ainda não pode ser decifrado. Não copiar uma asserção de rejeição
do evento/cursor inteiro que contrariaria essa política. O teste não representa
perda de energia, processo encerrado ou recepção por outro aparelho.

Três testes originais em `crates/client-core/src/backup.rs` verificam oito
roundtrips com nonces distintos e bytes exatos, rejeição por chave/dono incorretos,
adulteração de nonce/ciphertext/tag, truncamento e bytes extras. Corpos inválidos
selados pelo próprio teste cobrem framing, versão e dono interno autenticados. A
biblioteca client-core completa passou em 539 testes, com Clippy estrito e
formatação aprovados. Controles negativos detectaram nonce fixo e validações
removidas de versão/dono. Isso cobre o envelope, não a restauração real de SQLite,
Wasm/navegador ou a jornada inteira de recuperação. O código de produção não
precisou mudar; nenhum defeito de produção foi demonstrado por esses quatro novos
testes.

Em seguida, os commits `01de169b` e `0db43269` acrescentaram dois testes
originais. Em S04, a sessão WebSocket real do cliente recebe um evento persistido
que ultrapassa a última página de replay; solicita o cursor ausente antes do
primeiro `SyncDone`, reconcilia replay sobreposto e verifica quatro eventos e duas
mensagens com IDs, ordem, sequências e textos exatos, inclusive após reabrir o
SQLite. Nenhuma mensagem posterior disfarça a reconciliação. A revisão seguinte
retira explicitamente a posse do `EngineLease` antes de reabrir. Isso ainda não
qualifica a janela real de inscrição do relay/FDB/NATS nem uma notificação
completamente perdida: esses interleavings continuam como próximo lote.

Em C16, signatários, horários e IDs fixos comparam projeções de edições assinadas
no outbox durável, confirmações próprias, replay remoto, duplicatas e reabertura.
Asserções cobrem cardinalidade e DTO completo, quatro mensagens, texto editado,
citação, thread e nove eventos finais. O teste insere os intents assinados no
outbox diretamente; admissão pelo comando público de edição, MLS, anexos,
expiração e um corpus gerado amplo permanecem fora desse recorte. O lote integrado
do client-core passou nos 541 testes e no Clippy estrito. Os controles positivos
e negativos registrados são evidência dos contratos delimitados; esta atualização
do registro não executou novamente testes, aplicativos ou serviços e não altera
as flags de aceitação do produto.

A matriz também registra limites encontrados nas próprias suítes: comparações
por `zip` sem checagem prévia de cardinalidade; fixtures cujo erro esperado vem
de uma validação anterior ao problema sugerido pelo nome; testes de backup
condicionados a diretórios externos; e fuzzing com exclusões explícitas. Aplicar
as propriedades úteis e fortalecer as checagens do Zoen quando adequado, usando
fixtures próprias. Nenhum código de teste do Signal foi copiado.

## Avaliação de um fork completo

Recomendação em avaliação, sem migração autorizada por esta análise: manter o
Expo como interface do produto e avaliar as fundações de mensagens separadamente.
Um fork próximo do Signal pode reduzir o trabalho para um mensageiro iOS privado.
Para o Zoen, assume também aplicações distintas para as outras plataformas, seu
backend e a integração de Apps, Páginas, permissões e agentes. Estar pre-launch
permite mudar essa escolha; investimento já feito não é critério para preservá-la.

O núcleo Rust de libsignal pode ser avaliado pela ponte nativa que já existe no
Zoen. A ausência de um pacote pronto para Expo não impede essa integração. Os
critérios relevantes são compatibilidade de identidades, grupos e histórico,
transações de sessão, manutenção das atualizações e o custo de distribuição em
cada plataforma. A avaliação não autoriza misturar protocolos nem estabelece
que o MLS atual tenha as propriedades de privacidade do Signal. Nenhum código do
Signal foi integrado e a licença do Zoen permanece inalterada neste lote.

## Execução em lotes

O lote iOS até `2785d88e` passou em 931 testes do app e no build Release do
simulador. O núcleo passou em 534 testes antes do ajuste final do escopo MLS;
os 36 testes do módulo MLS passaram novamente após esse ajuste, incluindo seis
regressões de durabilidade. Esses conjuntos se sobrepõem. A verificação nativa
usou uma conta e um áudio sintéticos: envio real pelo relay local, reprodução,
fim sem reinício após nova mensagem e posição pausada mantida ao sair e voltar.
Ela não qualifica recepção por outro aparelho, continuidade audível durante
invalidações amplas ou interrupções de áudio do sistema.

O lote seguinte até `fc81d9a1` passou no `npm run check`: TypeScript, Oxlint,
Oxfmt e 948 testes, sem falhas ou testes ignorados. Três auxiliares Swift tinham
atingido o limite de compilação de 45 segundos. Um deles passou isoladamente;
limitar a execução a um arquivo de testes por vez permitiu passar a suíte completa
em 101,33 segundos, preservando os limites e as verificações. Isso sustenta uma
correção do agendamento; não identifica o gargalo interno exato do compilador.
O Excalidraw passou em 68 testes, incluindo navegador real, com prazos separados
de inicialização e execução. Não substitui a aprovação do CI em outro sistema.

Dois builds Release do simulador passaram com a edição nativa. O artefato final
de aplicativo corresponde a `4214e0d1`; `fc81d9a1` altera apenas o agendamento de
testes. Os hashes SHA-256 do executável e do bundle instalados foram conferidos:
`b360784f0afc55044b52e73863e74c651ad89a0bde839213c7af8c220628b22a` e
`c6013795e46abf907a7d8e17ec88863e21ec077fb6d0098c093c67a7bc059c63`.
O auxiliar nativo de ciclo de vida do preview passou em 11 cenários; esses testes
não demonstram trim de vídeo. No iPhone 18 Pro com iOS 27, a foto sintética passou
de 146,3 KB para 282,9 KB após uma linha de marcação, preservando a legenda no
rascunho. Após o envio manual confirmado pelo usuário e o reinício, a conversa
exibiu a imagem marcada e a mesma legenda. A página Você também foi conferida
com suas cinco linhas nativas e navegação Geral → voltar. Os registros de passos
contêm diferenças entre as árvores de acessibilidade do gravador e do executor;
nenhuma aprovação de replay automatizado é alegada.

O lote de aprovação de mídia em `c899de92` passou no `npm run check` com
TypeScript, Oxlint, Oxfmt e 957 testes, sem falhas ou ignorados, em 99,89 segundos.
Um build Release iOS passou em 70,54 segundos. O artefato foi assinado, copiado e
instalado preservando os dados em um simulador novo; executável e bundle foram
conferidos com o artefato selado. A aprovação usa componentes Expo/nativos, mantém
o diário Rust como autoridade e vincula o envio à seleção efetivamente revisada.
Na rodada de interface, Home e Chats abriram; em seguida, o simulador isolado
foi encontrado desligado. Uma recuperação limitada falhou na inicialização dos
serviços CoreSimulator, antes de abrir a conversa. Aprovação em tela cheia,
apresentação de QuickLook a partir dela, teclado da legenda, Cancel, trim e
recuperação do rascunho editado ainda não enviado continuam sem prova nativa
nesse artefato. Nenhum replay automatizado é alegado. Build e testes não
substituem essas provas.
A correção Web separa as linhas de configurações de SwiftUI; sua qualificação
Release Web precisa passar novamente no CI.

1. **Concluir o lote iOS atual.** Recuperação de texto sem perda e revisão nativa
   de anexos. Não ampliar sua aprovação para edição completa de mídia.
2. **Durabilidade e continuidade.** Fixar os quatro estados de mensagem e o
   modelo de falhas; qualificar envio/recebimento com perdas; playback fora das
   células e atualizações por escopo com âncoras medidas.
3. **Segundo plano real.** Resolver acesso ao banco entre processos, extensão de
   notificações e reconciliação por aparelho. Validar em iPhone físico, inclusive
   app encerrado, permissão negada, aparelho bloqueado e token inválido.
4. **Recuperação e privacidade.** Vínculo/backup/restauração, cobertura do banco e
   arquivos locais, verificação de chaves e limites explícitos para metadados e IA.
5. **Chamadas e operação.** Selecionar componentes mantidos, integrar ciclo de
   vida e testar rede/retomada/carga dentro do perfil já declarado.

Cada lote tem uma validação integrada e evidências do artefato correspondente.
As trinta jornadas, seis plataformas e três flags de aceitação do produto completo
continuam vigentes. Referência estudada, teste de fonte e build não são aprovação
de uma jornada em produção.

## Checkpoint iOS de mídia — fonte `f3ab6fae`

O defeito da foto foi reproduzido com o Yoga instalado do React Native 0.88:
a imagem chegava a 252×360 pontos, enquanto a moldura mantinha 294×360, expondo
42 pontos à direita. A correção calcula uma única geometria a partir da proporção
original; imagem, máscara, legenda e metadados acompanham essa geometria. A sonda
passou em 32 casos e a verificação integrada do app em 961 testes, tipos,
Oxlint/Oxfmt. O build Release passou em 42,58 segundos; assinatura e hashes do
executável/bundle instalados correspondem ao artefato selado.

No simulador isolado, uma foto sintética abriu a aprovação em tela cheia, recebeu
uma marcação nativa e a legenda exata, foi fechada e recuperada ainda não enviada
após reiniciar o processo. Um envio explícito produziu a foto marcada e a legenda,
com a moldura preenchida e alinhada. A prova inclui 27 capturas preservadas e
atribuição separada para a foto já existente, cuja autoria não foi inferida.
Recepção independente, vídeo, múltiplos anexos e rotação nativa ficam pendentes.
O percurso gravado tem avisos de projeção e não recebeu aprovação de replay.

Dois detalhes de UX permanecem: texto de confirmação pendente aparece durante a
abertura da edição normal; salvar no QuickLook volta ao rascunho compacto, não
diretamente à aprovação expandida. Não há perda de rascunho observada nesse caso.
A aprovação desse cenário não encerra as jornadas completas nem altera as três
flags de prontidão.

## Reconciliação periódica após perda de entrega ao vivo

A [qualificação de 21/10/2026](cursor-repair-qualification-2026-10-21.md)
acrescenta uma prova delimitada ao acompanhamento de S04: com dois relays e
PG/FDB/NATS reais, perder o último Event depois do broker não deixa o cliente
permanentemente desatualizado enquanto o mesmo socket recebe Pongs. O cliente
reconcilia cursores aceitos e salva uma única repetição do evento. O controle sem
agendamento falha na asserção esperada; o código restaurado passa novamente.
A suíte integrada passou em 558 testes, incluindo o candidato independente de
preferências locais; nove regressões de cursor e Clippy estrito também passaram.
As classificações históricas dos 51 registros continuam preservadas. Perda no
broker, todas as corridas de inscrição, MLS/mídia, ciclo de vida móvel e custo de
históricos grandes continuam fora dessa prova. As flags de produto permanecem
inalteradas.

## Seguimento: primeiro catch-up sem progresso

O [recorte de inatividade inicial](initial-catchup-liveness-qualification-2026-10-21.md)
reproduziu uma sessão presa apesar de Pongs saudáveis e a corrigiu no responsável
de sincronização existente. Cinco casos novos de progresso/inatividade e nove de
reconciliação periódica passaram em transporte local real. O prazo de 60 segundos
é provisório; avanço genuíno em outro Space ainda pode renovar a janela do stream.
A fonte, os limites e a prova RED→GREEN são registrados separadamente, sem alterar
as classificações históricas nem afirmar equivalência com o Signal.

## Seguimento: recusa de upload e confirmação de anexos

O [recorte de upload](media-upload-refusal-qualification-2026-10-21.md)
reproduziu HTTP 400/413 com mensagem `Sent` no remetente reaberto, mensagem no
destinatário frio e ciphertext removido da fila. Agora File e sua mensagem
aguardam o upload; a recusa terminal retém bytes e falha o outbox existente
atomicamente, incluindo o grupo e criações órfãs que bloqueariam mensagens
posteriores. Uma segunda regressão confirmou e corrigiu a perda do cartão na
recuperação de anexos default sem metadados explícitos; isso não autoriza
publicação ou edição do File reservado para recuperação.

O lote final passou em 55 casos de mídia, sete casos finais de upload/compatibilidade,
quatro de armazenamento, um de recovery MLS e Clippy estrito. A execução completa
de 629 testes qualificou a implementação inicial; o pequeno follow-up default
recebeu os testes afetados. O relay é um fixture local, não o cluster de produção.
Recuperar/descartar um envio falho pela interface e limpar backoff de download
num Retry explícito continuam recortes seguintes, separados deste reparo.

## Lote de 22/10/2026: contratos e limites de download

O [recorte de oito contratos](signal-contract-batch-2026-10-22.md) revisita asserções
upstream fixadas e corrige explicitamente o caminho C09. A comparação revelou o
#197: perfil/fundo baixavam corpos sem limite, e o cache de perfil contornava a
política de tamanho já existente. A prova RED e os 28 casos focados GREEN, com
Clippy estrito, estão delimitados no relatório; não representam execução do
Signal, da suíte completa ou de uma jornada móvel. #139/PR #190 continua sendo
um reparo separado de confirmação autenticada, já integrado em main.
