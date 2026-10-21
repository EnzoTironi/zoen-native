# Recuperação de uma entrega ao vivo perdida — qualificação de 21/10/2026

O log durável continua sendo a autoridade. Uma conexão aberta e Pongs saudáveis
não demonstram que todos os eventos chegaram ao cliente. O Zoen agora reconcilia
os cursores aceitos após o primeiro catch-up, mesmo quando não chega outro evento
que revele uma lacuna.

Fonte da implementação: `288cec096b011f6e59f412888dc08fde6c2c42b0`.

## Contrato e implementação

- `net/cursor_repair.rs` mantém a posição da única reconciliação periódica na fila
  ordenada de respostas `SyncDone`; respostas de catch-up parcial não a concluem
  nem adiam sua próxima execução.
- A política própria do Zoen usa 60 segundos entre reconciliações e um orçamento
  de 30 segundos a partir do vencimento. Pongs não renovam esse orçamento.
- O mesmo socket e a autorização existente transportam `Sync { all: true }` com
  os cursores locais aceitos. Não há nova tarefa, protocolo ou dependência.
- O watchdog envolve o dispatcher inteiro, inclusive esperas de envio/flush. A
  limpeza respeita o prazo corrente; Stop ou transporte abandonado não inicia
  outra espera de fechamento. O primeiro catch-up, antes de qualquer SyncDone,
  ainda conserva seu comportamento anterior e não tem esse prazo periódico.

Esses números são uma escolha do Zoen para qualificação, não constantes copiadas
nem uma promessa de latência ou capacidade operacional.

## Provas executadas

A suíte integrada do client-core passou em **558 testes**: esta mudança e a
correção independente de preferências locais em contas vinculadas estavam juntas.
Nove regressões de cursor cobrem ordenação, duplicatas, perda do último evento,
perda de Joined e de todos os eventos de um novo espaço autorizado, prazos,
retargeting e descarte do transporte. Os nove casos também passaram após a restauração da fonte. A formatação do
workspace e o Clippy estrito de todos os alvos de client-core/zoen-cli passaram.
Nenhuma suíte do Signal foi executada ou
copiada para obter esse resultado.

O teste `journey_cursor_repair_cluster` usa PostgreSQL, FoundationDB e NATS reais,
dois relays criados da fonte atual e dois clientes reais com contas sintéticas.
Antes de armar a falha, espera as duas trocas canônicas de chave de perfil, a
confirmação real da mensagem inicial, o mesmo prefixo aceito e os clientes
assentados. Depois perde exatamente um Event entre o segundo relay e o cliente.

| Execução | Resultado observado |
| --- | --- |
| Positiva | Reconciliação a 60.002 ms do SyncDone inicial, um único socket saudável, uma repetição do evento final e cursor salvo avançado uma vez. |
| Controle negativo | Omitir somente o agendamento após o primeiro SyncDone falha na asserção de recuperação ausente. O mesmo socket recebe Pongs naturais até 100 segundos; não há Sync nem replay após a perda. |
| Positiva restaurada | Código restaurado, recompilado e cenário novo: reconciliação a 60.004 ms; histórico SQLite completo igual ao log FDB verificado, sem duplicatas. |

A branch independente da correção de preferências também passou no mesmo teste
real, após recompilar a fonte isolada.

Não são enviados conteúdo posterior, Joined, publicação pelo destinatário,
refresh ou reconexão para provocar a recuperação. IDs, sequências, hashes,
conteúdo, cardinalidade e cursor persistido são conferidos. O traço anterior ao
Stop proposital é separado do traço de limpeza.

As primeiras tentativas ficam registradas como falhas, sem serem convertidas em
aprovação: resolução local da biblioteca FDB; nível de log que ocultava o sinal
de readiness; troca de perfil ainda pendente antes da falha; e interpretação de
log textual onde o produtor usa JSON. O harness final aguarda o preparo real e
exige dois IDs distintos de relay com fan-out NATS nos logs JSON.

### Correção do fixture após o CI

O job Rust `39538544331/119020735478` falhou na contagem desses IDs:
esperava dois e encontrou zero. O fixture herdava `LOG_FORMAT` e `RUST_LOG`,
mas interpretava somente JSON; o relay emite texto por padrão. Agora os dois
relays recebem explicitamente JSON e nível INFO. A exigência de dois IDs NATS
distintos permanece intacta; o log é preservado antes da asserção, inclusive
quando ela falha.

O cenário real passou com o processo pai configurado para texto e logs
desativados: reconciliação em 60.003 ms, um socket, um frame perdido, um replay
e uma linha local. A leitura independente dos logs confirmou dois nós NATS.
Alterar somente o formato dos filhos de JSON para texto reproduziu exatamente
a falha de contagem zero, depois das verificações de recuperação. A fonte foi
restaurada ao hash da execução positiva. Formatação e Clippy estrito do alvo
também passaram. Evidências: `/private/tmp/zoen-cursor-ci-v234`, incluindo
`positive.json`, `negative.json`, os logs e o resultado canônico. O novo CI
continua pendente; a falha anterior permanece registrada.

## Reprodução e limites

Com `ZOEN_TEST_PG`, `FDB_CLUSTER_FILE`, caminhos da biblioteca FDB e
`ZOEN_NATS_URL` apontando para serviços de teste, execute:

```sh
cargo test --locked -p client-core --lib net::tests::cursor_repair -- --test-threads=1
cargo test --locked -p zoen-cli --test journey_cursor_repair_cluster \
  final_wire_loss_recovers_with_real_cluster_and_same_healthy_client_socket \
  -- --exact --nocapture --test-threads=1
```

O teste cria banco/cell/portas próprios. `ZOEN_CURSOR_REPAIR_EVIDENCE_DIR` pode
indicar um diretório novo; sem ele, o teste escolhe um caminho temporário único.
Falhas preservam os fixtures e os traços. O CI existente fornece os serviços e
executa esse teste dentro da suíte; a correção do fixture exige um novo CI verde.

O recorte é perda de um frame **depois** da entrega NATS. Não comprova perda no
broker, todas as corridas da inscrição ao vivo do servidor, MLS, anexos, suspensão
móvel, perda de energia, bateria ou custo de varrer históricos grandes. A prova
nativa com o núcleo recompilado também permanece pendente. Não muda as flags de
aceitação do produto nem conclui a comparação S04 com Signal.
