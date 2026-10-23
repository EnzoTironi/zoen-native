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
