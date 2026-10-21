# Ordem de publicação de anexos — qualificação isolada de 21/10/2026

O contrato reaproveitado do levantamento Signal é a preservação da ordem e dos
intentos após falhas, sem copiar números de retries ou introduzir outra fila.
Esta entrega corrige uma perda de projeção executada no Zoen: uma mensagem
confirmada antes de seu File ou de seu predecessor no álbum permanecia no log,
mas desaparecia da conversa inclusive após uma reabertura fria.

## Falhas reproduzidas

- `red-01` e `red-02`: o primeiro membro de um álbum sofre uma recusa transitória;
  os demais envelopes oferecidos são confirmados em ordem de chegada. O outbox
  termina vazio e o destinatário retém todo o log verificado, mas a reabertura
  SQLite mostra somente um dos dois membros. `red-02` é o fixture adaptativo;
  a versão positiva acrescenta também a reabertura do remetente antes do retry.
- `red-single-02`: um envio público de um único anexo com apresentação `file`
  tem seu File recusado uma vez e depois confirmado. A mensagem também é
  confirmada, mas não aparece após reabrir o destinatário. O primeiro ensaio,
  `red-single-01`, falhou na preparação porque o modo comum compatível `media`
  mantém `media=None`; esse ensaio não é evidência da falha de publicação.
- `red-terminal-file-01`: a recusa permanente da criação do File faz a mensagem
  aceita desaparecer da projeção local em vez de conservar um intento falho.
- `red-ws-terminal-01`: depois de recusar definitivamente um membro, a sessão
  real não publica o próximo texto comum antes de outra confirmação ou Flush.
  O teste falha nessa asserção, após a notificação efetiva da recusa.

Os logs de falhas de compilação/preparação também foram preservados; não contam
como controles negativos funcionais.

## Correção e limites

`sync.rs` usa `PlanReplay` e `State.apply_checked` sobre o log confirmado para
admitir mensagens com metadados de mídia. Um File otimista ou apenas escrito no
socket não libera sua mensagem. Álbuns exigem também o predecessor exato. O mesmo
predicado de dependências continua compartilhado com a admissão da projeção;
a autorização de escrita permanece no caminho já existente.

Recusas permanentes atualizam o outbox numa única transação: o File não confirmado
falha suas referências assinadas do mesmo autor/Espaço; um membro falha os membros
pendentes do mesmo grupo, contagem, apresentação e resposta. Uma criação duplicada
recusada não invalida um Item canônico. Assinaturas inválidas, outros autores,
Espaços, grupos e referências incompatíveis não ampliam essa propagação. A falha
na escrita de um membro reverte a operação inteira. Os intentos assinados e os
bytes legíveis continuam disponíveis como recuperação, sem ganhar autoridade para
nova publicação. `net.rs` libera os próximos heads imediatamente após `UserFailed`.
Não foram alterados exclusão de mensagens confirmadas, voz, mensagens legadas ou
APIs de retry/descarte. Uma ação pública de recuperação de envio falho ainda é
uma lacuna separada; os demais File/bytes não são apagados por esta propagação.

Há um replay completo por Espaço relevante e passagem de oferta, com memória
proporcional ao histórico confirmado. `state`/`space_projections` já contêm o
overlay pendente e não podem substituir diretamente esse recorte. Não há cache
persistente, segunda autoridade ou qualificação de latência/capacidade em escala.

## Resultado executado na fonte final

| Lote | Resultado |
| --- | --- |
| `cargo test --locked -p client-core --lib --jobs 2 media_contract_ -- --nocapture --test-threads=1` | 21 passaram; 0 falharam |
| `cargo test --locked -p client-core --lib --jobs 2 media_dependencies_ws_ -- --nocapture --test-threads=1` | 2 passaram; 0 falharam |
| `cargo test --locked -p client-core --lib --jobs 2 publish_window_ -- --nocapture --test-threads=1` | 2 passaram; 0 falharam |
| `cargo clippy --locked -p client-core --lib --tests --no-deps --jobs 2 -- -D warnings` | passou |

Os dois novos cenários WS usam o dispatcher e o transporte de produção, uma porta
loopback efêmera, um draft público real e dois dispositivos MLS independentes.
O relay do teste ordena os envelopes reais; não executa o servidor de produção.
O cenário transitório recusa File e primeiro membro, compara CID, assinatura e
ciphertext exatos em cada retry e exige ambos os membros/captions/índices na
SQLite fria do destinatário. Um marcador FIFO com Flush prova que `Accepted`
isoladamente não libera dependências nem muda a mensagem para enviada.

O cenário terminal conserva os dois intentos locais falhos, retém uma confirmação
já escrita e todas as respostas incidentais, espera o listener real da recusa e
exige a publicação do próximo texto antes do marcador sem enviar outro Flush.
A conversa remota fria contém o texto uma vez e nenhum membro recusado.

Essas provas não executaram o cluster relay/FDB/NATS, aparelho físico ou a suíte
completa do workspace; não demonstram throughput. Nenhuma suíte/código de teste
do Signal foi copiada ou executada. A integração e o build nativo cabem ao lote
principal posterior.

Evidências locais persistentes:
`/Users/enzotironi/.codex/builds/zoen-signal-album-order-v254/`, com comandos,
hashes de fonte, logs brutos e resultados. A fonte final está ligada a
`final-source-inputs-03.json`, `green-media-07`, `ws-media-05`,
`green-mls-window-05` e `clippy-05`; o recibo `source-ready.json` registra o
commit final e o intervalo completo a partir de `e12ddee2`.
