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
ligam cada jornada às telas, lacunas e issues. Os PRs #631, #632 e #633 estão
integrados; a fonte combinada atual é `477ac2f3`. O lote #632 concluiu a persistência
de resultados conhecidos e a recuperação de ferramentas aceitas pelo recibo
canônico, além da correção MLS. O lote #633 integrou carrossel, envio nativo,
galeria, menus, áudio, Markdown, temas e controles de agentes.

O App718 instalado tem fonte `b7ab2265` e reutiliza o Core707 `eb62cdaf`; ele ainda
não contém o Core de #632. Seus testes e observações de Simulator conservam essa
fronteira. Os casos de criação concluídos antes da pausa permanecem históricos.
iPhone físico, provedor real na jornada completa, backend público e lançamento
continuam pendentes. O issue #170 registra responsáveis e execução; contagens
de código ou testes não encerram uma jornada.

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

