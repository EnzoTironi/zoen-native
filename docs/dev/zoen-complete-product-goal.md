# Meta completa do Zoen

Entregar o Zoen completo no iOS, Android, Web, macOS, Windows e Linux, incluindo
backend de produção, experiência refinada, privacidade, integridade dos dados e
operação comprovada sob carga. Preparar o produto para competir com WhatsApp,
Slack e Wabi em qualidade, confiabilidade e capacidade de crescimento, com
evidências reproduzíveis para cada afirmação.

Esta é a meta do produto inteiro. Sua conclusão exige os aplicativos finais,
o backend público e a operação de produção funcionando em conjunto. O registro
de critérios é [acceptance.json](../../apps/zoen/acceptance.json).
As evidências anteriores continuam com seu escopo e seus artefatos originais.
Ampliar a meta não transforma provas parciais em aprovação do produto completo.

## Execução atual — 20 de outubro de 2026

A meta foi ativada novamente nesta conversa. O próximo marco é **Zoen iOS pronto
para lançamento**, com jornadas completas contra os serviços reais. As outras
plataformas permanecem no escopo final, em fases posteriores; não dividem agora
o trabalho de refinamento e validação manual.

| Lote                          | Entrega observável                                                                                                                                                                                                  | Critério de saída                                                                                                                                                                                                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Conversa e mídia no iPhone | Timeline contínua; teclado e carrossel sincronizados; respostas/threads/reação/menu; busca no compositor; fotos e vídeo sem recorte involuntário; voz compacta e edição; anexos com aprovação explícita.            | Selecionar, revisar, cancelar, reiniciar, recuperar e enviar no app real; destinatário independente recebe conteúdo e contexto corretos. Rolagem, gestos e acessibilidade preservam a conversa.                                                                                 |
| 2. Confiança e continuidade   | Apple/Google; conta, perfil e chaves automáticos; vínculo/revogação; offline/outbox; backup e recuperação; confirmações com significado preciso.                                                                    | Dois dispositivos independentes, perdas de resposta, reinícios e revogação não perdem operações confirmadas nem liberam histórico sem chaves/autorização.                                                                                                                       |
| 3. Segundo plano e chamadas   | Push real, navegação por notificação, extensão/lifecycle; chamadas de voz e vídeo com integração de sistema.                                                                                                        | Provas em iPhone físico com app fechado, bloqueio, permissão negada, rede interrompida e retomada; falha de push não perde trabalho durável.                                                                                                                                    |
| 4. Apps e agentes             | Apps MCP com capas uniformes em chat/Home, seleção/reorganização e atalhos removíveis/restauráveis; tela cheia com X flutuante, animação nativa e retrato/paisagem; Excalidraw real; conectores e agentes duráveis. | Estado do App persiste e sincroniza sem reiniciar ao girar; remover um atalho não apaga o documento; permissões/revogação são efetivas; tarefa de agente continua sem cliente aberto e reconcilia aprovações, efeitos e custo após reinício.                                    |
| 5. Criação compartilhada      | Páginas em blocos e planos, comentários/presença/histórico; Slides, Sites, Imagens e Arquivos; edição por pessoas e agentes.                                                                                        | Edição colaborativa e offline, conflitos/recovery, papéis e revogação, lixeira/restauração; publicação explícita e rollback de Sites; artefatos reabrem completos.                                                                                                              |
| 6. Lançamento iOS             | Comunidades/convites e permissões; Você/configurações; design de céu uniforme; PT/EN; distribuição assinada; backend público compatível.                                                                            | Instalar/atualizar no iPhone físico, executar as jornadas iOS aplicáveis com provedores reais, passar acessibilidade e falhas críticas, documentar limites e qualificar o perfil iOS/backend fixado antes da medição, incluindo segurança, abuso, recuperação/rollback e custo. |
| 7. Web e Android              | Reutilizar contratos, componentes adequados à plataforma e backend; completar a matriz de jornadas.                                                                                                                 | Execução real em navegador e Android, incluindo offline, recuperação, mídia, Apps/criação e interoperabilidade com o iPhone.                                                                                                                                                    |
| 8. Desktop e aceite final     | macOS/Windows/Linux; distribuição/atualização; operação e recuperação do produto completo.                                                                                                                          | Matriz final de plataformas, perfil de carga/custo medido, restauração/rollback e critérios globais aprovados com evidência reproduzível.                                                                                                                                       |

O lançamento iOS exige um perfil próprio de carga iOS/backend, declarado e
fixado antes da medição, com segurança, proteção contra abuso, recuperação,
rollback e custo. O perfil multiplataforma existente mantém seus critérios e
hash originais; disponibilidade mensal exige seus 30 dias de observação. Nenhuma
qualificação de lançamento iOS aprova essas alegações globais por aproximação.

A criação de Apps deve acontecer pelos agentes, a partir de um pedido comum na
conversa. SDK, HTML, React, WASM, manifestos e configuração de MCP ficam nos
bastidores. Reutilizar o [SDK e o contrato interno existentes](../../packages/zoen-miniapp-sdk/README.md),
com uma mesma capa nativa e navegação para todos os motores. O fluxo completo de
gerar, verificar, registrar e atualizar um novo programa por agente ainda precisa
de seu próprio aceite; criar uma instância do catálogo não equivale a essa prova.

A ordem é de entrega; trabalho independente do núcleo/backend pode avançar em
paralelo. Cada lote fecha front, núcleo e backend da mesma jornada. Subagentes
trabalham com escopos isolados e um responsável por fonte, dispositivos e serviços.

Implementar um lote coerente, revisar e validar em conjunto. Repetir build ou a
suíte ampla quando uma mudança/falha justificar; não repetir tudo após ajustes de
documentação. Preservar testes de contratos, durabilidade e permissões. Uma falha
de ferramenta ou de ambiente é registrada separadamente, enquanto o trabalho
independente continua. Uma prova não executada permanece pendente.

**Marco iOS aceito** exige as jornadas iOS aplicáveis e as capacidades de criação
previstas, front e backend completos, provedores/configuração reais, build
assinado e validação em aparelho físico. O registro de plataforma não autoriza
marcar as três flags globais como aprovadas antes do produto completo. Não há
meta indefinida de copiar todas as features ou a escala global de concorrentes;
o escopo verificável é a matriz de 30 jornadas, criação e perfil numérico declarado.

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

## Plataformas e arquitetura

A execução atual conclui o iOS antes de ampliar a validação manual para Web e
Android. Em seguida, as mesmas jornadas são qualificadas nessas duas plataformas;
macOS, Windows e Linux ficam para a fase posterior do produto completo. O marco
de lançamento iOS tem seus próprios critérios e não depende da entrega desktop.
Segurança, integridade e backend acompanham cada lote.

Compartilhar o modelo de produto e o núcleo Rust, usando React Native/Expo e
adaptações nativas adequadas a cada plataforma. Evitar bases nativas completas
independentes, regras duplicadas, estado durável concorrente e caminhos legados
sem uso. Recursos exclusivos de um sistema precisam de alternativas explícitas
e adequadas nas outras plataformas.

O backend deve executar os serviços Rust nativos, sem reimplementá-los para um
runtime específico de hospedagem. Cloudflare pode servir DNS, CDN, frontend
estático ou armazenamento compatível com S3. A escolha final de hospedagem deve
favorecer operação simples e o volume declarado, sem um gargalo global oculto.

Na fase atual, desenvolver e validar localmente com os serviços reais em
containers, persistência, migrations, arquivos criptografados, verificações de
saúde e recuperação após falhas. O ambiente local deve aproximar a composição de
produção. Sua aprovação não substitui as jornadas no backend público, exigidas
na entrega final.

## Produto completo

Preservar as 30 jornadas obrigatórias, incluindo chamadas, desktop e critérios
de operação de produção. A matriz de produto deve cobrir:

- Apple/Google com criação automática de conta, chaves e perfil; edição posterior
  do perfil e recuperação segura do histórico em novos dispositivos.
- Mensagens, respostas, threads, edição, exclusão, reações, encaminhamento, busca,
  recibos e histórico longo, com interações, animações e acessibilidade cuidadas.
- Fotos, câmera, vídeo, documentos, áudio compacto com revisão/edição e chamadas
  de voz/vídeo seguras, com tratamento de interrupções e ciclo de vida do sistema.
- Home, comunidades, convites, permissões, agentes com controles de acesso e
  orçamento, notificações e integrações nativas previstas para cada plataforma.
- Páginas, planos, colaboração, histórico, conflitos, exportação e restauração.
- MCP Apps com isolamento e capacidades verificadas; Excalidraw permanece uma
  prova concreta, e integrações externas precisam funcionar com permissões reais.
- Tema de céu, consistência em português/inglês, VoiceOver/leitores de tela,
  teclado, tamanhos de texto, contraste e redução de movimento.

A comparação com WhatsApp, Slack e Wabi deve identificar a versão, plataforma,
data, jornada relevante e evidência. Não basta usar suas interfaces como referência.

## Referência principal de interface e interações

Telegram iOS é a referência principal de interface e interações, por direção do
usuário em 20 de outubro de 2026. O [guia de referência](../product/telegram-ios-reference.md)
e o [catálogo consultável](../../output/telegram-feature-matrix-2026-10-20/index.html)
orientam conversas, composição e teclado, anexos, mídia, ações, gestos,
transições, navegação, perfis e ajustes. O [estudo de cinco superfícies](../product/telegram-ios-next-ui-study.md)
registra os pontos de entrada nos componentes atuais.

Na instrução posterior do mesmo dia, o usuário **selecionou a reprodução da UI e
das interações completas da conversa do Telegram iOS, com fidelidade visual**,
adicionando os recursos próprios do Zoen sobre essa base. Isso inclui o fluxo de
anexos, e supera a interpretação anterior de que toda a UI aguardaria seleção.
Executar o [contrato de reprodução da conversa](../product/telegram-ios-conversation-parity.md),
usando o código iOS fixado para medidas e comportamento, sem substituir essa
direção por uma releitura visual genérica. Preservar o guia e o catálogo originais
como pesquisa datada; a seleção posterior está explícita neste adendo.

O usuário ampliou essa seleção para **todo o sistema de design e todas as telas
do Telegram iOS**, incluindo perfil, edição, detalhes, compositor, ajustes e
personalização de temas. Home, Apps/cards, agentes e criação entram como extensões
do Zoen. Portar superfícies e animações em lotes grandes de fonte, preservando
estado e autorização, antes da validação nativa integrada. A aparência inicial
terá somente **Céu diurno** e **Céu noturno**, com modo automático; não oferecer
as antigas fases como presets adicionais. Registrar diferenças e lacunas reais,
sem chamar uma tradução parcial de reprodução pixel perfect concluída.

As 506 entradas em 23 áreas são pesquisa de referência, não 506 capacidades
selecionadas para o Zoen. Os caminhos fixados são pontos de entrada por área,
sem rastreamento do comportamento de cada feature. A seleção de toda a UI
não seleciona automaticamente todos os produtos comerciais e serviços desse
catálogo. Essa direção não aprova um
fork, TDLib, infraestrutura do Telegram ou mudança de framework. A autorização
separada do usuário para AGPL foi aplicada e está registrada em LICENSING.md;
as partes derivadas mantêm suas atribuições e termos próprios.
Reprodução com nossos componentes e eventual adaptação de código precisam de
avaliação específica de comportamento, dependências, licença e manutenção.

Preservar a identidade de céu do Zoen, os cards consistentes e Apps em tela cheia,
os agentes, as permissões e os contratos do núcleo/backend. Signal continua como
referência de confiabilidade e testes comportamentais; Notion e ChatGPT Spaces
continuam como referências para documentos vivos e criação. As trinta jornadas
existentes permanecem inalteradas; nenhuma paridade ou validação nativa decorre
da importação do catálogo ou da seleção da referência visual. Registrar as
diferenças visuais e comportamentais até suas provas no iPhone.

## Referência principal de confiabilidade

Signal é a principal referência de confiabilidade no app, núcleo e backend.
O [estudo com fontes fixadas](signal-reliability-reference.md) define seis
contratos: operações duráveis e confirmações reais; continuidade da conversa;
ciclo próprio da mídia; separação entre login, vínculo e recuperação; privacidade
por camada; notificações e chamadas integradas ao ciclo de vida do sistema.

A execução deve preservar as autoridades existentes e priorizar OSS mantido e
recursos nativos adequados. Comparações de código não aprovam segurança equivalente
ou desempenho. Adoção de bibliotecas exige compatibilidade, licença e manutenção;
nenhuma referência justifica outra base de estado durável ou uma reescrita do
backend sem necessidade comprovada. As diferenças de banco local, metadados,
recuperação e segundo plano permanecem explícitas até serem resolvidas e validadas.

## Contratos aprendidos nas suítes do Signal

Manter o [registro de cenários](signal-test-contracts.json) com fontes e versões,
estado inicial, sequência, falha introduzida, asserções e comparação com o Zoen.
Ler os corpos dos testes, parâmetros, fixtures e consumidores dos fuzzers;
inventário por nome não comprova cobertura. Classificar equivalente, parcial,
lacuna ou incompatibilidade com a arquitetura. Priorizar perda de dados,
duplicação, autorização, recuperação e mídia, com testes originais dos nossos
limites. A maior parte roda no núcleo/backend; reservar aparelho físico para
entrada, áudio/câmera/chamadas, segundo plano e comportamentos de plataforma.
As evidências de testes, fonte, compilação, simulador e aparelho físico permanecem
distintas. O levantamento inicial é parcial e deve continuar por lotes.

## Área de criação compartilhada

Em 20 de outubro de 2026, o usuário definiu Notion e as Páginas do ChatGPT Space
como referências para as Páginas vivas e ampliou essa direção para toda a área de
criação: Páginas, Slides, Sites, Imagens e Arquivos. A experiência deve usar os
componentes nativos e o design de céu do Zoen, integrada às conversas e aos Apps.
As trinta jornadas originais e a entrega final nas seis plataformas continuam
obrigatórias; esta ampliação acrescenta capacidades, sem converter provas antigas
em aprovação dos novos fluxos.

Pessoas e agentes autorizados devem trabalhar sobre os mesmos artefatos
persistentes. Páginas precisam de edição direta em blocos, colaboração, comentários,
presença, histórico e recuperação. Slides devem permitir criar, editar, ordenar,
apresentar e exportar; Sites precisam de edição, prévia, publicação explícita,
versões e rollback; Imagens precisam de geração, upload, edição com referências,
variantes e biblioteca. Arquivos reutilizam o armazenamento e as permissões do
produto. A área compartilhada inclui organização em espaços e páginas filhas,
itens recentes, busca com escopo, compartilhamento, funções, revogação, lixeira e
restauração.

A conversa junto ao artefato fornece contexto explícito ao agente, com autoria e
alterações revisáveis. O agente continua sujeito às mesmas permissões do artefato;
uma resposta em chat, uma interface estática ou um adaptador privado não comprovam
edição colaborativa ou execução durável. Conteúdo privado permanece no modelo de
criptografia do Zoen; publicar um Site é uma ação explícita sobre a versão escolhida.

O primeiro lote continua no iOS, com o núcleo Rust e os serviços reais. Captura de
texto e revisão de anexos são fundamentos, não a conclusão da área de criação.
As capacidades adicionais ficam registradas em `creationWorkspace` no manifesto
de aceitação e só podem ser aceitas com evidências das jornadas completas.

Referências públicas: [colaboração no Notion](https://www.notion.com/help/collaborate-with-people),
[Páginas do ChatGPT Space](https://learn.chatgpt.com/docs/space/pages) e
[agentes no Space](https://learn.chatgpt.com/docs/space/agents).

## Backend, privacidade e integridade

Completar autenticação real, diretório de contas/dispositivos, criptografia,
pacotes de chaves, sincronização, entrega, associação a conversas/comunidades,
arquivos criptografados, recuperação, notificações e agentes. Validar os clientes
contra o backend público implantado, além dos testes locais e adaptadores privados.

Preservar criptografia de ponta a ponta e documentar os limites de confiança.
Login com provedor, sozinho, não pode liberar histórico criptografado anterior.
Chaves, autorização, revogação, dispositivos vinculados, backups e isolamento de
miniapps precisam de validação adversarial e revisão de segurança.

Provar comportamento correto com offline, repetição, perda de resposta,
reordenação, concorrência, gravação interrompida, encerramento do processo,
revogação e recuperação. Dentro do modelo de falhas declarado, uma operação
confirmada não pode desaparecer nem produzir efeitos duplicados indevidos.
Corrupção deve ser detectada e apresentada sem inventar sucesso ou descartar
dados silenciosamente. Migrações de dados já implantados precisam preservar essas
garantias mesmo durante atualização e rollback.

## Escala e operação comprovadas

Antes da verificação de escala, definir metas numéricas e o perfil de carga:
conexões simultâneas, mistura de jornadas, taxas sustentadas/picos de mensagens,
eventos, mídia e chamadas, tamanho de histórico, dispositivos por conta e fila
acumulada offline. Definir também latências p50/p95/p99, disponibilidade, orçamento
de erros, objetivos de recuperação e custo por usuário/operação/faixa de uso.

Os testes precisam medir carga sustentada, picos, reconexão em massa, históricos
grandes, concorrência, degradação, limites de serviço e recuperação. Registrar
gargalos, capacidade efetivamente validada, consumo e custos. Um teste de carga
isolado não demonstra a escala global de outro produto.

O primeiro perfil numérico está em
[zoen-operating-envelope-v1.json](zoen-operating-envelope-v1.json): 200 conexões
ativas sustentadas, reconexão de 1.000 dispositivos, 20 gravações por segundo e
pico de 100, além de mídia, chamadas, latência, recuperação e orçamento. São metas
de engenharia declaradas antes da medição. Ainda não são capacidade aprovada.
Cada execução deve fixar o hash desse perfil e dos artefatos, registrar os limites
e repetir o cenário completo duas vezes. A disponibilidade mensal exige 30 dias
de observação; testes de falhas não substituem essa evidência.

Operação de produção exige observabilidade que preserve privacidade, alertas,
limites, proteção contra abuso, tratamento de pressão, diagnóstico e procedimentos
de incidente. Exercitar restauração, recuperação de desastre, atualização,
migração e rollback; medir seus resultados.

## Critério para encerrar a meta

Encerrar apenas quando todas as jornadas e critérios passarem nos artefatos finais
da matriz de sistemas/dispositivos/navegadores suportados, com o backend público
correspondente, CI reproduzível, builds assinados quando aplicável e distribuição
adequada. Incluir instalação, atualização, reinício a frio, recuperação, desempenho,
acessibilidade, segurança e operação sob o perfil de carga declarado.

`fullAppParityAccepted`, `productionReadinessAccepted` e `scaleReadinessAccepted`
permanecem `false` até haver evidência suficiente. Os limites conhecidos e as
falhas abertas continuam registrados. Build aprovado, frontend hospedado, interface
polida ou adaptador privado funcional são progresso, sem encerrar a meta completa.

## Checkpoint histórico — Web v136

O Release Runtime06 foi exportado do checkout real, com cache de transformação
isolado: 91 arquivos e 2.358 fontes intactas durante o export. Passa 27 checkpoints
reais em pt-BR, sem erros de navegador. O inglês atual não está qualificado: o
backend limitou a preparação do peer antes de abrir o navegador nas duas
tentativas, incluindo a repetição após o cooldown, registradas à parte. Os 54 checkpoints bilíngues do
Runtime04 pertencem a outro artefato, preservado como evidência histórica.

As três legendas preservam espaços, Unicode e quebras de linha; o peer recupera
os três arquivos exatos e seus textos uma vez. Reload mantém IDs renderizados e
IDs dos arquivos; a interface principal exclui a thread e a raiz mostra uma
resposta. A CLI não expõe metadados canônicos de reply/thread, vínculo
legenda-arquivo ou IDs das mensagens: continuam `NOT_PROVEN` no peer.

Passam Rust79, Node focado34/36, Clippy estrito e formato Rust2021. O export leva
19,68 s, sem alegação de desempenho. A revisão integrada passa nos890 testes sem
skips, tipos/Oxlint/Oxfmt em20,91 s; depois só foram atualizados metadados dos recibos.
O Runtime05 falha com dois donos de idioma; o Runtime06 usa um dono inicializado.
A causa de geração/cache/realpath permanece sem prova. Negativos anteriores e os
recibos em `/private/tmp/zoen-web-attachment-intent-v136/` ficam preservados.
Não há nova aceitação nativa, de produção ou escala, nem testes reais de falhas de
anexos no navegador. Seleções pré-envio seguem voláteis. As30jornadas, seis
plataformas, dez lacunas, três flags `false` e registros abaixo mantêm seu escopo.

## Checkpoint histórico — iOS v135

O iOS v135 agora preenche o topo e a borda inferior físicos do canvas, sem
faixa de céu, título do host ou faixa de salvamento. O Release corrigido tem
HBC `e18e756a`, corte de 27 fontes `c59951ed`. Detalhes e formulário de ferramenta
ficam livres da alça; a Lista preenche o quadro e separa os controles. A primeira
tentativa continua reprovada: a alça cruzava o título nativo Detalhes. O título
nativo vazio no layout central corrige essa sobreposição no segundo artefato.

O reinício de processo `86049 → 87340` reabre o canvas com um retângulo. O peer
confirma os mesmos 678 bytes/hash `74c2b72c` após o reinício e depois de todos os
comandos de fechamento observados. A conta sintética QA134 foi vinculada pelo
protocolo público ao backend local real; isso não aprova o backend de produção.
O primeiro seletor AX incorreto expirou em 15 s; os pixels mostram o canvas e
o seletor real do botão Retângulo passa. Não se infere travamento ou tempo
preciso de abertura dessa divergência.

A robustez do gesto continua pendente. O primeiro pull de 450 ms, sem momentum,
após o reinício deixa o canvas visível e não atende à espera de Home de 10 s.
Tocar a mesma alça fecha; em reaberturas seguintes, os pulls de 650 ms com
momentum e 450 ms sem momentum chegam a Home. A causa da diferença não foi
isolada e não houve correção especulativa do gesto. Os pulls em Detalhes e no
formulário chegam a Home. O pull final na Lista foi enviado, mas Home não foi
observada de forma independente antes do reinício; não recebe aprovação.
O pan vertical com a ferramenta Mão permanece uma prova parcial do primeiro
artefato, sem ser atribuído ao segundo. O percurso gravado tem avisos e uma
espera com papel incorreto; não é um replay aprovado. VoiceOver, dispositivo
físico, salvamento nativo retido/com falha e robustez dos gestos seguem pendentes.

Passam 886 testes do app e os 64 Excalidraw finais, sem skips. A primeira
execução Excalidraw teve 63 sucessos e uma falha: o teste iniciava a janela de
200 ms no spawn antes de o filho estar pronto para stdin. O controle com atraso
causal e o sinal IPC de prontidão corrigem apenas esse teste; o helper de produção
fica idêntico e a falha original permanece. O segundo Xcode reutiliza os 176
inputs nativos e seções Mach-O idênticos, mantém os 18 corpos de recursos e
assinatura estrita, sem repetir Pods, Rust ou CNG. A transição padrão do provider
gerado e o negativo inicial de fechamento dos inputs continuam registrados.
Não há alegação de igualdade com HBC/mapa suplementar para o segundo build.

O CI publicado `73c7574c` termina com 11 jobs bem-sucedidos e nenhum job falho ou
pulado. Recursos passam 62 + 52 testes sem skips, incluindo nove casos reais no
Chrome. O console API 28 registra 108 testes, zero falhas e dois skips; API 35
registra 106 sem skips e teardown bem-sucedido. Os XML JUnit não foram
reinspecionados nessa execução. Esse CI não qualifica as cinco mudanças iOS
posteriores nem o ajuste posterior de teste. O histórico `eff1f0dd` de nove
sucessos/duas falhas, o Android v134 e suas correções explícitas da classificação
AX/pixels permanecem intactos abaixo. As 30 jornadas, seis plataformas, dez
lacunas e três flags `false` continuam sem aprovação integral.

### Checkpoint preservado v134

O Android v134 tem prova limitada de layout, gesto e reabertura no Release QA
`ced05640`, corte de 25 fontes `e8bf8dfc`. Lista e Excalidraw separam os controles
da alça de 48 dp. Puxar para baixo fecha; movimentos para cima/laterais mantêm
aberto. Um retângulo vertical permanece no editor até o gesto seguinte na alça;
o peer confirma um retângulo, 678 bytes/hash `74c2b72c`.

A espera original AX de 60 segundos continua não atendida. Sua classificação
como editor travado foi excessiva: o PNG original do mesmo resultado e o PNG
tardio já mostram o canvas. O primeiro PNG preservado é de cerca de 73,2 s após
o toque; não demonstra abertura antes de 60 s. Recibos e negativos originais
permanecem imutáveis, com suplementos de correção explícitos.

Uma comparação separada usa o mesmo APK normal, sem instrumentação temporária,
e reinicia o processo `5118 → 5236`. A captura terminada em +11,025 s já mostra
canvas e retângulo; a árvore pública UIAutomator também contém os controles,
enquanto o Argent ainda descreve “Connecting”. O mecanismo dessa divergência
não foi isolado. O peer mantém os mesmos 678 bytes. Isso prova uma reabertura
limitada; não é medição precisa de TTI, teste TalkBack ou correção causal do app.
O APK anterior `ba780784` continua reprovado na área segura da Lista. gRPC/setup
e o primeiro seletor AX incorreto permanecem registrados.

SDK/estilos e oito pares HTML/manifesto foram regenerados e recompilados no
núcleo Android. Excalidraw `7860e6dd`/6.195.767 bytes não mudou. Passam 886 testes
do app, 17 causais de navegação, 52 do SDK e 28 JVM, sem skips nessas suítes.
Os 62 testes Excalidraw anteriores mantêm seu corte e recurso. O HBC igual é
esperado: os novos recursos estão no JNI. As duas fontes de diagnóstico foram
restauradas byte a byte; o artefato de diagnóstico não qualifica produção.

O iOS de 22 fontes abaixo preenche os limites da sheet UIKit, mas ainda deixa
a faixa de status com o céu subjacente. O pedido mais recente para preencher
também o topo físico exige outra apresentação e outro artefato, ainda pendentes.
O arquivo nativo iOS antigo também não qualifica os recursos SDK novos.

O CI anterior `eff1f0dd` termina com nove sucessos e duas falhas: Rust passa;
API 28 mantém timeout PageDraft sem causa isolada e dois skips de capacidade do
provider; API 35 passa 106 testes, mas o job falha no teardown offline. Nenhum
job foi pulado; isso não elimina os skips JUnit nem aprova o novo corte.
As 30 jornadas, seis plataformas, dez lacunas e três flags `false` permanecem.

### Checkpoint preservado v133

O Release iOS de 22 fontes passa canvas sem título, controles separados, borda
inferior sem céu nos cantos e pull-down nativo antes/depois de um novo retângulo
vertical, sem fechar durante o desenho. O peer confirma os mesmos 1.692 bytes/hash
`4a27a643` após reinício do processo `95269 → 96237`, com três retângulos visíveis.
O Android API 35 mantém seu Release de 18 fontes: Voltar, edição e reinício
`5325 → 5664`, com os mesmos 1.184 bytes/hash `2708d8dd` no peer. Não há novo
artefato Android de 22 fontes. IDs/geometria/estilo originais persistem;
`boundElements: null → []` impede afirmar igualdade integral dos elementos.

Os recibos separam as falhas anteriores de cantos, grabber, seletor de idioma,
AX/harness e monitor das provas atuais. A preparação iOS, os três frameworks
Release e os entitlements reais do app/widget estão qualificados; a corrida do
monitor após o filho terminar permanece negativa, com suplemento do build real.
Isso não aprova reboot do aparelho, falhas de save induzidas, performance,
acessibilidade, distribuição em loja ou paridade completa.

Passam 886 testes do app e 47 de recursos, sem skips, no corte de 22 fontes.
O recurso `7860e6dd` mantém 6.195.767 bytes e o arquivo nativo não muda.
As 11 aprovações CI de `70753102` pertencem ao commit anterior; o novo CI e o
pull-down nativo Android continuam pendentes. As 30 jornadas, seis plataformas,
lacunas conhecidas e três flags de prontidão `false` permanecem preservadas.
