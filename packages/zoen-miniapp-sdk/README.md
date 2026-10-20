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
- Tocar abre o App em tela cheia com transição nativa e um X flutuante para sair,
  sem título ou faixa de cabeçalho. O fechamento usa a mesma guarda de salvamento
  da navegação; uma falha preserva o trabalho e oferece recuperação.
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
| WASM | Usar uma interface HTML/JS MCP App ao redor do motor. Não inventar um segundo protocolo para WASM. Admissão de binários, carregamento verificado, CSP específica e ponte de documento ainda precisam da integração de pacote; um build WASM isolado não comprova suporte no host. |

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
