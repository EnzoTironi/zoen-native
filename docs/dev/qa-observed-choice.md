# Clef para uma decisão de UI observada

O QA #345 já usa Argent como driver e gravador. O helper
[`scripts/qa-observed-choice.mjs`](../../scripts/qa-observed-choice.mjs) implementa
somente o guard de uma decisão incerta. Não conecta a um dispositivo ou provider,
não instala o app e não substitui o YAML gravado. Os testes usam drivers e respostas
sintéticos; não comprovam UI, latência, custo ou ganho de velocidade real.

O gate continua sendo o artefato integrado elegível e a janela exclusiva do operador
em #345/#361. O build `f2ee8e14` é build-only; a sua existência não libera esse gate.

## Ponte com o driver existente

Importar `routeObservedAction` no host do runner. Fornecer os callbacks já usados no
percurso, sem chamar ferramentas por shell ou adicionar outro driver:

```js
await routeObservedAction({
  objective, expectedScreen, candidates,
  driver: {
    ready: checkExistingArtifactAndOperatorGate,
    observe: readSanitizedRelevantNativeControls,
    execute: executeThroughExistingArgentRecorder,
  },
  decide: callAskClefThroughHostToolBridge,
  verify: assertCurrentNativeEffect,
});
```

Esses nomes representam pontos de integração do host, não funções já disponíveis no
repositório. Não existe ainda um runner nativo conectado a este helper. O operador
precisa mapear o driver autorizado depois do handoff; não preencher os campos com
uma árvore inventada nem copiar IDs dos testes.

- `ready(action?)`: confirma o gate/owner atual; antes da execução recebe a ação
  selecionada para conferir seu escopo autorizado. Somente `true` permite continuar.
- `observe()`: retorna `id`, `sequence`, `artifact`, `device`, `lease`, `screen`,
  `idle`, `summary` e `nodes`. Cada leitura real recebe ID e sequência novos. `lease`
  vincula a janela exclusiva, conta/device e sessão/foco; muda quando algum desses
  donos é retirado, mesmo que a tela e os IDs pareçam iguais.
- Cada nó contém `id`, `role`, `label`, `visible` e `enabled`, obtidos da observação
  atual. Enviar apenas controles relevantes à decisão e uma descrição curta da
  tela; excluir relógio, timeline e mensagens sem relação com a escolha. Campos
  brutos extras não entram na consulta. O host deve remover dados privados e
  credenciais antes da chamada; preferir contas e conversas sintéticas.
- `candidates`: lista autorizada pelo percurso com `id` local, `operation`
  (`tap` ou `long_press`) e `targetId` de um nó realmente observado. Não há execução
  por coordenadas ou seletor retornado pelo modelo. Sem alvos atuais habilitados,
  o guard abstém antes de gastar inferência.
- `decide(request)`: chama **uma vez** `ask_clef` por meio da ferramenta do host.
  O request segue o contrato `state` + `questions.next_action` de tipo `choice`,
  com `insufficient_evidence`. Aceita a resposta direta ou o `structuredContent`
  real do MCP. Erro, timeout, resposta inválida ou abstinência não viram um tap e
  não disparam retry automático. Todas as probabilidades ficam no recibo.
- `execute(action, scope)`: usa o driver/gravador existente e resolve `targetId`
  na observação `scope.observationId`, com o mesmo artefato/device/lease. O próprio
  driver precisa rejeitar uma janela retirada ou um alvo que mudou entre a última
  leitura e o comando. O helper não pode tornar atômica uma API nativa externa.
- `expectedScreen`: identidade exclusiva de destino definida pelo percurso,
  independente do modelo. Pode ser a mesma tela em uma ação local.
- `verify({before, after, action})`: executa asserções determinísticas de conteúdo,
  valores ou efeito. Retorna uma lista não vazia de `{name, passed, observationId}`;
  `passed` é booleano e `observationId` deve apontar para a leitura **depois** da
  ação. Identidade do destino e `idle === true` também são obrigatórios no guard.

Depois da escolha há uma nova leitura: artefato, dono, tela e controles relevantes
precisam coincidir. Uma mudança exige nova evidência, sem repetir a pergunta para
forçar uma opção. Depois de uma única execução há outra leitura e asserções. Uma
exceção do driver continua falha mesmo se a ação tiver chegado e os checks passarem;
ela não pode ser apagada por recuperação ou por confiança alta.

`assertions_passed` significa somente que os checks desse passo passaram. O QA de
produto continua exigindo destino+idle, revisão de evidência visual quando prevista
e duas execuções consecutivas do mesmo fluxo gravado. Rotas já estáveis não devem
consultar Clef a cada replay nem regravar o YAML por run.

## Verificação local

```sh
node --test scripts/tests/qa-observed-choice.test.mjs
```

Sem Cargo, rede, modelo ou dispositivo. Os casos cobrem membership, escopo/foco,
leitura repetida, alvo alterado/oculto/desabilitado, abstinência, falha do provider,
confiança alta com efeito errado e resposta perdida depois de execução. O mesmo
comando está ligado ao CI. A integração real segue pendente do lote elegível.

## Adaptador concreto do contrato Argent — #392

`routeArgentAction` em `scripts/qa-argent-choice.mjs` compõe o guard acima. Ele não
abre conexão, inicia servidor, instala, lança app ou inicia/reinicia uma gravação.
O host importa a função dentro da janela de QA já autorizada e injeta a ponte de
ferramentas e os mesmos gates/asserções que já usa:

```js
await routeArgentAction({
  objective, expectedScreen, decide: callAskClefThroughHostToolBridge,
  candidates: [{
    id: 'open_attachment', operation: 'tap',
    selector: { identifier: observedStableId, role: observedRole },
  }],
  argent: {
    callTool: existingArgentToolBridge,
    ready: checkExistingArtifactAndOperatorGate,
    readScope: readCurrentArtifactDeviceAndRetirementLease,
    identifyScreen: identifyActualNativeDestination,
    redactLabel: sanitizeRelevantControlLabel,
    recording: existingActiveRecording,
  },
  verify: assertCurrentNativeEffect,
});
```

Os callbacks pertencem ao host; não são novas funções de autorização disponíveis no
repositório. `readScope()` retorna `{artifact, device, lease}` do manifesto e dono
atuais. `identifyScreen({description, source})` usa os marcos/asserts nativos do
cenário para retornar `{screen, summary}`; não pode identificar uma tela pela sua
posição no loop ou pela opinião do modelo. `summary` deve ser curto, sanitizado e
restrito ao estado relevante. `recording` tem os campos reais `{name, project_root}`
de uma gravação **já ativa**; o projeto é absoluto. O adaptador não inicia nem
recomeça essa gravação, evitando truncar o YAML existente.

O formato foi conferido no pacote local **Argent 0.29.0**, no contrato
`describe/format-tree.ts`, projeção `ios-device/describe`, contrato gerado de
`flow-add-step` e `flow-step-definitions.ts` (fontes incorporadas ao pacote). O
`describe` público retorna `{description, source}`, com linhas de texto, atributos
opcionais `id`/`value`, flags e frames normalizados. Não retorna a árvore JSON
inventada nos primeiros exemplos do guard. O parser local aceita esse formato e
separa somente os controles permitidos; valores, timeline e relógio não são
projetados para o Clef. `redactLabel` é obrigatório e não recebe o documento inteiro.
Respostas MCP usam `structuredContent` ou um único bloco JSON textual; erro,
resultado ambíguo ou formato desconhecido não viram controles.

Cada candidato deve ter uma operação `tap` e um `identifier` exato ou `label` exato
com `role`. Seletor somente por role não entra. Zero correspondências, flags
`disabled`/`password` e frame sem área não geram opção; duplicação não é resolvida
por ordem. O `targetId` é um fingerprint **local da observação**, não um ID nativo
inventado ou seletor persistente. Ele inclui os campos brutos do controle e seu
frame, então trocar label/identidade/estado/posição invalida a escolha. Esses campos
brutos e coordenadas não entram na pergunta ao modelo.

A observação usa `await-screen-idle` e `describe` reais, conferindo o escopo antes e
depois. Após a escolha há a releitura exigida pelo guard e outra releitura no
próprio despacho. O adaptador confere novamente o gate e a lease antes de chamar
**uma vez** `flow-add-step`, com `command: gesture-tap` e `args` JSON contendo apenas
o device autorizado e o centro do frame atual. O modelo não fornece argumentos de
ferramenta. Conferir o recibo real `toolResult`, `recorded` e `savedTo`: somente um
`tap` gravado com o mesmo `id` ou texto exato é aceito. Retarget, seletor só por role,
ponto cru, warning ou falha de escrita preservam a falha, mesmo que o efeito tenha
chegado. Não repetir o toque para tentar obter outro recibo.

A superfície não garante atomicidade entre processos nem prova oclusão por uma
árvore que não a expõe. `visible` significa área positiva no frame reportado. O
host continua responsável pelo gate da janela exclusiva, overlays, prontidão e
asserts visuais/estruturais previstos no cenário. O recorder deriva e grava o
seletor pelo seu próprio mecanismo; o adaptador não escreve outro YAML nem
reescreve o fluxo estável. O resultado de um passo continua sem substituir as duas
passagens completas de QA.

### Limite de actionability observado no driver

A projeção `xcuitest-runner` recebe `XCUIElementSnapshot.isEnabled` e grava o estado
negativo como `disabled`, então a ausência dessa flag tem um significado comprovado
nessa fonte. Os adaptadores atuais AX/native do simulador omitem esse estado. O
adaptador específico retorna `unsupported_actionability_source` nessas fontes;
não inventa `enabled: true`. Isso **não revoga a autorização para usar o simulador**:
o guard genérico continua disponível quando o driver/host fornece evidência real
de actionability pelo seu contrato. Esta entrega não implementa um fallback novo,
nem inicia servidor para contornar a indisponibilidade observada do serviço local.

O timeout de inferência é limitado a 90 segundos (ou menos, por passo). Ele produz
estado desconhecido, não retry; uma resposta tardia já cobrada não pode executar
nada. O timer local não afirma cancelar a chamada remota. `isError`,
`insufficient_evidence` e probabilidades continuam sob o contrato do guard.

Verificação sem provider, Cargo, build ou dispositivo:

```sh
node --test scripts/tests/qa-observed-choice.test.mjs scripts/tests/qa-argent-choice.test.mjs
```

Os testes usam texto sintético no formato do driver, não capturas nativas. O host
real ainda precisa importar o adaptador e fornecer gates, identidade, redator,
ponte de ferramentas e asserts reais depois do handoff elegível. Nenhuma execução
nativa, inferência, latência/custo ou ganho de QA está comprovado por esses testes.
