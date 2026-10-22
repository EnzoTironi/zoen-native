[Signal reference index](signal-reliability-reference.md).

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
