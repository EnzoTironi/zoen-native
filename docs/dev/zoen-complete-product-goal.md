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

## Plataformas e arquitetura

A primeira fase entrega iOS, Android e Web. A fase seguinte entrega macOS,
Windows e Linux; desktop faz parte da meta final. Segurança, integridade e
validação acompanham todas as fases.

Compartilhar o modelo de produto e o núcleo Rust, usando React Native/Expo e
adaptações nativas adequadas a cada plataforma. Evitar bases nativas completas
independentes, regras duplicadas, estado durável concorrente e caminhos legados
sem uso. Recursos exclusivos de um sistema precisam de alternativas explícitas
e adequadas nas outras plataformas.

Cloudflare é a preferência inicial de hospedagem. A arquitetura e os limites
de cada serviço precisam sustentar o volume declarado, sem um gargalo global
oculto nem dependência operacional que inviabilize manter o produto sozinho.

## Produto completo

Preservar as 24 jornadas originais e acrescentar chamadas, desktop e critérios
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
