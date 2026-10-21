# Exact conversation contracts: pinned Telegram iOS → Zoen

Source-only comparison at commit
`f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`; source hashes are recorded next to
this file. These are read source conclusions, not Telegram or Zoen runtime tests.
The user adopted the conversation UI baseline; unimplemented feature families in
the506-row research catalog are not silently selected.

| Area | Pinned upstream evidence | Current Zoen before this slice | Contract / bounded result |
| --- | --- | --- | --- |
| Current composer branch | [ChatControllerNode941](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Sources/ChatControllerNode.swift#L941) constructs ChatTextInputPanelNode; [panel662](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatTextInputPanelNode/Sources/ChatTextInputPanelNode.swift#L662) internal top5/left12/bottom4/right11;1432 sides8 and left action40+6;2342 minimum;2480/2481 actions40. | `components/message-composer.tsx`:52pt field,16pt type,18pt sides,15/14 vertical padding,10pt row gap,12pt container sides. `composer-action.ios.tsx`/`media-actions.ios.tsx`:52pt actions. | Candidate adopts regular-font base40/17 and exact base insets/gaps, with existing native controls and durable callbacks. The extra rounded-device side-inset branch is explicitly pending. |
| Font and dynamic geometry | [ChatTextInputFontMetrics19–68](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatTextInputPanelNode/Sources/ChatTextInputFontMetrics.swift#L19): regular field31, large33, XL38, XXL42; regular top4.5/bottom5.5. | `composer-text-field.tsx` snapshots fixed16×fontScale; `core/composer-fitting.ts` clamped52–160 even if native measured smaller. Swift measures the exact mounted UITextView and already supplies real insets. | Candidate changes iOS display+measurement to17×scale and minimum40 without adding padding twice; text/tag/nonce/width/context cancellation remains. OS font scaling is preserved rather than pretending it is Telegram's app-level seven-step setting.160pt cap remains pending exact available-height work. |
| Send / microphone | [panel3407](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatTextInputPanelNode/Sources/ChatTextInputPanelNode.swift#L3407) places media action6pt outside field;3453 puts send inside its trailing edge;75–84 action clearance40−10. | `message-composer.tsx` placed send, microphone and search navigation in the same separate right slot. | Candidate embeds only explicit send/edit/caption/AI-send; empty mic/search arrows stay outside, with exactly the previous intent/disable/retry functions. First slice does not replace media recording owner or recreate all send transitions. |
| Header/title/avatar | [ChatTitleComponent365/626/1047/1084](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/ChatTitleView/Sources/ChatTitleComponent.swift#L1044):14pt sides,17pt semibold/12pt regular,44pt container, title/subtitle centered; background true at160/1183. [ChatAvatarNavigationNode82–83](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatAvatarNavigationNode/Sources/ChatAvatarNavigationNode.swift#L82):44pt toolbar, inset3→avatar38. [ChatController6060–6073](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Sources/ChatController.swift#L6060) title long-tap search and separate avatar/details. | `chat-title.tsx` combined34pt avatar,16/11 title/activity and chevron; `chat-header.ios.tsx` reserved132pt for two disabled calls plus search. | Candidate separates actual details/avatar and centered title, removes only dead calls, preserves actual native Back and Search as Zoen extension. Extra Search affects available title width; native long-title/large-text and shared-glass fitting remains pending. |
| Keyboard/list/cards | [ChatControllerNode4572–4670](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Sources/ChatControllerNode.swift#L4572) coordinates interactive keyboard dismissal with input accessory geometry. | `conversation-viewport.tsx` uses KeyboardChatScrollView/native insets and SDK keyboard height; `keyboard-motion.tsx` drives rail progress; `conversation-rail.tsx` keeps mounted rail and collapses its reservation; chat/thread routes own scroll/reveal anchors. | Preserve the single existing native clock and measured composer reservation; smaller actual composer flows through its existing onLayout. No timer-based imitation or list reset. Full interruption/interactive-drag synchronization needs native proof. Cards remain our160pt Apps, not a removed Telegram-only area. |
| Bubble grouping/tails/status | [ChatMessageItemCommon137–146](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageItemCommon/Sources/ChatMessageItemCommon.swift#L137):edge3,spacing2+pixel/grouped0,text6±pixel/11,avatar38; [BubbleItemNode3754](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageBubbleItemNode/Sources/ChatMessageBubbleItemNode.swift#L3754) applies group spacing; [DateAndStatus617–620](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageDateAndStatusNode/Sources/ChatMessageDateAndStatusNode.swift#L617)11pt timestamp/6pt check offset; ChatMessageBackground implements real merged curve/tail. | `conversation-message.tsx`:3/14 row gaps,48pt incoming gutter even in direct chat,22/6 corner approximation, timestamp on its own row; `message-receipt.tsx`:14pt check. `core/message-presentation.ts` has180s grouping. | Separate artcraft slice owns message rendering. Use actual curve/group/text/status evidence; do not label a changed corner radius an exact tail. Preserve core receipt states, author routing, full image aspect and fixed160 App cards. This packet does not alter these paths. |
| Reply / thread gestures | [BubbleItemNode7264–7386](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageBubbleItemNode/Sources/ChatMessageBubbleItemNode.swift#L7264):incoming45/outgoing60 threshold, rubber-band range100/coefficient0.4, max180, indicator33, one threshold heavy haptic; default allowBothDirections=false at3972. | `core/message-swipe.ts`:64pt threshold plus velocity admission; `message-interaction.tsx`:horizontal gesture guards, native media arbitration, reduced motion, VoiceOver alternatives; negative→reply, positive→thread. | Telegram reply feel is a remaining concrete gap; opposite-direction thread is a Zoen feature to keep. Reuse existing ownership/gesture cancellation and edge navigation guards. No gesture change or new haptic claim in this packet. |
| Reply / edit accessory | [ChatInputMessageAccessoryPanel426–436](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatInputMessageAccessoryPanel/Sources/ChatInputMessageAccessoryPanel.swift#L426):52pt accessory with44pt close; typed reply IDs/quote model106+. | `message-context.tsx` has quote/edit alongside composer; `AttachmentReplyContext` uses immutable batch scope. | Keep true reply/thread IDs and frozen attachment scope; do not substitute current text draft as batch target. Existing compiled tests remain passing. Visual accessory matching belongs to subsequent message context slice. |
| Tap/reactions/context menu | [BubbleItemNode5822–5852](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageBubbleItemNode/Sources/ChatMessageBubbleItemNode.swift#L5822) dispatches real hit actions/click-through; double tap can apply default reaction; [reaction context38+](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Sources/ChatControllerOpenMessageReactionContextMenu.swift#L38) has reaction-specific participants/actions. | `conversation-message.tsx`:tap opens reaction bar;350ms hold actions; `message-actions.ios.tsx`:custom anchored Modal/palette/native buttons; `use-message-actions.tsx`:canonical callbacks measured after keyboard dismissal with recycled-row guards. | Full Telegram extraction, bubble lift, hit semantics, press cancellation and menu geometry remain to implement, reconciling earlier Zoen tap reaction intent with the now-adopted Telegram baseline. Preserve target/permission checks, after-dismiss navigation and accessible alternatives. No menu-completeness claim from this slice. |
| Media/attachments | [ChatMessageItemCommon140–143](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatMessageItemCommon/Sources/ChatMessageItemCommon.swift#L140) image2pt insets/300×380 compact bounds and video height limits; catalog MediaPicker/Gallery pinned entrypoints lead to full selection/review flows. | `attachment-approval.ios.tsx`, `durable-attachment-composer.tsx`, `attachment-review.ts`, `MediaAttachment.tsx`, `inline-image-frame.ts` preserve genuine media ownership/bytes/CAS. | Security agent owns full-bleed/paged review candidate. My sole media edit is the dropdown trigger40pt. Exact review/selection/viewer/video/zoom behavior remains separately qualified; no new crop/HD/sticker facade. |
| Compact audio / playback | [ChatRecordingPreviewInputPanelNode581–582](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/Chat/ChatRecordingPreviewInputPanelNode/Sources/ChatRecordingPreviewInputPanelNode.swift#L581):40pt preview/13pt waveform; panel3077 places preview inside input surface; [MediaPlayer1216–1236](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/MediaPlayer/Sources/MediaPlayer.swift#L1216) serializes pause/seek context; ChatMessageFileBubbleContentNode119+ derives genuine status. | `voice-dock.tsx`/`voice-review.ios.tsx` split compact/expanded editing; `message-audio-provider.tsx`/runtime/session have row-independent owned transport, native lease/session/revocation and preview/record exclusivity. | Compact waveform/play/record visual parity remains a real next slice; retain native owners and reviewed original bytes. Changing the shared native ComposerAction default reduces its inherited action size only; no audio lifecycle/engine mutation here. |
| Search / Apps additions | [ChatControllerUpdateSearch17–108](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Sources/ChatControllerUpdateSearch.swift#L17) derives query/location/thread and cancels/replaces prior search. | `core/conversation-search.ts` and chat route own scoped local-history query, stable selected-message reveal/older-newer navigation; no-results launches ephemeral assistant. `miniapps/rail.tsx` and canonical preferences own fixed160 cards/order. | Keep Zoen's actual-composer search/arrows/AI fallback and App cards as explicit additions. No new server search, cloud account, Premium, bot or other506-row capability is selected. |

Remaining work is implementation under the adopted UI direction, not optional
research. The first packet is deliberately bounded so root can batch all three
source slices, full app checks and one native Release. Only that integrated native
artifact can establish actual geometry/keyboard/scroll/animation/accessibility
results. The JSX tests do not attest haptics, focus or pixel matching.


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
