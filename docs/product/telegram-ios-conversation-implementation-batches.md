# Telegram iOS conversation implementation batches

[Back to the source comparison and conversation contracts](telegram-ios-conversation-implementation.md).

## Lote integrado de bolhas e revisão de anexos — 20/10/2026

Além do cabeçalho/compositor descritos acima, a revisão02 de bolhas aplica a
largura compacta/85% sobre a viewport antes dos recuos, tipografia17pt, margens
11/6pt e metadados na última linha somente quando as medidas nativas deixam
espaço. Preserva recibos reais, acessibilidade, dimensões de mídia e cards de Apps.
As caudas, agrupamento, autor e arte dos recibos ainda exigem a próxima tradução;
este lote de cantos16/8pt não é declarado pixel perfect. A revisão independente
corrigiu a base das larguras e verificou os limites500/501pt.

A revisão02 de anexos apresenta páginas pretas de tela cheia, imagens em contain,
vídeo ativo apenas na página selecionada, legenda por item e seleção ordenada.
O gesto consome uma conclusão por arrasto; mudanças de item, geometria e sessão
invalidam callbacks antigos. Arrasto sem desaceleração conclui no endDrag;
scroll programático não altera a seleção como se fosse gesto. Zoom/saída
interativa, editor completo e geometria de paisagem ainda estão pendentes.

Os pacotes privados imutáveis de fonte são
`/private/tmp/zoen-telegram-message-bubbles-v187/revision-02` (manifesto
`714eb998db0678d19dd21eca63d0ddfe6884abe9bb202f782f7f429818e604a2`) e
`/private/tmp/zoen-telegram-ios-attachments-v185/revision-02` (manifesto
`1049ebb13f7f38df8d58e5e20c962f380fdb34c331903ea578a886b28eaab856`).
As cópias de referência e os relatórios listam hashes e linhas da revisão
Telegram fixada; nenhum código ou asset upstream foi importado neste lote.
A aprovação de fonte e testes compilados não é aceite visual nativo.

O módulo Expo MediaLibrary58.0.11 e sua configuração de permissão estão
preparados neste lote; a nova galeria Photos permanece em pacote separado,
ainda sem integração nem validação no iPhone. Os identificadores estáveis dos
títulos Início/Chats apoiam a verificação do artefato sem alterar o layout.

## Segundo lote: gestos, gráficos nativos e Photos

O segundo lote substitui a aproximação do swipe por limites45/60pt para
recebidas/próprias, rubber-band de alcance100/coeficiente0,4 e retorno300ms
com a curva cúbica(0,38;0,70;0,125;1). Essa curva corresponde à implementação
efetiva da transição de300ms no `CAAnimationUtils.swift` fixado, e não a uma
mola genérica escolhida pelo nome da API. O indicador acompanha o dedo e o
haptic dispara uma vez ao cruzar o limite. O gesto oposto mantém threads como
extensão do Zoen. Reciclagem, perda de foco e troca de sessão invalidam ações
enfileiradas; o teste compilado cobre esses cancelamentos.

Balões e recibos passam a ter renderização UIKit/CoreGraphics. A cauda e as
margens de imagem expansível seguem as medidas estudadas; os checks usam os
dados procedurais upstream identificados em `THIRD_PARTY_NOTICES.md`. A
transição de reconhecimento usa100ms e escala1,3→1; o relógio tem ciclos
lineares de6s/1s. Uma célula reanexada exige novas propriedades de referência
antes de admitir efeitos, evitando animar um reconhecimento histórico como
novo. Reduce Motion remove os efeitos. A animação de substituição do fundo
só é admitida explicitamente; a transformação completa do envio não está
implementada por esse componente.

A barra pública de substituição mantém a mesma navegação nativa. O caminho
normal do Telegram usa60pt após a região superior e controles44pt em top+10,
bordas16pt e folga10pt para o título. O título medido centraliza e depois
respeita os controles efetivamente presentes, incluindo a busca adicional do
Zoen. A primeira versão instalada mostrou o título deslocado pela barra UIKit;
essa evidência motivou a substituição. Menu de histórico do botão Voltar,
transformação nativa do título e comportamento de primeiro layout permanecem
pendentes de qualificação.

A galeria Photos agora integra álbuns, seleção ordenada, acesso limitado e
transição para a revisão existente após o encerramento real da sheet. Usa a
API58 atual. Copia somente o arquivo autorizado para o cache próprio e libera
uma exportação temporária do SDK antes de admitir o rascunho. O módulo nativo
aceita apenas o namespace e nome canônico exatos do SDK, sem apagar originais
da biblioteca. Câmera, iCloud, rotação e a jornada do primeiro pedido de permissão
ainda exigem execução no iPhone.

Validação integrada de fonte: `npm run check` passou com1179 testes, tipos,
Oxlint e Oxfmt. Os dois modelos Swift reais de limpeza e recibos passaram em
macOS, falharam ao remover suas proteções e passaram após restauração. Isso
não substitui execução UIKit/iOS. O núcleo configurado Release e a atualização
dos Pods passaram; a compilação e a validação nativa deste segundo lote
permanecem pendentes nesta revisão.

O menu extraído estava num protótipo privado separado nesta revisão anterior.
O lote integrado descrito abaixo passa a registrar Stage/Source sob a mesma
raiz Fabric/RNGH. Isso resolve a composição do código, mas ainda exige provar
o recebimento de toques e a extração no UIKit. Não há aceite global de pixel
matching ou animações completas.

## App-wide design-system batch — in integration

The user expanded the selected baseline to the entire Telegram iOS UI, including
profiles, composer and theme customization. This supersedes the earlier
conversation-only scope. Home, App cards, creation and agents remain Zoen
extensions; no Telegram messaging/account backend is imported.

The shared presentation layer translates the pinned Classic/Night factories,
typography, navigation/list/context roles and selected motion constants. Exactly
two default sky families ship: Day and Night, with local automatic selection
(day06:00–20:00). Earlier phase preferences migrate without account changes.
Native appearance editing has two live previews, custom accent/outgoing/sky
colors, message text size, large static emoji and Reduce Motion. Custom colors
retain the raw selection for backgrounds while foreground controls and custom
bubble ink receive contrast protection. Device preference errors are tracked
per appearance/day/night key; saving Night cannot acknowledge a failed Day write.

The13–26pt continuous font control is currently a Zoen extension; Telegram's
seven named font steps, theme cloud/import/export, wallpaper photo/pattern and
automatic-night trigger editor are not yet claimed equivalent. Default Night
bubble colors retain the source gradient; arbitrary custom bubble colors get
adaptive ink. The native gradient currently uses the individual bubble bounds;
Telegram's coordinated viewport-wide gradient alignment remains a visual gap.

Inbox geometry now uses60pt avatars with16pt leading edge, compact16/15/14pt
title/preview/date, source-inspired folder underlines, actual delivery state,
muted unread badges and pin/mute indications. Conversation authors appear inside
group bubbles with38pt avatars; direct messages reserve no group avatar gutter.
The ten-minute grouping decision keeps real Zoen thread/day boundaries and
refuses to infer missing forward attribution. Static emoji sizes use the pinned
font-size rule; animated Telegram emoji assets are not imported. Date headers
follow actual local civil days, including projected App activity rows.

Three independently frozen packets are composed without replacing durable
media or message authority: context Stage/Source v196, composer/audio v197 and
attachment/gallery v197. Shared native module/media files were merged with exact
base/packet hashes retained privately. A root-owned Stage remains above the
actual navigation Stack under the same Fabric/RNGH root; its cleanup captures
the old native instance and participates in existing session/device drains. A
loading/erasing phase does not prematurely replace its ref with a new account.
Native menu extraction and media gestures still require the integrated build.

The reusable App keyboard viewport revision02 resizes the actual WebView using
the existing keyboard clock and publishes its real child layout through the
existing MCP host contract. No second keyboard clock or document authority is
created. This is not a claim that the separately observed fast DeckCraft title/
Notes character loss is repaired.

Earlier d5a419dc Release compilation and code signing passed and its exact bundle
was sealed; it was not installed or qualified after the user asked for a larger
UI port first. That older build does not qualify this new app-wide source batch.
Full app checks, native compilation, hardware gestures/accessibility and pixel
matching remain distinct gates. Profile is now integrated in the same batch: You,
Edit Profile, person/conversation details and actual scoped shared media. Profile
save/picker callbacks retain account/session/epoch and focus fences, including
blur without unmount. The presentation keeps existing permissions, draft ownership,
file integrity and disposal barriers. Calls, expanded-avatar transitions and
server-wide shared-link indexing are not fabricated by this port. None of these
source inventories sets product/parity/production flags true.

The integration review also corrected two presentation ownership races: native
settings callbacks now merge only the changed field against the provider's latest
admitted values, and retained voice controls cannot cancel, pause, retry or send
a replacement capture/draft. Already admitted Stop and successful send ACKs still
drain normally. Deterministic compiled-component regressions reproduce the old
failures and pass with the corrections. These are source/test proofs; actual
native touch and audio delivery remain part of the integrated device gate.
