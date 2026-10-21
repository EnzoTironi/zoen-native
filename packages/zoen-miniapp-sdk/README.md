# Apps criados por agentes

Este é o SDK interno para os agentes que criam Apps no Zoen. A pessoa pede o que
precisa na conversa, recebe um cartão e toca para usar. Não escolhe framework,
runtime, manifesto, servidor MCP ou permissões técnicas. Pedidos de acesso surgem
quando uma ação precisa deles, explicados pelo resultado para a pessoa.

Reutilizar esta base e `@zoen/ui`; não criar outro transporte, outro armazenamento
ou uma interface de configuração de SDK no produto. O cliente oficial MCP Apps
continua responsável pelo protocolo. O kit do Zoen acrescenta serviços do host,
tokens e convenções de experiência.

## Uma experiência de App

- A capa é uma projeção nativa dos dados canônicos, com um quadro único de
  160 × 160 pontos, raio 24, respiro 12 e espaçamento 10. A mesma capa aparece em
  Home, no carrossel e nas mensagens. Não é uma WebView ou HTML arbitrário dentro
  do cartão. O host decide sua geometria; cada App fornece conteúdo.
- O título, a ilustração ou métrica e o resumo usam posições e prioridades
  comuns. Texto grande reduz informação secundária antes de decoração; a
  descrição acessível mantém o conteúdo completo. Imagens e estados não mudam o
  tamanho do cartão. Fonte de geometria:
  [`app-card-layout.ts`](../../apps/zoen/src/miniapps/app-card-layout.ts).
- Os cards aparecem diretamente em suas posições, sem animação de entrada. Ao
  segurar para organizar, todos os cards dessa área mostram X e Pin, com alvos
  nativos de 44 pontos. O X remove o atalho; o artefato e seu histórico continuam
  disponíveis. Pin controla o favorito da Home. A organização pertence ao host;
  o App não desenha menus ou controles de gerenciamento dentro da própria capa.
- Tocar abre o App em tela cheia com transição nativa e um X flutuante para sair,
  sem título ou faixa de cabeçalho. O fechamento usa a mesma guarda de salvamento
  da navegação; uma falha preserva o trabalho e oferece recuperação.
  O host publica a área do X em `hostContext.zoen.chrome.closeButton`; o SDK a
  valida e os tokens `--z-close-right` e `--z-close-bottom` permitem afastar apenas
  os controles que a cruzam. O canvas mantém a tela inteira e as áreas seguras
  reais. A projeção é removida quando o App volta ao modo inline ou é encerrado.
- Retrato e as duas posições horizontais atualizam dimensões e áreas seguras da
  mesma sessão. Rotação ou tema não significam reabrir o documento, reenviar o
  resultado inicial ou salvar uma mutação.
- Cores, controles, foco, teclado, acessibilidade e movimento reduzido seguem o
  kit compartilhado. Não duplicar o X do host, pinagem, gestão de conta ou
  confirmação de permissões dentro do App. Gestos do canvas e dos jogos pertencem
  ao App; fechar não pode capturá-los globalmente.

## Tecnologias de criação

| Implementação interna | Caminho |
| --- | --- |
| HTML/CSS e JavaScript/TypeScript | Usar o SDK sem React. O produtor atual compila uma entrada JS/TS em HTML offline com integridade; ele não aceita `.html` como entrada de manifesto. Um provedor MCP externo pode fornecer seu recurso HTML padrão. |
| React | Usar `@zoen/ui`, seus hooks e tokens. Manter uma única cópia de React com o produtor existente. |
| WASM | Usar uma interface HTML/JS MCP App ao redor do motor. O perfil explícito do DeckCraft já integra a identidade fixada do par JS/WASM, carregamento verificado, isolamento e ponte para o documento canônico. Isso não admite automaticamente outros motores: cada pacote precisa de identidade, orçamento, ABI e jornada próprios. Um build WASM isolado não comprova suporte no host. |

O agente escolhe a tecnologia a partir da tarefa e dos recursos comprovados do
host. A escolha não aparece como uma etapa para a pessoa. Preferir o caminho
mais simples que entregue a experiência; não reimplementar um editor mantido.

## Usar os contratos existentes

`zoen.ready()` negocia o host. `zoen.state.get/subscribe` lê a projeção do Item;
`zoen.state.call` chama uma ferramenta declarada, recebendo o resultado canônico.
`zoen.capabilities.has` detecta serviços disponíveis, sem conceder permissão.
`zoen.theme` e os tokens do kit acompanham o host. As APIs opcionais nativas,
compartilhamento e links passam pelas regras e confirmações do host.

Pessoas e agentes operam o mesmo artefato e as mesmas ferramentas autorizadas.
Seleção, campos ainda não enviados e animação podem ser locais. Documento,
versões, recibos, autoria, permissões e sincronização pertencem ao núcleo Rust.
Não usar localStorage/IndexedDB/WebView como única cópia durável. Não aceitar
`actor`, caminho de arquivo ou identidade enviados pela interface como autoridade.
Uma capa é conteúdo de apresentação, nunca um comando ou uma permissão.

Para aplicações React, `useZoenState` e `useTheme` vêm de `@zoen/ui`.
Os seis [Apps existentes](../../miniapps/builtins/README.md) demonstram o uso real,
com estado e ferramentas canônicos. Eles são uma referência de implementação,
sem autorizar copiar suas regras de domínio para outro App.

## Produção e validação pelo agente

O produtor existente em [`tools/miniapp-build`](../../tools/miniapp-build/build.mjs)
valida o manifesto, os imports, os limites físicos de arquivos e links simbólicos,
as dependências permitidas e o orçamento do pacote. Gera HTML e hash verificados
pelo host. Reutilizar esse fluxo para JS/React; um novo formato de pacote precisa
de uma extensão explícita e testada, não de relaxar os limites atuais.

O agente deve provar criação, uso manual, alteração autorizada por agente,
fechamento, reinício e recuperação. Conferir operações repetidas, ACK perdido,
base antiga, cancelamento, perda de acesso, tema, retrato/paisagem e texto grande.
A primeira integração de motor externo segue o
[aceite DeckCraft](../../docs/dev/artcraft-editor-integration.md), incluindo
exportação e compartilhamento de uma versão escolhida.

## Estado e próximos passos

Já existem o SDK privado, o kit React, o produtor de recursos, ferramentas de
criação dos Apps do catálogo, o host MCP e documentos criptografados. Criar uma
instância de um App existente por ferramenta não equivale a gerar, validar e
registrar um novo programa arbitrário durante uma conversa.

O contrato acima orienta a consolidação dessas peças. Ainda faltam o fluxo
completo de pacote criado por agente, sua admissão e atualização com revisão
verificável, o carregador binário WASM e a qualificação completa de cada editor.
Não anunciar essas capacidades a partir de um cartão, compilação ou demonstração.
Não publicar automaticamente o pacote npm privado nem importar contas, colaboração
ou armazenamento dos projetos usados como motores.

Referências: [MCP Apps](https://modelcontextprotocol.io/extensions/apps/overview),
[host e persistência atuais](../../docs/dev/mcp-apps.md),
[plano de editores](../../docs/dev/artcraft-editor-integration.md).

## Motores de documento gerenciados

O helper independente [`src/managed-host-ui.ts`](src/managed-host-ui.ts) compartilha
somente apresentação: dimensões/áreas seguras/tema, área do X, fila de geometria,
validação do ACK, aposentadoria e um pedido de exportação de cada vez. Ele não
importa `src/index.ts`, não cria outro `App`, não conecta transporte e não guarda
documentos. Um bootstrap que já possui o cliente oficial MCP deve importar essa
entrada diretamente; importar o SDK principal criaria a instância padrão dele.

O adaptador tipado oferece `applyHostLayout(layout)` e
`setExportIntentHandler(callback)`. A configuração empacotada fornece formatos
suportados, `share(format)`, guarda `ready()` e tratamento de erro. Os formatos
são copiados ao instalar o owner. Não obter essa política de conteúdo, de callbacks
do motor, de `kind`/`revision`/`capabilities` declarados pelo guest nem do contexto
da interface. Esse adaptador não concede acesso: ferramentas do host e núcleo
continuam verificando artefato, versão, autoria, permissões e bytes.

| Comum, já extraído nesta camada | Continua específico de cada editor |
| --- | --- |
| Geometria finita, áreas seguras, X e tema | Como o motor aplica esses dados aos seus controles/canvas |
| Coalescência, ACK limitado e aposentadoria | Binding/API da Session do motor e sua inicialização |
| Exclusão de pedidos de exportação simultâneos | Lista de formatos permitida pelo build e ferramenta nativa autorizada |
| Categorias fixas de erro de inicialização | Formato nativo, parsing, imports/exports e bytes exatos |
| Protocolo MCP e loader verificado existentes | Revisão/ABI/asset hashes, registro e validação canônica aceitos |

A fachada DeckCraft permanece em
[`miniapps/deckcraft-ui/src/host-ui.ts`](../../miniapps/deckcraft-ui/src/host-ui.ts).
Ela traduz o adaptador para o binding `zoenHostRequest`, conserva PDF/PPTX e captura
o Item já autorizado para a ferramenta existente `app_binary_share`. As validações
nativas/Rust do DeckCraft continuam fixas; a extração não torna outros formatos,
revisões, motores ou apps disponíveis.

O codec CSS de [`host-chrome.ts`](src/host-chrome.ts) conserva seu contrato próprio
para modo fullscreen e limites de controles. Ele não é substituído silenciosamente
pelo codec do motor, cujo contrato já existente aceita a geometria finita completa.
Um novo editor ainda precisa de adaptador de formato/Session, seed real, validação
canônica/recibos/CAS, registro e recursos pinados, exportação autorizada, integração
de agente e provas reais de edição/fechamento/recuperação. Não há integração de
Grid ou de outro editor anunciada por este helper.
