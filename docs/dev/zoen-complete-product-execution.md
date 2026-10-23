## Execução atual — 20 de outubro de 2026

A meta foi ativada novamente nesta conversa. O próximo marco é **Zoen iOS pronto
para lançamento**, com jornadas completas contra os serviços reais. As outras
plataformas permanecem no escopo final, em fases posteriores; não dividem agora
o trabalho de refinamento e validação manual.

| Lote                          | Entrega observável                                                                                                                                                                                                  | Critério de saída                                                                                                                                                                                                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Conversa e mídia no iPhone | Timeline contínua; teclado e carrossel sincronizados; respostas/threads/reação/menu; busca no compositor; fotos e vídeo sem recorte involuntário; voz compacta e edição; anexos com aprovação explícita.            | Selecionar, revisar, cancelar, reiniciar, recuperar e enviar no app real; destinatário independente recebe conteúdo e contexto corretos. Rolagem, gestos e acessibilidade preservam a conversa.                                                                                 |
| 2. Confiança e continuidade   | Apple/Google; conta, perfil e chaves automáticos; vínculo/revogação; offline/outbox; backup e recuperação; confirmações com significado preciso.                                                                    | Dois dispositivos independentes, perdas de resposta, reinícios e revogação não perdem operações confirmadas nem liberam histórico sem chaves/autorização.                                                                                                                       |
| 3. Segundo plano e chamadas   | Push real, navegação por notificação, extensão/lifecycle; chamadas de voz e vídeo com integração de sistema.                                                                                                        | Provas em iPhone físico com app fechado, bloqueio, permissão negada, rede interrompida e retomada; falha de push não perde trabalho durável.                                                                                                                                    |
| 4. Apps e agentes             | Apps MCP com capas uniformes em chat/Home, seleção/reorganização e atalhos removíveis/restauráveis; tela cheia com X flutuante, animação nativa e retrato/paisagem; Excalidraw real; conectores e agentes duráveis. | Estado do App persiste e sincroniza sem reiniciar ao girar; remover um atalho não apaga o documento; permissões/revogação são efetivas; tarefa de agente continua sem cliente aberto e reconcilia aprovações, efeitos e custo após reinício.                                    |
| 5. Criação compartilhada      | Páginas em blocos e planos, comentários/presença/histórico; Slides, Sites, Imagens e Arquivos; edição por pessoas e agentes.                                                                                        | Edição colaborativa e offline, conflitos/recovery, papéis e revogação, lixeira/restauração; publicação explícita e rollback de Sites; artefatos reabrem completos.                                                                                                              |
| 6. Lançamento iOS             | Comunidades/convites e permissões; Você/configurações; design de céu uniforme; PT/EN; distribuição assinada; backend público compatível.                                                                            | Instalar/atualizar no iPhone físico, executar as jornadas iOS aplicáveis com provedores reais, passar acessibilidade e falhas críticas, documentar limites e qualificar o perfil iOS/backend fixado antes da medição, incluindo segurança, abuso, recuperação/rollback e custo. |
| 7. Web e Android              | Reutilizar contratos, componentes adequados à plataforma e backend; completar a matriz de jornadas.                                                                                                                 | Execução real em navegador e Android, incluindo offline, recuperação, mídia, Apps/criação e interoperabilidade com o iPhone.                                                                                                                                                    |
| 8. Desktop e aceite final     | macOS/Windows/Linux; distribuição/atualização; operação e recuperação do produto completo.                                                                                                                          | Matriz final de plataformas, perfil de carga/custo medido, restauração/rollback e critérios globais aprovados com evidência reproduzível.                                                                                                                                       |

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

A ordem é de entrega; trabalho independente do núcleo/backend pode avançar em
paralelo. Cada lote fecha front, núcleo e backend da mesma jornada. Subagentes
trabalham com escopos isolados e um responsável por fonte, dispositivos e serviços.

Implementar um lote coerente, revisar e validar em conjunto. Repetir build ou a
suíte ampla quando uma mudança/falha justificar; não repetir tudo após ajustes de
documentação. Preservar testes de contratos, durabilidade e permissões. Uma falha
de ferramenta ou de ambiente é registrada separadamente, enquanto o trabalho
independente continua. Uma prova não executada permanece pendente.

**Marco iOS aceito** exige as jornadas iOS aplicáveis e as capacidades de criação
previstas, front e backend completos, provedores/configuração reais, build
assinado e validação em aparelho físico. O registro de plataforma não autoriza
marcar as três flags globais como aprovadas antes do produto completo. Não há
meta indefinida de copiar todas as features ou a escala global de concorrentes;
o escopo verificável é a matriz de 30 jornadas, criação e perfil numérico declarado.

### Evidências iOS preservadas

[Registro integral de 21 de outubro](zoen-complete-product-ios-evidence.md), com
seus checkpoints, falhas, limites e recibos; nenhum aceite é ampliado.

