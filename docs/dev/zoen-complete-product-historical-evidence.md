# Evidências históricas da meta completa

Registros datados preservados da [meta completa](zoen-complete-product-goal.md).
Provas históricas mantêm seu escopo original e não aprovam o produto atual.

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
