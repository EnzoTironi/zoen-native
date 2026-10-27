## Execução atual — 26 de outubro de 2026

O próximo marco é **Zoen iOS pronto
para lançamento**, com jornadas completas contra os serviços reais. As outras
plataformas permanecem no escopo final, em fases posteriores; não dividem agora
o trabalho de refinamento e validação manual.

### Prioridade confirmada — 26 de outubro de 2026

Enzo definiu e confirmou o foco exclusivo em **agentes e conversação**. Concluir
as telas e interações selecionadas do Telegram iOS, com o backend correspondente,
antes de retomar Apps. São jornadas completas de mensagens, compositor, anexos e
mídia, voz, busca, grupos, perfis, configurações/temas, notificações e chamadas.
Agentes devem executar trabalho durável, respeitar permissões/aprovações/orçamento
e recuperar interrupções com resultados verificáveis.

**Desenvolvimento e QA de Apps e criação estão pausados:** MCP Apps, expansão de
host/SDK, editores externos/internos, Páginas, Slides, Sites, Imagens e comentários
ancorados706. Preservar código, dados, ramos e evidências; a pausa não cancela as
capacidades finais aprovadas. Edição nativa de legenda/mídia da mensagem pertence
à conversação; integração de editores de Apps continua pausada.

A arquitetura segue contratos tipados por domínio, uma autoridade de estado e
adaptadores UI/SDK/CLI/MCP. Fortalecer as fronteiras no workspace existente;
extrações de repositórios e novas integrações de Apps aguardam sua retomada.
Implementar e validar em lotes verticais, com dono de integração e operador único
de QA. A medida de conclusão é a jornada aceita de ponta a ponta.

O [checklist completo](../roadmap/README.md) e o [painel de revisão](../roadmap/index.html)
ligam cada jornada às telas, lacunas e issues. O catálogo continua como revisão
datada de `477ac2f3`; suas imagens App718/Core707 são históricas.

Os PRs [#635](https://github.com/EnzoTironi/zoen-native/pull/635), [#636](https://github.com/EnzoTironi/zoen-native/pull/636) e [#637](https://github.com/EnzoTironi/zoen-native/pull/637) foram integrados após #632/#633. Os issues [#217](https://github.com/EnzoTironi/zoen-native/issues/217), [#615](https://github.com/EnzoTironi/zoen-native/issues/615), [#622](https://github.com/EnzoTironi/zoen-native/issues/622) e [#440](https://github.com/EnzoTironi/zoen-native/issues/440) foram fechados nos recortes de paginação, reconexão autenticada/recuperação da mesma aprovação, ordem sob rate limit e AutoSky no artefato original v544, respectivamente; não encerram jornadas amplas.

O App732 tem fonte `9f724c1e` e reutiliza Core/UniFFI genuíno de `c4b8f7ce` (F), que inclui #632, após igualdade dos inputs nativos. CLI de ciclo de vida `4ff0ca77` preparado, CLI G2 `f0a01c1f` efetivamente usado nas leituras canônicas/bytes da QA e relay `4057a0af`/schema40 mantêm proveniência própria. Build e assinatura passaram. A QA732 confirmou larguras nativas, empilhamento em fonte ampliada e um Cancelar preservando ambos os rascunhos, sem Enviar; o tamanho do texto foi restaurado. A [referência canônica do artefato](../../apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json) separa essas evidências; esse recorte não demonstra encaminhamento entregue, aceite físico ou jornada completa.

iPhone físico, provedor real na jornada completa, backend público e lançamento
continuam pendentes. Apps/criação permanecem pausados. O issue #170 registra
responsáveis e execução; contagens de código ou testes não encerram uma jornada.

Provas anteriores permanecem atribuídas às suas fontes: F724 registrou encaminhamento de texto/mídia uma vez cada, imagem idêntica de 1.268.382 bytes (SHA256 `5864d328…`) e rascunhos preservados; 727 passou busca global com resultados e abertura do destino no primeiro toque. Os recibos estão na referência canônica acima; não são retestes 732. O CLI 4ff ficou preparado/help-only, sem execução de fixture confirmada.

| Lote | Entrega observável | Critério de saída |
| --- | --- | --- |
| 1. Conversa e mídia | Timeline, teclado, busca, gestos, respostas/threads, menus, fotos/vídeo e voz compacta. | Revisar/cancelar/recuperar/enviar; destinatário independente recebe conteúdo e contexto corretos; leitura e acessibilidade preservadas. |
| 2. Agentes — em paralelo ao lote 1 | Pedido, execução durável, permissões, orçamento, aprovação e resultado atribuído. | Fechar clientes e reiniciar executor sem repetir efeito; reconciliar aprovação, resultado e custo pela mesma autoridade. |
| 3. Confiança e continuidade | Apple/Google, chaves, perfil, vínculo/revogação, backup e recuperação. | Dispositivos independentes e falhas não perdem operações confirmadas nem liberam histórico indevido. |
| 4. Segundo plano e chamadas | Push privado, destinos autorizados, voz/vídeo e integração de sistema. | Prova física com bloqueio, app encerrado, permissões negadas e reconexão; falha de push não perde trabalho durável. |
| 5. Apps — pausado | Host/SDK, dependências externas MCP, cards uniformes, tela cheia e orientação. | Ao retomar: pacote fixado, persistência, atualização independente, isolamento e revogação; remover atalho não apaga documento. |
| 6. Criação — pausado | Páginas, planos, Slides, Sites, Imagens e Arquivos compartilhados por pessoas/agentes. | Ao retomar: edição, conflitos, recovery, histórico, papéis, exportação; publicação explícita e rollback. |
| 7. Lançamento iOS | Comunidades, Você/configurações, temas, PT/EN, distribuição e backend público. | Todas as jornadas iOS aplicáveis, provedores reais, acessibilidade e instalação/atualização físicas; perfil iOS/backend medido, segurança, abuso, recuperação e custo. |
| 8. Web e Android | Mesmos contratos e jornadas com controles adequados à plataforma. | Runtime real, offline, recuperação, mídia, criação e interoperabilidade com iPhone. |
| 9. Desktop e aceite final | macOS/Windows/Linux, instalação, atualização e operação. | Matriz final, envelope medido, restauração/rollback e critérios globais comprovados. |

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

A execução segue a [ordem de engenharia do AGENTS.md](../../AGENTS.md#engenharia):
questionar requisitos, eliminar partes dispensáveis, simplificar, acelerar e
automatizar. A solução deve concretizar a visão completa com uma estrutura
reutilizável e uma única autoridade de estado. As 30 jornadas, capacidades de
criação e critérios de lançamento permanecem no escopo aprovado.

Cada lote fecha front, núcleo e backend da mesma jornada e usa um issue, um
responsável e um registro final conciso de mudança, testes e limitações. Frentes
independentes avançam em paralelo. Implementação, checks adequados, QA por lote
e revisão/merge formam o caminho de entrega; decisões rotineiras já autorizadas
prosseguem sem novas rodadas de ACK. Repetir build ou suíte ampla exige mudança,
falha ou dúvida material. Preservar testes de autorização, durabilidade e
recuperação; registrar falhas de ferramenta separadamente e manter pendente a
prova ainda não executada.

**Marco iOS aceito** exige as jornadas iOS aplicáveis e as capacidades de criação
previstas, front e backend completos, provedores/configuração reais, build
assinado e validação em aparelho físico. O registro de plataforma não autoriza
marcar as três flags globais como aprovadas antes do produto completo. Não há
meta indefinida de copiar todas as features ou a escala global de concorrentes;
o escopo verificável é a matriz de 30 jornadas, criação e perfil numérico declarado.

### Evidências iOS preservadas

[Registro integral de 21 de outubro](zoen-complete-product-ios-evidence.md), com
seus checkpoints, falhas, limites e recibos; nenhum aceite é ampliado.

