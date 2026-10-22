# Evidências iOS da meta completa

Registros datados preservados da [meta completa](zoen-complete-product-goal.md).
O escopo e os critérios de aceite permanecem no documento principal.

### Progresso iOS — 21 de outubro de 2026

O Release v228 de `b1eee705`, com o motor DeckCraft experimental fixado por hash,
passou nos dois casos conhecidos de perda de maiúsculas a 35 ms. Um documento
novo recebeu o título completo de 18 caracteres; Notes preservou os 18 caracteres
minúsculos e o acréscimo `AN`. Título e notas permaneceram exatos após fechar,
reiniciar o processo e reabrir. Os exports reais da versão 7 foram salvos pelo
compartilhamento nativo: PDF contém o título e PPTX contém título e notas exatos.
A inspeção independente também renderizou o PDF sem corte. Recibo delimitado:
`/private/tmp/zoen-ios-deck-input-v230/native-result-v230.json`.

A correção permanente de configuração iOS está em `59dcf887`: 1.366 testes,
tipos, Oxlint/Oxfmt e os prebuilds reais isolados passaram. Só a configuração
Release do widget muda; os demais 33 arquivos iOS gerados permanecem idênticos.
Essa prova de geração não executa novamente o build ou o widget.

O teclado do editor ainda exige um gesto adicional; IME, seleção/delete/undo,
acessibilidade, aparelho físico e alteração pelo agente com modelo real seguem
pendentes. A leitura independente do arquivo canônico não foi executada. A
conversa `Teste Zoen` permanece vazia, com compositor vazio e teclado fechado;
os outros históricos foram preservados. Esse lote não aprova o editor completo
nem altera as três flags globais. Os resultados anteriores abaixo mantêm seu
escopo original.

No Release v238 de `5c2212a0`, a instalação preserva os dados e confere todos os
452 arquivos e a assinatura. O núcleo configurado passa; passam também 1.370
testes do app, tipos e Oxlint/Oxfmt. A suíte Rust configurada passa 574 casos do
client-core e três de registro em `7ef8e1f3`; depois muda somente uma fixture
TypeScript do frame nativo, com esse delta verificado antes do reuso da prova.
O binário inclui a recuperação de cursor durável do PR 150.

A captura nativa do primeiro toque mostra cursor e seleção de texto, com o
TextAgent ativo antes do `touchend`, mas sem teclado de software. São 12
observações numéricas v2 não saturadas sobre o mesmo WASM imutável do v236.
Elas distinguem eventos Pointer, Touch e foco; não comprovam o proprietário
interno do gesto nem a causa do teclado ausente. O fluxo diagnóstico mantém a
verificação de teclado que falhou e não tem aceite de replay. Recibo:
`/private/tmp/zoen-ios-contact-runtime-v238/native/native-result.json`.
O simulador retorna à conversa `Teste Zoen` vazia; os três chats e dois Apps
existentes continuam presentes. A correção do primeiro toque segue pendente.

O lote de fonte v240 transfere a liberação de um contato Pointer/Touch pareado
para Touch sem emitir outro Down. A suíte também encontrou um Up omitido após
duplo/triplo clique em texto: a seleção ficava em arraste e bloqueava a ativação.
O reparo entrega esse Up físico preservando a seleção de palavra/parágrafo e os
guardas de proprietário, cancelamento e arraste. Passam 259 testes reais de UI;
quatro testes longos/screenshot preexistentes permanecem ignorados. A falha
anterior (257 passaram, um falhou) permanece registrada. A receita reproduz os
621 arquivos, com as mesmas allowlists e dependências: dez testes Python e três
Node passam. O Release WASM locked/offline passa em 389,305 segundos, com audit
dos assets e fonte idêntica à testada. Passam também os 1.370 testes do app,
tipos, Oxlint/Oxfmt em `e9f86fe4`. Os novos assets ainda não mudam os pins do
runtime nem o iPhone instalado; o primeiro toque continua sem aceite. Recibo:
`/private/tmp/zoen-deckcraft-recipe-v240/result-01/composition-proof.json`.
Qualificação do build:
`/private/tmp/zoen-deckcraft-v240-build-receipts/qualification.json`.

O [PR 151](https://github.com/EnzoTironi/zoen-native/pull/151), de limites de
tempo e tamanho no transporte de aprovações, foi integrado em `773a4e6f` após
11/11 checks e ausência de conflitos no head `d895215c`. Isso não qualifica
um provedor real nem a execução nativa dos agentes.

O [PR 144](https://github.com/EnzoTironi/zoen-native/pull/144) foi integrado
em `ec28d8bc`. Suas evidências abaixo mantêm o escopo original. A fonte iOS
`bd10bf35` cobre 1.039 casos no lote e repetição isolada:
1.038 passaram no lote; um helper Swift excedeu o prazo de compilação e passou
na repetição isolada da mesma fonte. A execução ampla conserva o resultado de
falha; tipos, Oxlint/Oxfmt e build Release passaram. A instalação preservou os
dados e conferiu os 430 arquivos e a assinatura. O simulador comprovou X/Pin em
todos os cards, organização no Início, cartões canônicos no chat e retorno do
catálogo sem espaço extra. O Release07 corrigiu as coordenadas nativas dos cards,
permitiu arrastar o card secundário durante edição e preservou a ordem C,B,E após
reiniciar. Entrar em edição pressionando um card secundário ainda apresenta falha
intermitente e permanece sem aceite.

O registro do Signal contém 51 famílias de cenários, com escopos sobrepostos,
extraídas de seis repositórios fixados. Dois novos testes originais do Zoen já
estão integrados: equivalência completa do snapshot restaurado com mídia e
continuidade após entrada inválida sem confirmação falsa. Os 543 testes da biblioteca
client-core e o Clippy estrito de todos os alvos passaram em `4f175f40`. Os status parciais e as evidências anteriores permanecem.

O caso original C17 também está integrado: downloads históricos mistos usam o
worker real, rejeitam ciphertext corrompido pelo hash assinado e recuperam o item
correto em nova tentativa, preservando cache, IDs e timeline. O teste focado e o
Clippy estrito passaram. Isso não qualifica toda a política de mídia do Signal;
suas suítes upstream continuam sem execução.

DeckCraft já tem persistência canônica/MCP, registro, carregador verificado e
UI/bootstrap nativos integrados. O primeiro artefato `0f30f530`, com 1.073 casos,
criou um cartão real e o preservou após reinício, mas ficou preso na abertura;
essa observação permanece no registro. O novo artefato `a974701b` passou por
1.094 casos, tipos, Oxlint/Oxfmt, núcleo nativo configurado e build Release.
Os 431 arquivos instalados e a assinatura conferem. O motor real abriu o mesmo
documento na WKWebView, confirmou seu estado salvo e voltou a abri-lo após
reiniciar o processo. O fluxo específico dessa conta foi gravado e reproduzido;
a primeira tentativa excedeu cinco segundos, e a repetição confirmou abertura
em 7,8 segundos. Desempenho, layout de desktop e tema claro no iPhone permanecem
sem aceite. A adaptação de toque/área segura está em preparação.

As correções originais de observadores de áudio, gravação e revisão têm prova
focada de 71 casos e revisão independente no
[PR 146](https://github.com/EnzoTironi/zoen-native/pull/146), ainda em rascunho.
A falha de CI na inicialização do Chrome, anterior às asserções do App, continua
registrada e não foi dispensada. O executor privado do DeckCraft passou pelos
casos de autoridade, aliases e bloqueio padrão; a jornada real completa por agente
continua pendente. Nenhuma chamada de modelo foi comprovada nessa jornada.

A jornada completa no iPhone, alteração por agente, recuperação, exportação e
compartilhamento continua pendente. Só depois desse aceite o padrão será repetido
nos demais editores ArtCraft. O detalhe atual está em `deckcraftIntegrationCheckpoint`,
preservando `iosContinuityCheckpoint` e seus artefatos anteriores no
[registro de aceite](../../apps/zoen/acceptance.json). Nenhuma dessas provas
parciais aprova o lançamento iOS ou as flags globais.

Na fonte do motor, o lote v235 passou em 346 testes de núcleo e 254 de interface,
Clippy estrito e verificação wasm32. Os cinco testes já ignorados de screenshots
e soak longo conservam esse estado. O gesto textual inicial, cancelamento,
seleção e o caminho real de IME têm controles negativos executados. A composição
também preserva a apresentação completa no replay, incluindo formatação,
hyperlinks, marcas de parágrafo, navegação e desfazer. A receita reproduziu os
621 arquivos da fonte testada byte a byte. O pacote atualizado compilou com
JS/WASM verificados. Recibo: `/private/tmp/zoen-deckcraft-input-batch-v233/source-qualification-v235.json`.
O check completo do app também passou: 1.366 casos, tipos, Oxlint e Oxfmt;
o artefato instalado agora é o Release v236 de `b691debe`, com a nova tupla
JS/WASM fixada e verificação dos 452 arquivos e da assinatura após instalação.
O pacote anterior permanece preservado como controle. A configuração permanente
do widget foi incluída no build, com extração real dos metadados AppIntents.

No documento QA230 existente, o primeiro duplo toque de edição abriu o teclado
de software, a digitação a 35 ms funcionou e o iOS corrigiu `teh` para `the`.
O título exato `v236 the QA222 Native Title` permaneceu depois de fechar,
reiniciar o processo e reabrir o mesmo documento. O toque simples só selecionou
o título e continua sem aceite; a hipótese de perda do contato no ganho de foco
não foi atribuída ao evento nativo sem capturar sua família e ordem de foco.
Recibo: `/private/tmp/zoen-ios-deck-input-v236/native-result-v236.json`.
Esta prova não cobre Notes ou novas exportações, todos os caminhos de IME,
acessibilidade ou aparelho físico. O percurso gravado é diagnóstico, com
recuperações manuais e coordenadas de canvas, sem aprovação de replay.
A conversa Teste Zoen foi deixada vazia, sem rascunho nem teclado, preservando
as outras duas conversas e os Apps existentes. As flags globais seguem falsas.

O [PR 149](https://github.com/EnzoTironi/zoen-native/pull/149) foi integrado em
`ff303ec7`. O [PR 150](https://github.com/EnzoTironi/zoen-native/pull/150) recebeu
a correção do fixture de logs em `266c313d`: recuperação real com dois relays e
um único socket passou, e mudar somente JSON para texto reproduziu a falha do CI
anterior. As 11 verificações do novo head passaram e o PR foi integrado em
`00a008ba`, sem conflitos nem bypass, em 21 de outubro. Recibo:
`/private/tmp/zoen-ios-deck-runtime-v236/pr150-merge.json`.
A qualificação desse lote no núcleo do iPhone permanece pendente.
