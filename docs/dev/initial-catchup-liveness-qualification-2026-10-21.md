# Sincronização inicial sem progresso — qualificação de 21/10/2026

A conexão pode responder a Pongs indefinidamente sem concluir o primeiro `Sync`.
Antes desta correção, `CursorRepair` só recebia um prazo depois do primeiro
`SyncDone`; `flush` continuava impedido por `synced = false`. Uma conexão saudável
não demonstrava progresso do histórico nem liberava o trabalho local pendente.

Base: `5cb1a7f7ac14c4279c0c739c063d90f17f12d50c`.
Reprodução negativa: `d8c898d805e186609add48577147e27d43adae5f`.
Implementação qualificada: `d5c4d1f088baf6f56991b2afe966095f3dbed0d0`.

## Contrato

O mesmo `CursorRepair`, canal watch e guard do dispatcher passam a possuir o
prazo de **inatividade inicial de 60 segundos**. É uma política provisória do Zoen,
sem SLA medido e sem copiar os prazos do Signal. A primeira escrita de `Sync` e o
flush inicial estão dentro desse guard; não há outro scheduler, cache ou outbox.

Somente `Applied`/`Confirmed` **junto com aumento de `next_seq`**, observado antes e
depois do ingest sob o mesmo lock do Engine, renova a janela inicial. Pongs,
eventos repetidos/históricos, lacunas, assinaturas inválidas, respostas Accepted e
tráfego efêmero não renovam. Um prazo já vencido não pode ser reanimado por um
evento que encontrou o watchdog pronto. O primeiro `SyncDone` ordenado termina a
fase inicial e conserva a reconciliação periódica existente: intervalo de 60
segundos e orçamento de resposta de 30 segundos. Avanço posterior não estende o
orçamento periódico. Históricos que continuam avançando não recebem um limite
absoluto de duração.

O vencimento produz o `Exit::Retry` existente. O laço de conexão conserva seu
backoff com jitter e os mesmos cursores/outbox; somente um primeiro `SyncDone`
bem-sucedido reseta o backoff. Stop/retirement e uma sincronização inicial
incompleta abandonam o transporte sem iniciar outra espera de fechamento.

## Evidência executada

O teste negativo usa TCP/WebSocket reais em loopback efêmero, autenticação real
do cliente e um Engine com uma mensagem assinada pendente em SQLite. O relay de
teste omite o primeiro `SyncDone` e envia Pongs a cada 20 ms. Sem a correção, o
oráculo termina após 900 ms com **45 Pongs, zero Publish e a sessão ainda presa**.
O envelope pendente, o cursor 1 e a mensagem local permanecem iguais após uma
reabertura real do banco. Só então falha a asserção de encerramento; não é uma
falha de preparo do fixture.

A fonte corrigida passou nos cinco casos novos:

1. O mesmo caso retorna Retry pelo guard; zero Publish antes da sincronização,
   backoff preservado e outbox/cursor intactos na reabertura.
2. Pongs, duplicata local, histórico de outro Space, evento inválido, lacuna e
   typing continuam chegando. Requests parciais reais demonstram que as lacunas
   foram processadas, mas a sessão expira sem avançar nenhum desses prefixos.
3. Três páginas pequenas de eventos assinados, com pausas de 140 ms e janela de
   fixture de 240 ms, duram mais de uma janela e avançam até o cursor 7. Nenhum
   offline Publish sai antes do `SyncDone`. Depois dele, o envelope original é
   publicado, Accepted/Event/duplicata confirmam exatamente uma mensagem; na
   reabertura há oito eventos válidos, sete mensagens e outbox vazio.
4. Stop durante o primeiro catch-up encerra o responsável e mantém o mesmo
   envelope pendente e backoff.
5. O limite exato de expiração não renova; a primeira conclusão ordenada retira o
   prazo inicial, mesmo com sync parcial pendente; progresso posterior e resposta
   parcial não adiam a política periódica.

Os **nove testes periódicos existentes** também passaram, incluindo perda do
último evento, perda de Joined/histórico de novo Space, respostas ordenadas,
fechamento suspenso, Stop e retargeting do prazo. **Clippy estrito de todos os
alvos do client-core** e formatação dos quatro arquivos Rust passaram. Nenhuma
suíte upstream foi copiada ou executada.

Evidências persistentes:
`/Users/enzotironi/.codex/builds/zoen-initial-catchup-v259/`.
`red-02` é o negativo executado; `green-02` e `periodic-01` são as execuções
positivas; `clippy-01` é o Clippy. Comandos, hashes das fontes, logs e resultados
ficam separados. `red-01` registra uma correção de tipo do próprio fixture;
`green-01` registra nomes de variantes incorretos no fixture. Essas tentativas
não contam como prova funcional. `source-ready.json` sela os hashes e commits.

## Protocolo, custo e limites

`ServerFrame::Event` não distingue replay de evento ao vivo, `SyncDone` não tem
request ID e o Sync inicial usa `all: true`, podendo descobrir Spaces que o
cliente ainda não conhece. Portanto, **avanço canônico genuíno em qualquer Space
renova a inatividade do stream**. Não é detecção de conclusão por Space: avanço
contínuo em um Space pode esconder outro Space estagnado. Duplicatas e históricos
sem avanço de outros Spaces estão cobertos; não foi inventado um frontier ou
protocolo de tagging para resolver o limite restante.

O relay de produção pagina 500 eventos por leitura e já limita operações do log a
cinco segundos. O teste usa páginas pequenas atrasadas sobre o protocolo real;
não executa esse paginador de produção nem o cluster PostgreSQL/FDB/NATS. O custo
adicionado é bookkeeping O(1) e duas consultas locais a `next_seq` por Event sob o
lock já existente. Não há benchmark de histórico grande, frequência de watch,
CPU, bateria, suspensão móvel ou latência/SLA. O guard cancela awaits cooperativos;
não é preempção de CPU nem garantia contra bloqueio síncrono do Engine.

Não foram executados reconexão automática de ponta a ponta, relógio de produção
por 60 segundos, MLS, media HTTP, cluster, perda de energia, aparelho físico,
UI/núcleo nativo recompilado ou suíte completa do workspace. As provas de Retry,
backoff e Stop são do cliente de produção com política curta de teste. Este lote
não encerra S04, a auditoria Signal ou a aceitação de produto em escala.

## Reproduzir o recorte

```sh
cargo test --locked -p client-core --lib --jobs 2 \
  net::tests::initial_catchup -- --nocapture --test-threads=1
cargo test --locked -p client-core --lib --jobs 2 \
  net::tests::cursor_repair -- --nocapture --test-threads=1
cargo clippy --locked -p client-core --all-targets --jobs 2 -- -D warnings
```
