# Execução da meta completa — checkpoint v105

O backend nativo e o candidato Cloudflare compartilham agora as transições MLS,
a retenção do histórico cifrado e as regras de perfis criptografados. A meta do
[produto completo](zoen-complete-product-goal.md) continua ativa: as 30 jornadas,
seis plataformas e todos os critérios de produção permanecem exigidos.

## Conversas e histórico cifrados

O oráculo usa o núcleo MLS real e o log nativo FDB, comparados com Rust/WASM e
SQLite em Durable Objects. Passaram 302 transições criptografadas e 275 aberturas
de mensagens pelo dispositivo destinatário, além dos 17 cenários anteriores,
quatro recusas, oito testes Workerd e quatro contratos SQL.

Os cenários incluem Direct/Group, entrada de outro dispositivo, Welcome, disputa
de epochs, commit antigo, checkpoints, retenção e poda de mais de 256 mensagens.
A poda preserva sequência, hash e recibo canônico; uma falha tardia reverte a
transação. A célula FDB sintética foi removida e seu prefixo vazio foi confirmado.
O estado anterior de grupos fechados mantém seu formato versão 1.

## Perfis e orçamento de busca de chaves

`PublishAgreementKey`, `AgreementKeys`, `PutProfile` e `GetProfiles` usam a mesma
validação Rust nos dois servidores. O oráculo PostgreSQL manteve os 71 cenários
anteriores, acrescentou 76 passos de leitura/quota e 45 de perfis cifrados. A
prova inclui abrir bytes recuperados do banco, recusar outra chave/versão e
preservar assinaturas e versões maiores que a precisão inteira do JavaScript.

O servidor recusa versões fora do intervalo não negativo do `BIGINT` e usa conversão verificada,
evitando que um UInt64 grande se torne negativo e quebre a ordem das atualizações.
Zero continua permitido. Repetir a mesma versão e os mesmos bytes é aceito;
substituí-los na mesma versão ou voltar a uma versão anterior é recusado.

Pedidos inválidos de `ClaimKeyPackages` consomem o orçamento compartilhado antes
da validação específica, preservando estoque e recibos. Reconectar ou alternar
dispositivos não renova esse orçamento. O teste usa duas publicações MLS reais
separadas para estabelecer a ordem nativa de idade dos pacotes.

Passaram 123 testes Rust, Clippy rigoroso e 27 testes conjuntos Directory/leitura/
perfis, sem casos ignorados. Os testes de sockets continuam com 14/16: as duas
falhas conhecidas de fechamento ocioso permanecem visíveis, com o mesmo prazo.
O PostgreSQL descartável foi encerrado e removido. Os serviços protegidos não
foram reiniciados nem limpos.

Contratos: [MLS](https://github.com/EnzoTironi/zoen-native/blob/b4cffb7163888d34e47e470f6bb007672a1ed028/infra/cloudflare/closed-groups.md),
[leituras](https://github.com/EnzoTironi/zoen-native/blob/b4cffb7163888d34e47e470f6bb007672a1ed028/infra/cloudflare/relay-state/DIRECTORY-READS.md) e
[perfis cifrados](https://github.com/EnzoTironi/zoen-native/blob/b4cffb7163888d34e47e470f6bb007672a1ed028/infra/cloudflare/relay-state/DIRECTORY-PROFILES.md).

## Operação e CI

O [perfil numérico inicial](zoen-operating-envelope-v1.json) declara capacidade,
latência, falhas, recuperação e orçamento antes da qualificação. Define mistura
de mensagens/mídia/chamadas, ciclo mensal de custo, unidade decimal de GB e
amostras mínimas. Atraso de snapshot não autoriza perda de dados confirmados:
a restauração precisa recuperar também o diário durável. Os alvos não foram
medidos em produção; carga remota continua desabilitada.

A CI do commit anterior `e2a002cd` confirma Web com 22 checkpoints por idioma,
provedores, mídia, transferência, leituras e planejamento de sync. O cliente
Linux teve 811 testes aprovados e dois ignorados exclusivos de Darwin; a prova
local anterior de 813/813 mantém seu escopo. A auditoria verificou oito arquivos
de artefatos e 162 objetos de código contra o commit exato. As duas diferenças
de formatação que interromperam os jobs Rust foram corrigidas nesta revisão.
As falhas de fechamento de sockets e da matriz MCP Android não foram ocultadas.

A auditoria final confirma a compilação Expo Android Release e 14/14 testes JVM.
O APK tem 124.622.009 bytes, não é depurável e usa certificado Android Debug;
assinatura de produção e novas jornadas do app no aparelho continuam pendentes.
Cinco arquivos Android e 422 entradas de código foram verificados contra o commit.
Na suíte legada, API 35 teve 104 testes aprovados e dois MCP falhando; API 28 teve
104 aprovados e os mesmos dois ignorados. Cada API também passou seu teste
separado de TLS após reinício do processo.

Os avisos npm receberam triagem estática: o único uso instalado de UUID no Xcode
é `v4()` sem buffer, fora das APIs afetadas pelo aviso. O alcance de `braces` e
`node-forge` e suas mitigações ainda exigem revisão; não houve downgrade forçado
ou supressão. Isso não constitui aprovação global de segurança.

## Limites e recibos

Não houve nova implantação neste checkpoint. A composição pública de Directory e
Spaces ainda precisa de descoberta completa, visibilidade após remoção, paginação,
fanout ordenado e autorização no envio. Provedores reais, recuperação, push,
chamadas, artefatos finais, desktop e operação sob carga continuam pendentes.

- Recibo integrado: `/private/tmp/zoen-backend-v105/acceptance.json`.
- MLS final: `/private/tmp/zoen-mls-v105/final-integrated/summary.json`.
- Directory/perfis: `/private/tmp/zoen-claim-quota-v103/final-acceptance.json`.
- CI anterior: `/private/tmp/zoen-ci-e2a-audit-v104/audit-checkpoint.json`.
- CI Android concluída: `/private/tmp/zoen-ci-e2a-audit-v104/audit-completed-android.json`.

Os recibos preservam hashes de código, WASM, fixtures e resultados. O manifesto
[acceptance.json](../../apps/zoen/acceptance.json) acrescenta
`evidence.productExecutionV105` sem alterar evidências anteriores.
`fullAppParityAccepted`, `productionReadinessAccepted` e
`scaleReadinessAccepted` continuam `false`.
