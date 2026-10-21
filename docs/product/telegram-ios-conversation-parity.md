# Reprodução da conversa do Telegram iOS

## Direção selecionada

Em 20 de outubro de 2026, depois da entrega do catálogo, o usuário pediu para
replicar toda a UI e as interações da conversa do Telegram iOS, incluindo anexos,
com fidelidade visual, adicionando os recursos do Zoen sobre essa base. O pedido
substitui a interpretação anterior de usar a conversa apenas para estudo.

Referência fixada: [Telegram iOS
f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838](https://github.com/TelegramMessenger/Telegram-iOS/tree/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838).
O [guia original](telegram-ios-reference.md), o
[mapa inicial de superfícies](telegram-ios-next-ui-study.md) e a
[matriz de 506 capacidades](../../output/telegram-feature-matrix-2026-10-20/index.html)
continuam como pesquisa datada. A matriz não comprova paridade de implementação.
O [mapa de implementação da conversa](telegram-ios-conversation-implementation.md)
registra os arquivos e as linhas upstream, o primeiro lote de código e as
diferenças ainda pendentes. Seus resultados de fonte não são aceite visual nativo.

## Escopo de implementação

| Superfície | Reprodução esperada | Extensão do Zoen preservada |
| --- | --- | --- |
| Navegação da conversa | Altura, alinhamento, título/status, avatar, retorno, controles e menus conforme o código fixado; área segura e gestos de navegação consistentes. | Entrada para agentes, Apps e detalhes existentes sem criar uma segunda barra permanente. |
| Timeline e mensagens | Largura, margens, agrupamento, cantos/caudas, tipografia, metadados, recibos, data, não lidas, respostas, links, seleção e posição de rolagem. | Tipos e recibos continuam representando os fatos do núcleo; cartões de Apps canônicos e padronizados. |
| Compositor e teclado | Geometria, campo multilinha, botões e seus estados, transição de envio/gravação, edição/resposta, teclado interativo e interrupção de gestos. | Busca na conversa/alternativa por agente e recolhimento dos cards coordenados com o mesmo movimento do teclado. |
| Anexos | Apresentação, seleção e contagem, câmera/galeria/arquivo, revisão, legenda, edição disponível, envio, cancelamento e retorno à conversa. | Permissões, rascunhos e capacidades realmente implementadas; preservar os originais e a seleção quando uma operação falhar. |
| Mídia | Miniaturas com proporção correta, álbuns, progresso/erro/tentativa, visualizador, zoom, vídeo, reprodução e saída interativa. | Criptografia, verificação dos bytes e download retomável sob autoridade existente. |
| Voz | Gesto de gravar, cancelar/bloquear, waveform/tempo, pausa/revisão, envio, reprodução e interrupções. | Revisão compacta e editor detalhado solicitados para o Zoen, sem substituir envio real por uma animação. |
| Ações e reações | Toque prolongado, origem da apresentação, reação rápida/seleção, menus, resposta, encaminhar, copiar, editar, excluir e seleção múltipla. | Threads do Zoen e respectivas permissões; cada controle só aparece quando sua ação funciona. |
| Busca | Entrada/saída, resultado ativo, contexto destacado, anterior/próximo e restauração de posição. | Uso do compositor e envio opcional para agente efêmero, sem enviar a consulta automaticamente. |

As linhas descrevem o escopo selecionado, não entregas já aceitas. O levantamento
de cada implementação deve apontar arquivos, linhas e condições da revisão
upstream, além de identificar as diferenças ainda existentes no Zoen. Não inferir
gestos, métricas ou durações somente do nome do componente ou de uma screenshot.

## Integração

Usar os componentes nativos existentes quando reproduzirem o comportamento.
Adaptar código upstream apenas com dependências, atribuição e licença registradas.
Uma tradução de comportamento não equivale à importação de um módulo Swift.
Não adicionar TDLib, uma conta Telegram ou seu backend para reproduzir a UI.

O núcleo Rust e os contratos autenticados de mensagens, mídia, outbox, permissões,
Apps, agentes e persistência continuam sendo as autoridades do produto. Evitar um
segundo estado durável no frontend. Preservar drafts, retry, cancelamento e
recuperação ao trocar a apresentação. Não redefinir sucesso de envio ou salvamento
para imitar uma animação upstream.

Os cards de Apps usam o mesmo componente canônico no chat, no carrossel e no
Início. Manter tamanho padronizado, organização, X/Pin durante edição, abertura em
tela cheia com X flutuante e suporte a retrato/paisagem. O céu e os recursos próprios
entram como extensões; não justificam alterar arbitrariamente as medidas da
conversa que o usuário pediu para reproduzir.

## Critério de verificação

Comparar o mesmo tamanho de tela, orientação, escala de texto, idioma, tema e
estado da conversa. Registrar os parâmetros e usar conteúdo sintético equivalente.
Conferir por sobreposição/diferença visual as áreas reproduzidas e registrar
separadamente as extensões intencionais do Zoen. Uma semelhança geral ou um teste
que apenas repete constantes da implementação não comprova fidelidade visual.

Para interações, executar jornadas completas de abrir, interromper/cancelar,
retomar e concluir. Cobrir teclado, scroll/gestos concorrentes, seleção de mídia,
erro de envio, permissão negada, áudio interrompido, VoiceOver, texto ampliado e
redução de movimento. Preservar drafts e conteúdo confirmado após fechar e
reiniciar. Medir a animação real, incluindo o gesto invertido no meio do movimento.

Implementar e validar em lotes coerentes. Concentrar integridade e falhas no
núcleo/backend; reservar o iPhone para comportamento visual, teclado, gestos,
mídia, áudio e ciclo de vida. Builds, testes unitários, catálogo e código de
referência são evidências parciais. A paridade visual e de interação permanece
**pendente** até a comparação e a execução nativa dos respectivos fluxos.

Essa seleção não altera as trinta jornadas existentes, não aprova automaticamente
as outras capacidades Premium/Business/comércio do catálogo e não altera as flags
globais de prontidão do produto.
