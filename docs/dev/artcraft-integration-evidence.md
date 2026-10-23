# Evidências delimitadas da integração ArtCraft

Voltar ao [contrato e plano dos editores](artcraft-editor-integration.md).
Estes registros preservam as revisões, os recibos e os limites originais; não ampliam o aceite atual.

## Pacote reproduzido — 21 de outubro de 2026

O registro fixa deliberadamente o candidato 04 abaixo, reproduzido a partir da
revisão upstream `a31b990d32f2c8562f1588562f922960c3fde48b` e da receita gerenciada
com `0017-eframe-ios-input.patch`. O rebuild independente recompilou o crate Web
final e os componentes próprios, executou bindgen/otimização e produziu JS/WASM
idênticos ao candidato 02. Reutilizou dependências externas em cache; não foi
um bootstrap hermético em máquina limpa. Os avisos e a origem das fontes foram
preservados, sem uma nova auditoria completa das dependências transitivas.

| Recurso | Arquivo                                  |      Bytes | SHA-256                                                            |
| ------- | ---------------------------------------- | ---------: | ------------------------------------------------------------------ |
| JS      | `deckcraft-web-1eb288115950a000.js`      |    157.851 | `4a3a583c958a150900c58e773c2ed58df6213e733ed3496e9c60c173cd9bebc4` |
| WASM    | `deckcraft-web-b257808a686003f1_bg.wasm` | 36.387.042 | `cb0af84197762083dc06f9b3812b6c60fe2bbbfeac534ff44c186081dd37b241` |

O WASM anterior `2c6ab9025a157f346849dcbb4e7cc956e2afbb6665048727654f60995eee24c6`,
de 36.387.050 bytes, continua ausente. A causa da diferença restante não foi
comprovada; **o novo pacote não é recuperação nem prova de equivalência daquele
binário**. O registro rejeita o tuplo antigo e combinações parciais. Revisão do
motor, ABI, JS, limites, permissões e autoridade canônica do documento permanecem
os mesmos. O manifesto padrão continua sem endpoint e indisponível.

Proveniência e os 36 testes de motor/ponte/input da reprodução estão em
`/Users/enzotironi/.codex/builds/zoen-deckcraft-repro-v249/candidate-qualification.json`.
O pacote selado fica em `sealed-candidate-04/` nesse mesmo diretório; o JSON tem
SHA-256 `c972da1a80f30c889e6d3ad7d08e8821822bbf96cc98e525459e6d856716169e`.
O novo manifesto completo deve ser gerado contra o HTML do build que o utiliza,
com o gerador existente e sem ampliar a rede do guest. A promoção dos pins não
ativa entrega HTTPS, confiança TLS ou um build nativo. Carregamento, teclado,
edição manual/por agente, persistência, reinício e exportação no iPhone com
**este** pacote ainda precisam de aceite; as provas antigas abaixo são históricas.

## Aceite nativo delimitado — candidato 04

O Release `e12ddee2` foi instalado preservando os dados, com os 415 arquivos e a
assinatura conferidos. Tipos, Oxlint/Oxfmt e 1.496 testes passaram. O manifesto
completo e a entrega HTTPS local contêm os hashes acima; a CA de QA foi adicionada
somente ao simulador designado, sem mudar a confiança do macOS.

No simulador iPhone/iOS 27.1, o documento existente **QA230 Input** abriu no motor
real. Um toque selecionou o objeto; o duplo toque seguinte abriu o teclado. A
inserção de 11 caracteres no título apareceu no canvas, foi salva e continuou
igual após fechar, terminar o processo e reabrir. Não é aceite de ativação textual
por gesto único, Notes, IME, seleção, delete ou undo completos.

Os menus reais abriram o compartilhamento nativo da versão editada 16 em PDF
(70.579 bytes, SHA-256 `28ef084afe21bbc755c802582cb93d4c45cbffb1ad300f784111389c65668366`)
e PPTX (25.070 bytes, SHA-256 `082a94ce5ed492568cb975a25fe4873272f5ce28eceec7c206b9199789bf5aae`).
Uma inspeção independente leu o título completo nos dois arquivos, confirmou um
slide/página e renderizou o PDF sem cortes aparentes. Cancelar removeu os arquivos
temporários do app; cópias somente dos fixtures de QA foram guardadas como prova.
Não houve envio a um destinatário externo, leitura independente dos bytes
canônicos, edição por modelo/agente real ou qualificação de aparelho físico.

Recibo: `output/telegram-attachment-port-v242/presentation-proof/runtime-15-batch.json`.
Inspeção: `presentation-proof/runtime-15-exports/independent-export-inspection.json`
no mesmo diretório. Orientações, acessibilidade, memória, desempenho, falhas de
save/conflito e o restante da jornada continuam pendentes. A integração completa
de ArtCraft e os indicadores de produto/produção/escala continuam **false**.

## Checkpoint de integração — 20 de outubro de 2026

O carregador gerenciado, registro e UI nativa usam o documento binário
canônico do Zoen. O pacote fixa JS/WASM, fontes e licenças por hash; o host verifica
os recursos antes de entregá-los ao sandbox. A WebView não tem autoridade de
armazenamento durável. A fonte `e16c755d` passou por 1.094 casos, tipos,
Oxlint/Oxfmt, núcleo nativo configurado e build Release. Os 431 arquivos
instalados e a assinatura conferem com o artefato lacrado.

A criação anterior produziu o cartão real “QA160 Shared deck”, com um slide.
O primeiro artefato ficou preso na abertura por mais de cinco minutos; a falha
permanece como evidência histórica. O novo pacote abriu o mesmo documento no
motor real da WKWebView, confirmou o estado salvo, fechou e reabriu após reiniciar
o processo. Um fluxo específico dessa conta foi gravado e reproduzido. A primeira
reprodução excedeu cinco segundos; uma repetição com orçamento explícito de
abertura mediu 7,8 segundos. Isso não aprova o desempenho. O layout de desktop e
o tema claro pertencem à evidência histórica `a974701b`. A versão posterior
`e16c755d` abriu o mesmo documento em 8,245 segundos com controles compactos,
tema escuro do host e fechamento seguro no canto. Em paisagem, ainda há
sobreposição no rodapé de status; a usabilidade completa não foi aprovada.

A correção integrada `c5ac7652` mantém a tela bloqueada após timeout de save,
até receber confirmação de recuperação. O pacote posterior também trata falhas
conhecidas de startup e remove o uso de PID indisponível em WebAssembly. Um
controle real no navegador abriu o seed canônico e outro, sem canvas, recusou
requisições pendentes e futuras. Isso não estabelece a causa exata da primeira
falha nativa nem cobre todos os traps possíveis.

Os menus do próprio editor abriram o compartilhamento nativo de PDF e PPTX da
versão1 original, sem edição. Foram conferidos a assinatura do PDF e o ZIP/XML
do PPTX, com um slide; os arquivos temporários foram removidos após cancelar.
Isso não aprova exportação de conteúdo editado, renderização independente do PDF
ou envio a um destinatário externo. Falhas nos oráculos de legenda ficaram
preservadas separadamente das observações e dos arquivos reais.

**A edição manual continua bloqueada pelo teclado da WKWebView.** Título e Notas
não receberam texto, embora a busca nativa do mesmo artefato tenha aberto o
teclado e aceitado o valor completo. Um controle com a política pública do
RN-WebView também falhou e foi revertido. A causa de foco ainda não foi
comprovada. Correções de aposentadoria do rascunho de Notas e layout em paisagem
estão preparadas, sem qualificação de execução. Acessibilidade semântica do
editor, memória e ciclo de vida também permanecem abertos.

O executor privado passou uma jornada real por Runtime, Gate, SpaceTools e
componente WASI, com comandos reversíveis, isolamento entre artefatos,
persistência canônica e replay após reabrir o cliente SQLite. O cérebro do teste
fornece chamadas fixas: **nenhum provedor/modelo real foi comprovado**, nem a
interoperabilidade com o iPhone. A tradução inicial do componente levou
444,553 segundos sob compilação concorrente; o orçamento do guest não limita
esse custo. A reabertura usa o componente já compilado no mesmo processo.
O executor ainda é privado e não integrado. A jornada completa do documento
editado e os demais editores Storytold seguem pendentes.

As medições de pesquisa abaixo preservam seus pacotes originais. Não descrevem
o pacote gerenciado posterior nem encerram a integração. Revisões, limites,
resultados e recibos atuais estão em `deckcraftIntegrationCheckpoint` no
[registro de aceite](../../apps/zoen/acceptance.json).
