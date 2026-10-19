# Execução da meta completa — checkpoint v102

Este checkpoint registra avanços verificáveis no código e nos adaptadores locais.
A meta continua sendo o [produto completo](zoen-complete-product-goal.md), com as
30 jornadas em iOS, Android, Web, macOS, Windows e Linux, backend público e operação
de produção. `fullAppParityAccepted`, `productionReadinessAccepted` e
`scaleReadinessAccepted` continuam `false`.

As provas abaixo pertencem aos arquivos e artefatos identificados em cada recibo.
Foram produzidas sobre a árvore de trabalho a partir de
`4da0d9f759ea59dce1f7c2476f7ff89b96c44d39`; não representam uma nova versão pública
ou uma aprovação conjunta de todas as plataformas.

## Implementado e verificado neste checkpoint

| Entrega                                                   | Evidência delimitada                                                                                                                                                                                                                                                                                          | O que ainda exige integração e aceitação                                                                                                                                                                                   |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Google nativo no Android                                  | O app usa o contexto `android`, registro OAuth próprio e apresentador obrigatório no backend. Foram aprovados 801 testes do app com TypeScript, Oxlint e Oxfmt; 19 testes da política de inscrição; 20 cenários SQL anteriores e 3 novos cenários Android; 40 testes Workerd em cópia congelada dos arquivos. | Registro Android no Google Cloud, configuração pública app/servidor, migração de produção e entrada real no app Android assinado. Apple nativo permanece iOS; OAuth continua sem liberar histórico criptografado anterior. |
| Planejamento compartilhado de sincronização               | O relay nativo e WASM compartilham a política Rust de visibilidade durante atualização do histórico. Passaram 7 testes Rust e 8 vetores protobuf nativos em Node e Workerd, executados por um teste em cada runtime.                                                                                          | Índice de participação, projeções MLS, leitura paginada, autorização atual de cada entrega, recibos, fanout e sincronização pública completa.                                                                              |
| Transferência criptografada de histórico                  | A política Rust compartilhada e o adaptador privado R2 preservam admissão, chaves de objetos, criação condicional, retomada, revogação e exclusão pelo prefixo exato. Passaram 31 vetores de HTTP nativo e 7 testes Workerd.                                                                                  | Endpoint público, R2 remoto nesta nova implementação, associação/recuperação de dispositivos e jornada completa de transferência de histórico nos clientes finais.                                                         |
| Compatibilidade de mídia e I/O limitado                   | Após compartilhar orçamento de mídia/transferência e controle de corpos, passaram 7 testes HTTP nativos, 28 vetores e 16 testes Workerd. O teste de timeout de armazenamento aguardou o prazo real de 15 segundos.                                                                                            | Capacidade e custo de produção, quotas personalizadas e novas jornadas públicas ou em dispositivos finais.                                                                                                                 |
| Isolamento da compilação usada pelos testes de integração | A fixture passa a usar diretório de compilação próprio, evitando sobrescrever metadados de dependência utilizados por doctests. A fixture real `journey_m1` compilou com `--no-run`.                                                                                                                          | Nova execução de CI e execução das jornadas. Compilar a fixture não prova que ela iniciou um relay ou concluiu uma jornada.                                                                                                |

O controle de I/O agrupa fragmentos em buffers próprios de 64 KiB, limita leitores
em espera e compartilha a mesma concessão de memória entre mídia e transferência.
Um timeout de R2 retorna falha e mantém novos leitores bloqueados até a operação
original terminar, pois a chamada de armazenamento não pode ser cancelada. O
resultado de uma gravação que excedeu o prazo é desconhecido; a retomada deve usar
o mesmo chunk. Esses limites privados não estabelecem escala de produção.

Os testes Android de autenticação usam assinaturas RSA reais com uma chave
exclusivamente sintética. A primeira tentativa do novo cenário reutilizou um
subject sintético já associado a outra conta; o backend recusou corretamente com 409. Foi corrigido somente o subject da fixture, preservando a política do produto.
O banco descartável foi encerrado e removido; os serviços protegidos nas portas
5547 e 8790 não foram alterados.

## CI auditada e falhas mantidas visíveis

A auditoria refere-se ao commit `4da0d9f7`, anterior às novas alterações acima:

- [Expo 39253948698](https://github.com/EnzoTironi/zoen-native/actions/runs/39253948698):
  mídia aprovada com 6 testes HTTP nativos, 28 vetores e 13 testes Workerd; Web com
  22 checkpoints em cada idioma, inglês e português, sem erros do navegador.
  O Directory continua com 32/34 testes aprovados e duas falhas de encerramento
  real de sockets ociosos. Os testes e os prazos continuam exigentes.
- [Android 39253948550](https://github.com/EnzoTironi/zoen-native/actions/runs/39253948550):
  a suíte Android legada descobriu 106 testes por matriz. API 28 teve 104 aprovados
  e dois testes MCP ignorados; API 35 teve 104 aprovados e os mesmos dois testes MCP
  falhando. Essa suíte não aprova as novas jornadas do app Expo Android.
- [CI 39253948641](https://github.com/EnzoTironi/zoen-native/actions/runs/39253948641):
  a falha de metadados de doctest foi reproduzida e recebeu a correção local de
  isolamento de compilação descrita acima. A execução auditada continua vermelha;
  a correção ainda precisa passar na CI de uma nova revisão.

O trabalho de leitura do Directory v103 está **em andamento, sem aceitação** neste
checkpoint. Seus resultados serão registrados separadamente após verificação.

## Recibos e identificação dos artefatos

Os hashes foram conferidos diretamente nos arquivos locais, incluindo os
artefatos referenciados pelos recibos principais. Caminhos temporários identificam
evidências de desenvolvimento; não constituem distribuição ou implantação.

| Evidência                  | Recibo                                                                | SHA-256 do recibo                                                  |
| -------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------ |
| App e autenticação Android | `/private/tmp/zoen-android-auth-v102/acceptance.json`                 | `c6d6fa935c84d13e1f499e95166f553fa1214c924620d519d220c87c93c3d85e` |
| Política de sincronização  | `/private/tmp/zoen-sync-v102/acceptance.json`                         | `385100a3dd52bbbee41846fa86313182f4fab5dd99c37f9fa78f1b3ca8a0d09e` |
| Transferência              | `/private/tmp/zoen-transfer-v102/differential-03/summary.json`        | `1f38270e8c9c12a7094745fffd8c5924cd0e4d01623fc5fd20a466d2108dd646` |
| Compatibilidade de mídia   | `/private/tmp/zoen-transfer-v102/media-compatibility-01/summary.json` | `168c4548b2673238653b3e0952acf4d33d4560d21d0986cefa090fa686d57bbb` |
| Auditoria da CI `4da0d9f7` | `/private/tmp/zoen-ci-v102/acceptance.json`                           | `dbbc80bfb83d8f419ec742b312b6ee994e116f367fbb79f39ad56c36a79c0edf` |
| Compilação da fixture real | `/private/tmp/zoen-ci-v102/fixture-compile-acceptance.json`           | `b1e2801e364df54a893e7c5bda5f3b099c25a5095d94845a46035077f02f3204` |

O registro aditivo está em `evidence.productExecutionV102` no
[manifesto de aceitação](../../apps/zoen/acceptance.json). As evidências anteriores
permanecem com seus artefatos e limites originais. Backend público completo,
provedores reais, artefatos finais nas seis plataformas, integridade sob falhas,
acessibilidade, escala declarada e operação de produção continuam pendentes.

## Continuação verificada — v103

O Directory implementa agora `Lookup`, `Profiles` e `Devices`. O oráculo manteve
os 71 cenários nativos anteriores e acrescentou 70 passos de leitura com PostgreSQL.
Após a correção de leitura por cursor e limite agregado de 768 KiB, passaram
21 testes Workerd sem falhas ou casos ignorados. A recusa por capacidade é
repetível, sem inventar uma resposta parcial. A integração do orçamento nativo de
`ClaimKeyPackages` ainda precisa ser concluída; não há aprovação global dos
controles de abuso. O [contrato de leituras](../../infra/cloudflare/relay-state/DIRECTORY-READS.md)
registra os limites privados.

A base foi atualizada para Expo estável 58.0.15 e os quatro patches recomendados
pelo SDK. A verificação de dependências ficou alinhada e o Expo Doctor aprovou
20/20 verificações. O código Android de notificações FCM acrescenta configuração
explícita, criação de canal antes da permissão/token, rotação sem sobrescrita por
consulta antiga e wakes sem conteúdo. A entrada nas configurações Android usa a
mesma ação explícita de permissão. O check completo mais recente passou 813/813
testes, TypeScript, Oxlint e Oxfmt. Configurações Expo reais foram validadas com
cliente Firebase sintético temporário e com pacote incorreto recusado.

O conjunto integrado de adaptadores passou 83 testes antes do último ajuste de
capacidade do Directory. Esse ajuste tem os 21 testes finais acima; os recibos
preservam a sequência e os bytes de cada prova. Não há nova implantação pública,
novo binário nativo aprovado, entrega real de FCM ou sync headless aceitos.
As duas falhas locais de fechamento de sockets continuam abertas.

A auditoria de dependências reportou 31 nós do grafo associados a três avisos:
[braces](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm),
[node-forge](https://github.com/advisories/GHSA-86w9-cpqp-85rv) e
[uuid](https://github.com/advisories/GHSA-w5hq-g745-h8pq).
Os dois primeiros não têm versão corrigida publicada nos avisos consultados.
A análise de alcance e mitigação permanece pendente; nenhum downgrade forçado
foi aplicado ou aviso ocultado.

Recibos finais: `/private/tmp/zoen-directory-read-v102/final-acceptance.json`,
`/private/tmp/zoen-transfer-v102/final-compatible-01/acceptance.json` e
`/private/tmp/zoen-android-notifications-v103/acceptance.json`. O registro aditivo
`evidence.productExecutionV103` preserva seus hashes e os critérios completos
permanecem pendentes.
