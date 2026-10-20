# Integração dos editores ArtCraft no Zoen

Começar pelo DeckCraft e concluir uma apresentação real dentro da conversa antes
de repetir o padrão nos outros editores. Reutilizar seu motor, comandos e
exportadores; o Zoen continua responsável pelo documento criptografado, identidade,
permissões, versões, sincronização e histórico de pessoas e agentes.

## Checkpoint de integração — 20 de outubro de 2026

O carregador gerenciado, registro e UI nativa agora usam o documento binário
canônico do Zoen. O pacote fixa JS/WASM, fontes e licenças por hash; o host verifica
os recursos antes de entregá-los ao sandbox. A WebView continua sem autoridade de
armazenamento durável. O build e a suíte do app na fonte `0f30f530` passaram,
incluindo 1.073 casos e conferência dos 431 arquivos instalados no simulador.

A criação pelo catálogo nativo produziu o cartão “QA160 Shared deck”, com um
slide, e o mesmo cartão reapareceu após reiniciar o processo. O editor ficou
preso na abertura por mais de cinco minutos após buscar JS/WASM: **edição,
rotação, exportação e compartilhamento nativos permanecem sem aceite**. O teste
foi diagnóstico, sem fluxo automatizado qualificado. Uma falha de startup que
deixava requisições esperando indefinidamente foi identificada no código; não
foi estabelecido que ela causou essa observação. A correção está em preparação.

A correção integrada `c5ac7652` mantém a tela bloqueada após um timeout de save,
até receber a confirmação de recuperação. Doze testes com o SDK oficial cobrem
confirmações tardias, Close e exportação, sem reproduzir falha no WASM/aparelho.
O executor isolado de agente, ainda privado, compila o motor real para WASI e
passa em três testes de autoridade. Sua primeira jornada atingiu o prazo durante
a tradução do componente, antes da execução. Ainda não há prova de alteração por
agente real nesse documento do iPhone, nem chamada de modelo nessa prova.

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

| Prova executada | Resultado e alcance |
| --- | --- |
| Motor nativo com revisão e lockfile fixados | PASS em 247,3 s. `text.set` e `slide.notes` alteraram a mesma Session; undo/redo preservou os estados esperados. A segunda alteração foi programática, sem agente real. |
| Formato nativo | Modelo idêntico após salvar e reabrir os 4.631 bytes. Isso não comprova recuperação no banco criptografado do Zoen. |
| PPTX | Exportação de 25.129 bytes, reimportação e inspeção independente do ZIP/XML preservaram título e notas. |
| PDF | Exportação de 45.142 bytes; página renderizada e texto do título conferidos separadamente. As notas não fazem parte desse PDF de slides. |
| Interface WASM | O check passou em 91,587 s. O pacote Release otimizado também passou: WASM de 35.840.639 bytes, JS de 155.070 bytes e HTML de 1.622 bytes. A execução na WebView ainda não foi realizada. |
| Repetição do pacote | Duas execuções Trunk fixadas/offline produziram os quatro recursos idênticos e ZIPs idênticos de 14.355.956 bytes. A segunda usou cache Cargo na mesma máquina; não é uma reconstrução hermética em máquina limpa. |

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

| Lote | Saída necessária |
| --- | --- |
| Pacote e iPhone | Pacote reproduzível, fonte/fontes verificadas, ponte na mesma Session e controles de telefone. Renderizar, editar texto, usar teclado/VoiceOver/pinch, alternar retrato/paisagem e medir memória/tempo na WebView real. |
| Persistência e isolamento | Bytes exatos após reabertura; ACK perdido, mutação repetida ou adulterada, base antiga, perda de acesso, chunks ausentes/corrompidos, documento excessivo/futuro e falhas transacionais. Encaminhamento usa chaves novas e não mantém referências ao chat de origem. |
| Pessoas e agentes | Edição humana aceita, comando de agente certificado no mesmo artefato, autoria verificável e nova versão. Conflitos não sobrescrevem trabalho; tarefa retomada não duplica efeitos. Undo local e restauração durável têm semânticas explícitas. |
| Jornada completa | Conversa → criar apresentação → cartão → editar → alteração por agente → fechar → reiniciar processo → recuperar o mesmo documento → exportar PPTX e PDF da versão escolhida → compartilhar. Conferir os arquivos independentemente e repetir falhas/cancelamentos. |
| Próximos editores | Somente após o aceite do DeckCraft, reutilizar o contrato de pacote/identidade/versão/permissão e criar adaptadores específicos. |

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
