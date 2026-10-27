# Direção de interface da meta completa

Parte normativa da [meta completa](zoen-complete-product-goal.md), separada por
responsabilidade documental. A seleção de UI e suas restrições são preservadas
integralmente abaixo; isso não reduz o escopo da meta.

## Referência principal de interface e interações

Telegram iOS é a referência principal de interface e interações, por direção do
usuário em 20 de outubro de 2026. O [guia de referência](../product/telegram-ios-reference.md)
e o [catálogo consultável](../../output/telegram-feature-matrix-2026-10-20/index.html)
orientam conversas, composição e teclado, anexos, mídia, ações, gestos,
transições, navegação, perfis e ajustes. O [estudo de cinco superfícies](../product/telegram-ios-next-ui-study.md)
registra os pontos de entrada nos componentes atuais.

Na instrução posterior do mesmo dia, o usuário **selecionou a reprodução da UI e
das interações completas da conversa do Telegram iOS, com fidelidade visual**,
adicionando os recursos próprios do Zoen sobre essa base. Isso inclui o fluxo de
anexos, e supera a interpretação anterior de que toda a UI aguardaria seleção.
Executar o [contrato de reprodução da conversa](../product/telegram-ios-conversation-parity.md),
usando o código iOS fixado para medidas e comportamento, sem substituir essa
direção por uma releitura visual genérica. Preservar o guia e o catálogo originais
como pesquisa datada; a seleção posterior está explícita neste adendo.

O usuário ampliou essa seleção para **todo o sistema de design e todas as telas
do Telegram iOS**, incluindo perfil, edição, detalhes, compositor, ajustes e
personalização de temas. Home, Apps/cards, agentes e criação entram como extensões
do Zoen. Portar superfícies e animações em lotes grandes de fonte, preservando
estado e autorização, antes da validação nativa integrada. A direção inicial de
aparência selecionou **Céu diurno** e **Céu noturno**, com modo automático. A seleção
vigente foi ampliada para **seis temas**: esses dois céus do Zoen e os quatro temas
Telegram **Classic, Day, Night e Tinted**. Os céus permanecem os padrões do Zoen;
as antigas fases não voltam como presets adicionais. Registrar diferenças e lacunas reais,
sem chamar uma tradução parcial de reprodução pixel perfect concluída.

As 506 entradas em 23 áreas são pesquisa de referência, não 506 capacidades
selecionadas para o Zoen. Os caminhos fixados são pontos de entrada por área,
sem rastreamento do comportamento de cada feature. A seleção de toda a UI
não seleciona automaticamente todos os produtos comerciais e serviços desse
catálogo. Essa direção não aprova um
fork, TDLib, infraestrutura do Telegram ou mudança de framework. A autorização
separada do usuário para AGPL foi aplicada e está registrada em LICENSING.md;
as partes derivadas mantêm suas atribuições e termos próprios.
Reprodução com nossos componentes e eventual adaptação de código precisam de
avaliação específica de comportamento, dependências, licença e manutenção.

A [revisão integral das 506 capacidades e o inventário de telas](../roadmap/README.md)
registram seleção de escopo, implementação, evidências e lacunas por capacidade e
superfície. A cobertura dessa revisão não significa paridade funcional completa
nem aceitação do produto ou das jornadas nativas.

Preservar a identidade de céu do Zoen, os cards consistentes e Apps em tela cheia,
os agentes, as permissões e os contratos do núcleo/backend. Pela direção mais
recente de Enzo, **Signal tem prioridade como referência para chamadas e huddles**,
com estudo de fonte fixado no [issue #178](https://github.com/EnzoTironi/zoen-native/issues/178).
Telegram iOS permanece a referência principal da UI de conversação. Signal também
continua orientando confiabilidade e testes comportamentais; Notion, ChatGPT Spaces/Live Pages
e [Morrow](../product/live-pages.md#product-references) orientam documentos vivos e criação,
por confirmação de Enzo em 26 de outubro de 2026. Essas referências de criação
permanecem no produto futuro, com sua execução pausada conforme a prioridade ativa
da meta completa. As trinta jornadas
existentes permanecem inalteradas; nenhuma paridade ou validação nativa decorre
da importação do catálogo ou da seleção da referência visual. Registrar as
diferenças visuais e comportamentais até suas provas no iPhone.
