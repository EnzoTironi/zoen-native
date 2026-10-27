# Meta completa do Zoen

Entregar o Zoen completo no iOS, Android, Web, macOS, Windows e Linux, incluindo
backend de produção, experiência refinada, privacidade, integridade dos dados e
operação comprovada sob carga. Preparar o produto para competir com WhatsApp,
Slack e Wabi em qualidade, confiabilidade e capacidade de crescimento, com
evidências reproduzíveis para cada afirmação.

Esta é a meta do produto inteiro. Sua conclusão exige os aplicativos finais,
o backend público e a operação de produção funcionando em conjunto. O registro
de critérios é [acceptance.json](../../apps/zoen/acceptance.json).
As evidências anteriores continuam com seu escopo e seus artefatos originais.
Ampliar a meta não transforma provas parciais em aprovação do produto completo.

## Roadmap verificável — 26 de outubro de 2026

A [roadmap completa em checklist](../roadmap/README.md) organiza as 30 jornadas,
as dez capacidades de criação, as seis plataformas e os critérios de operação.
O [painel consultável](../roadmap/index.html) cruza esse plano com a revisão das
506 capacidades do Telegram e o inventário das telas, sheets, menus e superfícies
nativas do Zoen. O levantamento tem fonte e data: presença de código, evidência
parcial e aceite integral são estados distintos.

Enzo responde pela direção de produto. Codex propõe e executa as decisões técnicas
e de sequência nos issues correspondentes, antecipando falhas e eliminando
duplicação. Elegância significa poucas autoridades de estado, contexto preservado
entre telas e ações claras; ambição significa jornadas completas de pessoas e
agentes, incluindo interrupção e recuperação. Completude inclui estados vazio,
carregando, offline, erro, permissão recusada e revogação, além do caminho feliz.

O catálogo do Telegram serve para encontrar lacunas. Capacidades essenciais de
conversa, privacidade e comunidades entram na entrega; modelos de moeda, boosts,
colecionáveis e publicidade continuam referências adicionais. Benefícios úteis
de aparência, expressão e automação devem usar os controles comuns do Zoen,
sem criar versões concorrentes do mesmo recurso. As decisões e recomendações
ficam explícitas no painel, preservando todo o escopo previamente aprovado.

O issue #170 e seus issues de entrega continuam sendo o registro de execução;
o manifesto de aceitação continua sendo o contrato de conclusão. O painel é uma
leitura datada dessas fontes, sem um segundo estado de tarefas editável.

## Coordenação durável no GitHub

Por direção do usuário em 22 de outubro de 2026, o registro de trabalho é o
[issue principal #170](https://github.com/EnzoTironi/zoen-native/issues/170).
Ele vincula as frentes de entrega #172–#184 e os problemas anteriores ainda
abertos. Cada novo pedido, defeito ou restante de uma entrega deve estar em um
issue com responsável lógico, critérios observáveis, dependências, estado e PR.
O chat e as mensagens entre agentes não substituem esse registro.

Implementação e QA avançam em paralelo em escopos isolados. Um lote pronto segue
para revisão e validação enquanto outros continuam em desenvolvimento. Agrupar
o check completo e o build nativo por fonte coerente; repetir por mudança, falha
ou dúvida material. Cada simulador e serviço tem um operador exclusivo. A fila
das demais frentes permanece explícita no GitHub e não é anunciada como trabalho
em execução. Um PR parcial referencia seu issue; não encerra os critérios ainda
pendentes. Merge normal após fonte limpa e checks aplicáveis aprovados.

## Execução atual — 26 de outubro de 2026

[Ordem de entrega iOS, lotes e critérios de saída](zoen-complete-product-execution.md).
O plano integral e as evidências preservadas mantêm seu escopo e seus limites.

## Prioridade ativa — 26 de outubro de 2026

Por direção confirmada de Enzo, o trabalho ativo é exclusivamente **agentes e
conversação**, concluindo o recorte selecionado de interface/interações do
Telegram iOS e seu backend antes de retomar Apps. Desenvolvimento e QA de Apps,
criação compartilhada, editores e expansão de SDK/host estão pausados. Código,
dados e evidências ficam preservados; as capacidades permanecem no produto final.
Esta é uma mudança de sequência, sem aceite ou exclusão das jornadas pendentes.

## Plataformas e arquitetura

O Zoen deve compor capacidades que pessoas, agentes e outros produtos possam
consumir pelos mesmos contratos de domínio. UI, SDK, CLI e MCP adaptam essas
operações; cada domínio preserva a autoridade de seu estado, permissões e
recuperação. Comprovar as fronteiras no workspace atual e o consumo independente
antes de extrair repositórios. Implementação nativa, bindings e contratos gerados
compatíveis integram a mesma unidade de release.

A execução atual conclui o iOS antes de ampliar a validação manual para Web e
Android. Em seguida, as mesmas jornadas são qualificadas nessas duas plataformas;
macOS, Windows e Linux ficam para a fase posterior do produto completo. O marco
de lançamento iOS tem seus próprios critérios e não depende da entrega desktop.
Segurança, integridade e backend acompanham cada lote.

Compartilhar o modelo de produto e o núcleo Rust, usando React Native/Expo e
adaptações nativas adequadas a cada plataforma. Evitar bases nativas completas
independentes, regras duplicadas, estado durável concorrente e caminhos legados
sem uso. Recursos exclusivos de um sistema precisam de alternativas explícitas
e adequadas nas outras plataformas.

O backend deve executar os serviços Rust nativos, sem reimplementá-los para um
runtime específico de hospedagem. Cloudflare pode servir DNS, CDN, frontend
estático ou armazenamento compatível com S3. A escolha final de hospedagem deve
favorecer operação simples e o volume declarado, sem um gargalo global oculto.

Na fase atual, desenvolver e validar localmente com os serviços reais em
containers, persistência, migrations, arquivos criptografados, verificações de
saúde e recuperação após falhas. O ambiente local deve aproximar a composição de
produção. Sua aprovação não substitui as jornadas no backend público, exigidas
na entrega final.

## Produto completo

Preservar as 30 jornadas obrigatórias, incluindo chamadas, desktop e critérios
de operação de produção. A matriz de produto deve cobrir:

- Apple/Google com criação automática de conta, chaves e perfil; edição posterior
  do perfil e recuperação segura do histórico em novos dispositivos.
- Mensagens, respostas, threads, edição, exclusão, reações, encaminhamento, busca,
  recibos e histórico longo, com interações, animações e acessibilidade cuidadas.
- Fotos, câmera, vídeo, documentos, áudio compacto com revisão/edição e chamadas
  de voz/vídeo seguras, com tratamento de interrupções e ciclo de vida do sistema.
- Home, comunidades, convites, permissões, agentes com controles de acesso e
  orçamento, notificações e integrações nativas previstas para cada plataforma.
- Páginas, planos, colaboração, histórico, conflitos, exportação e restauração.
- MCP Apps com isolamento e capacidades verificadas; Excalidraw permanece uma
  prova concreta, e integrações externas precisam funcionar com permissões reais.
- Tema de céu, consistência em português/inglês, VoiceOver/leitores de tela,
  teclado, tamanhos de texto, contraste e redução de movimento.

A comparação com WhatsApp, Slack e Wabi deve identificar a versão, plataforma,
data, jornada relevante e evidência. Não basta usar suas interfaces como referência.

## Referência principal de interface e interações

A [direção integral de UI](zoen-complete-product-ui-reference.md) é parte normativa
desta meta: Telegram iOS, suas telas/animações e as extensões do Zoen. A seleção
do usuário, os dois temas de céu, as fontes e os limites continuam preservados.

## Referência principal de confiabilidade

Signal é a principal referência de confiabilidade no app, núcleo e backend.
O [estudo com fontes fixadas](signal-reliability-reference.md) define seis
contratos: operações duráveis e confirmações reais; continuidade da conversa;
ciclo próprio da mídia; separação entre login, vínculo e recuperação; privacidade
por camada; notificações e chamadas integradas ao ciclo de vida do sistema.

A execução deve preservar as autoridades existentes e priorizar OSS mantido e
recursos nativos adequados. Comparações de código não aprovam segurança equivalente
ou desempenho. Adoção de bibliotecas exige compatibilidade, licença e manutenção;
nenhuma referência justifica outra base de estado durável ou uma reescrita do
backend sem necessidade comprovada. As diferenças de banco local, metadados,
recuperação e segundo plano permanecem explícitas até serem resolvidas e validadas.

## Contratos aprendidos nas suítes do Signal

Manter o [registro de cenários](signal-test-contracts.json) com fontes e versões,
estado inicial, sequência, falha introduzida, asserções e comparação com o Zoen.
Ler os corpos dos testes, parâmetros, fixtures e consumidores dos fuzzers;
inventário por nome não comprova cobertura. Classificar equivalente, parcial,
lacuna ou incompatibilidade com a arquitetura. Priorizar perda de dados,
duplicação, autorização, recuperação e mídia, com testes originais dos nossos
limites. A maior parte roda no núcleo/backend; reservar aparelho físico para
entrada, áudio/câmera/chamadas, segundo plano e comportamentos de plataforma.
As evidências de testes, fonte, compilação, simulador e aparelho físico permanecem
distintas. O levantamento inicial é parcial e deve continuar por lotes.

## Área de criação compartilhada

A direção de 25/10/2026 separa os editores ArtCraft como dependência externa MCP
App, derivada de upstream e com release próprio. Seu desenvolvimento e QA internos
estão parados. O Zoen continua responsável pelo host, identidade, permissões,
persistência criptografada, versões e compartilhamento; as capacidades de criação
continuam no produto, fornecidas pela integração com Apps externos. A migração
não está concluída. Ver a [fronteira e o histórico](artcraft-editor-integration.md).

Em 20 de outubro de 2026, o usuário definiu Notion e as Páginas do ChatGPT Space
como referências para as Páginas vivas e ampliou essa direção para toda a área de
criação: Páginas, Slides, Sites, Imagens e Arquivos. A experiência deve usar os
componentes nativos e o design de céu do Zoen, integrada às conversas e aos Apps.
As trinta jornadas originais e a entrega final nas seis plataformas continuam
obrigatórias; esta ampliação acrescenta capacidades, sem converter provas antigas
em aprovação dos novos fluxos.

Pessoas e agentes autorizados devem trabalhar sobre os mesmos artefatos
persistentes. Páginas precisam de edição direta em blocos, colaboração, comentários,
presença, histórico e recuperação. Slides devem permitir criar, editar, ordenar,
apresentar e exportar; Sites precisam de edição, prévia, publicação explícita,
versões e rollback; Imagens precisam de geração, upload, edição com referências,
variantes e biblioteca. Arquivos reutilizam o armazenamento e as permissões do
produto. A área compartilhada inclui organização em espaços e páginas filhas,
itens recentes, busca com escopo, compartilhamento, funções, revogação, lixeira e
restauração.

A conversa junto ao artefato fornece contexto explícito ao agente, com autoria e
alterações revisáveis. O agente continua sujeito às mesmas permissões do artefato;
uma resposta em chat, uma interface estática ou um adaptador privado não comprovam
edição colaborativa ou execução durável. Conteúdo privado permanece no modelo de
criptografia do Zoen; publicar um Site é uma ação explícita sobre a versão escolhida.

O primeiro lote continua no iOS, com o núcleo Rust e os serviços reais. Captura de
texto e revisão de anexos são fundamentos, não a conclusão da área de criação.
As capacidades adicionais ficam registradas em `creationWorkspace` no manifesto
de aceitação e só podem ser aceitas com evidências das jornadas completas.

Referências públicas: [colaboração no Notion](https://www.notion.com/help/collaborate-with-people),
[Páginas do ChatGPT Space](https://learn.chatgpt.com/docs/space/pages) e
[agentes no Space](https://learn.chatgpt.com/docs/space/agents).

## Backend, privacidade e integridade

Completar autenticação real, diretório de contas/dispositivos, criptografia,
pacotes de chaves, sincronização, entrega, associação a conversas/comunidades,
arquivos criptografados, recuperação, notificações e agentes. Validar os clientes
contra o backend público implantado, além dos testes locais e adaptadores privados.

Preservar criptografia de ponta a ponta e documentar os limites de confiança.
Login com provedor, sozinho, não pode liberar histórico criptografado anterior.
Chaves, autorização, revogação, dispositivos vinculados, backups e isolamento de
miniapps precisam de validação adversarial e revisão de segurança.

Provar comportamento correto com offline, repetição, perda de resposta,
reordenação, concorrência, gravação interrompida, encerramento do processo,
revogação e recuperação. Dentro do modelo de falhas declarado, uma operação
confirmada não pode desaparecer nem produzir efeitos duplicados indevidos.
Corrupção deve ser detectada e apresentada sem inventar sucesso ou descartar
dados silenciosamente. Migrações de dados já implantados precisam preservar essas
garantias mesmo durante atualização e rollback.

## Escala e operação comprovadas

Antes da verificação de escala, definir metas numéricas e o perfil de carga:
conexões simultâneas, mistura de jornadas, taxas sustentadas/picos de mensagens,
eventos, mídia e chamadas, tamanho de histórico, dispositivos por conta e fila
acumulada offline. Definir também latências p50/p95/p99, disponibilidade, orçamento
de erros, objetivos de recuperação e custo por usuário/operação/faixa de uso.

Os testes precisam medir carga sustentada, picos, reconexão em massa, históricos
grandes, concorrência, degradação, limites de serviço e recuperação. Registrar
gargalos, capacidade efetivamente validada, consumo e custos. Um teste de carga
isolado não demonstra a escala global de outro produto.

O primeiro perfil numérico está em
[zoen-operating-envelope-v1.json](zoen-operating-envelope-v1.json): 200 conexões
ativas sustentadas, reconexão de 1.000 dispositivos, 20 gravações por segundo e
pico de 100, além de mídia, chamadas, latência, recuperação e orçamento. São metas
de engenharia declaradas antes da medição. Ainda não são capacidade aprovada.
Cada execução deve fixar o hash desse perfil e dos artefatos, registrar os limites
e repetir o cenário completo duas vezes. A disponibilidade mensal exige 30 dias
de observação; testes de falhas não substituem essa evidência.

Operação de produção exige observabilidade que preserve privacidade, alertas,
limites, proteção contra abuso, tratamento de pressão, diagnóstico e procedimentos
de incidente. Exercitar restauração, recuperação de desastre, atualização,
migração e rollback; medir seus resultados.

## Critério para encerrar a meta

Encerrar apenas quando todas as jornadas e critérios passarem nos artefatos finais
da matriz de sistemas/dispositivos/navegadores suportados, com o backend público
correspondente, CI reproduzível, builds assinados quando aplicável e distribuição
adequada. Incluir instalação, atualização, reinício a frio, recuperação, desempenho,
acessibilidade, segurança e operação sob o perfil de carga declarado.

`fullAppParityAccepted`, `productionReadinessAccepted` e `scaleReadinessAccepted`
permanecem `false` até haver evidência suficiente. Os limites conhecidos e as
falhas abertas continuam registrados. Build aprovado, frontend hospedado, interface
polida ou adaptador privado funcional são progresso, sem encerrar a meta completa.

## Evidências históricas preservadas

[Checkpoints integrais Web v136 e iOS v135/v134/v133](zoen-complete-product-historical-evidence.md),
com as falhas e os limites originais; não constituem aceite atual.
