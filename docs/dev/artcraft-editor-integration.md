# Integração dos editores ArtCraft no Zoen

Começar pelo DeckCraft e concluir uma apresentação real dentro da conversa antes
de repetir o padrão nos outros editores. Reutilizar seu motor, comandos e
exportadores; o Zoen continua responsável pelo documento criptografado, identidade,
permissões, versões, sincronização e histórico de pessoas e agentes.

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

## Viabilidade do DeckCraft

A revisão fixada é `a31b990d32f2c8562f1588562f922960c3fde48b`, versão 0.9.0.
O motor compartilha `Session::execute` entre interfaces e oferece operações sobre
bytes para abrir e salvar apresentações. A interface Web usa eframe/WASM, mas
precisa de uma ponte com o host e de controles adequados ao telefone. O servidor
MCP existente não declara uma interface MCP App pronta.
[Motor](https://github.com/storytold/deckcraft/blob/a31b990d32f2c8562f1588562f922960c3fde48b/crates/engine/src/lib.rs),
[comandos de arquivo](https://github.com/storytold/deckcraft/blob/a31b990d32f2c8562f1588562f922960c3fde48b/crates/engine/src/cmd/file.rs),
[interface Web](https://github.com/storytold/deckcraft/blob/a31b990d32f2c8562f1588562f922960c3fde48b/apps/deckcraft-web/src/web.rs).

| Prova executada                             | Resultado e alcance                                                                                                                                                                                                |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Motor nativo com revisão e lockfile fixados | PASS em 247,3 s. `text.set` e `slide.notes` alteraram a mesma Session; undo/redo preservou os estados esperados. A segunda alteração foi programática, sem agente real.                                            |
| Formato nativo                              | Modelo idêntico após salvar e reabrir os 4.631 bytes. Isso não comprova recuperação no banco criptografado do Zoen.                                                                                                |
| PPTX                                        | Exportação de 25.129 bytes, reimportação e inspeção independente do ZIP/XML preservaram título e notas.                                                                                                            |
| PDF                                         | Exportação de 45.142 bytes; página renderizada e texto do título conferidos separadamente. As notas não fazem parte desse PDF de slides.                                                                           |
| Interface WASM                              | O check passou em 91,587 s. O pacote Release otimizado também passou: WASM de 35.840.639 bytes, JS de 155.070 bytes e HTML de 1.622 bytes. A execução na WebView ainda não foi realizada.                          |
| Repetição do pacote                         | Duas execuções Trunk fixadas/offline produziram os quatro recursos idênticos e ZIPs idênticos de 14.355.956 bytes. A segunda usou cache Cargo na mesma máquina; não é uma reconstrução hermética em máquina limpa. |

A prova usou fontes locais de fallback, sem baixar os binários craft-fonts. A
fidelidade tipográfica, o teclado, VoiceOver, toque, GPU, memória e recuperação no
iPhone pertencem ao próximo aceite. O pacote Release posterior incorporou somente
as três fontes craft-fonts selecionadas pelo build, com hashes e OFL conferidos,
e os quatro fallbacks egui com seus avisos próprios. Isso não aprova toda a cadeia
de dependências ou sua fidelidade no telefone. A apresentação serializada não guarda o
undo/journal local da Session; undo durável e atribuído exige histórico do Zoen.

O WASM otimizado tem SHA-256
`5b1df0ccc851cf44894055c41440d964146be62ae1c19d4c228d24c281d12880`.
São 10.929.117 bytes em Brotli-9, uma medição de pacote, sem benchmark de rede ou
memória no iPhone. A memória linear declara 11.862.016 bytes iniciais e nenhum
máximo explícito; isso não mede o pico de processo. O módulo upstream exporta
main/memória/suporte wasm-bindgen, sem API de abrir, editar, salvar ou exportar o
documento pelo host. Ainda precisa da ponte na mesma Session e de um carregador
verificado de múltiplos recursos, sem elevar globalmente o limite de HTML.

A evidência reproduzível está em
`/private/tmp/zoen-artcraft-research-v147/supplement-release-package/manifest.json`;
ZIP `e131facc25b9ea5b6b75621c04546b0b50585985e79df0b389670adf04a4e6a0`.
O [contrato interno de autoria](../../packages/zoen-miniapp-sdk/README.md) mantém
a criação por agentes nos bastidores; a pessoa recebe o mesmo cartão e a mesma
experiência, independentemente do motor escolhido.

## Contrato de integração

Reutilizar o App Item criptografado, FileDoc, recibos de mutação, controle de
versão e isolamento já usados pelo Excalidraw. Extrair somente as diferenças
reais de formato do adaptador atual: MIME, validação, metadados compactos,
serialização e exportação. Não criar um armazenamento paralelo para DeckCraft.

1. Abrir somente o artefato autorizado, com versão, hash, revisão do motor e
   limites conhecidos. A WebView recebe uma projeção temporária desse documento.
2. Executar cada alteração uma única vez. Registrar o resultado canônico sob
   comparação de versão/hash e adotar exatamente esse resultado nos outros
   clientes. Comandos que usam relógio ou autoria não podem ser executados
   independentemente no navegador e no backend esperando resultados iguais.
3. Persistir bytes, chaves, evento, outbox e recibo na mesma autoridade existente.
   Um ACK perdido repete o recibo exato; uma base antiga produz conflito
   recuperável, preservando o trabalho local.
4. Derivar preview e exportação de uma versão imutável escolhida. Registrar hash
   de origem, motor, opções e hash do resultado. Compartilhar exige uma ação
   explícita sobre esse resultado.

A ponte atual de desenho usa JSON. O documento ZIP precisa de transferência
binária delimitada em chunks, com tamanho/hash, identidade de mutação e ACK;
não deve ser escondido como base64 dentro de `scene_json`. O carregador também
precisa de recursos WASM/fontes verificados e carregados sob demanda, com CSP por
App. Os limites existentes de HTML e mensagens não são orçamento para um motor
inteiro. A interface deve aceitar retrato e paisagem sem reiniciar a Session.

Usar o SDK oficial MCP Apps já instalado para handshake, recursos, ferramentas,
contexto do host e encerramento. Oferecer uma lista explícita de comandos
autorizados. Caminhos de arquivo, rede, plugins, provedores e fontes externas
exigem capacidades separadas. A autoria vem do usuário ou agente autenticado;
um campo `actor` enviado pela WebView não concede autoridade.
[Protocolo MCP Apps](https://modelcontextprotocol.io/extensions/apps/overview).

Validar também cada comando aninhado de `batch`/`run_command`. Operações de
fontes e dicionários podem alterar estado global do processo; bloquear apenas
uma função de varredura não isola leituras tardias ou registros compartilhados.
Usar fontes verificadas e limitar a autoridade do executor por artefato.

O carregador upstream pode reparar estruturas e perder campos futuros durante
uma regravação. Preservar os bytes originais e abrir versões incompatíveis em
leitura ou importar uma cópia com revisão explícita. Definir os limites de slides,
bytes descomprimidos, pixels, fontes, comandos, duração e memória a partir da
medição no iPhone; os limites de desktop não são os do Zoen.
[Formato nativo](https://github.com/storytold/deckcraft/blob/a31b990d32f2c8562f1588562f922960c3fde48b/crates/format/src/lib.rs).

## Lotes de implementação e aceite

| Lote                      | Saída necessária                                                                                                                                                                                                                                                     |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pacote e iPhone           | Pacote reproduzível, fonte/fontes verificadas, ponte na mesma Session e controles de telefone. Renderizar, editar texto, usar teclado/VoiceOver/pinch, alternar retrato/paisagem e medir memória/tempo na WebView real.                                              |
| Persistência e isolamento | Bytes exatos após reabertura; ACK perdido, mutação repetida ou adulterada, base antiga, perda de acesso, chunks ausentes/corrompidos, documento excessivo/futuro e falhas transacionais. Encaminhamento usa chaves novas e não mantém referências ao chat de origem. |
| Pessoas e agentes         | Edição humana aceita, comando de agente certificado no mesmo artefato, autoria verificável e nova versão. Conflitos não sobrescrevem trabalho; tarefa retomada não duplica efeitos. Undo local e restauração durável têm semânticas explícitas.                      |
| Jornada completa          | Conversa → criar apresentação → cartão → editar → alteração por agente → fechar → reiniciar processo → recuperar o mesmo documento → exportar PPTX e PDF da versão escolhida → compartilhar. Conferir os arquivos independentemente e repetir falhas/cancelamentos.  |
| Próximos editores         | Somente após o aceite do DeckCraft, reutilizar o contrato de pacote/identidade/versão/permissão e criar adaptadores específicos.                                                                                                                                     |

O núcleo/backend recebe testes em lotes. O aceite de teclado, gestos, áudio,
memória e ciclo de vida precisa da plataforma real, separando simulador de iPhone
físico. Compilação, preview ou a prova de motor acima não encerram essa jornada.

## Sequência dos Apps e bibliotecas

Primeiro grupo: DeckCraft, GridCraft, PhotoCraft, PdfCraft e WordCraft. Depois:
FilmCraft, SoundCraft, FigCraft e VectorCraft. Expansão: EffectCraft, DesignCraft,
LightCraft e componentes selecionados do ArtCraft. Os doze editores independentes
têm código Web; vários são alpha, e SoundCraft/DesignCraft se declaram pre-alpha.
ArtCraft é um workspace maior de desktop/provedores e precisa de uma seleção de
componentes própria. Revisões: [registro fixado](artcraft-upstream-pins.json).

Adicionar bibliotecas somente quando usadas pelo App integrado. Para o primeiro
recorte, manter o `craft-pdf` de escrita que DeckCraft já fixa, sem ativar um leitor
desnecessário ou atualizar silenciosamente craft-libs. craft-atomic não substitui
transações/chunks do Zoen e sua implementação WASM é indisponível. craft-media e
craft-cms ficam vinculados a necessidades comprovadas; adaptadores provisórios de
craft-ai-integrations preservam consentimento, credenciais, orçamento e execução
do Zoen. Distribuir somente as fontes/fontes necessárias, com seus avisos.

## Licenças e colaboração

Os manifests dos doze editores declaram MIT OR Apache-2.0 para o código. Isso não
aprova automaticamente dependências, fontes, assets, modelos ou marcas. Preservar
avisos e revisões, retirar marcas upstream da distribuição modificada e auditar o
subconjunto efetivamente empacotado.
[Termos da marca DeckCraft](https://github.com/storytold/deckcraft/blob/a31b990d32f2c8562f1588562f922960c3fde48b/docs/brand/LICENSE-brand.txt).

Excluir contas, armazenamento e servidores colaborativos upstream. FigCraft
documenta arquivos de colaboração em plaintext e acesso por link/código; essa
implementação não fornece a E2EE do Zoen.
[Privacidade do FigCraft](https://github.com/storytold/figcraft/blob/d9c924c3ba7114dff9141c667aec56d3764f3af9/README.md#privacy-and-online-sharing).
O NOTICE do ArtCraft exclui determinados áudios, imagens, skyboxes, movimentos e
renders de terceiros da licença aberta; esses assets ficam fora do pacote. Pesos
de modelos não são baixados ou distribuídos automaticamente.
[Avisos do ArtCraft](https://github.com/storytold/artcraft/blob/40e04793b504147de97ba1f0659d3975167524e5/NOTICE).
