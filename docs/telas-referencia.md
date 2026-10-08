# Telas de referência → protótipo

Mapa dos 133 pôsteres do conceito original (tironi.xyz) para o que existe neste protótipo. A ideia é manter a estrutura, o conjunto de telas e as ideias visuais do conceito, desenhados em SwiftUI nativo com Liquid Glass. Só nos afastamos dele onde a análise do conceito apontou problema.

Legenda: **Feita** = segue a referência · **Adaptada** = mesma intenção, desenho diferente (com o motivo) · **Falta** = ainda não existe.

## Navegação

| Referência | Estado | Notas |
|---|---|---|
| Barra inferior: Conversas · Comunidades · ✦ · Arquivos · Atividade | **Adaptada** | Mesma ordem e o mesmo botão central. Fica numa cápsula de **vidro** em vez da barra escura opaca: a conversa continua visível por baixo, como pede a linguagem da plataforma. |
| ✦ central (atalho para Agentes) | **Adaptada** | Virou o **menu radial**: um semicírculo com Busca · Seus agentes · Pedir ao Zoen · Seu contexto · Registro. A análise apontou que Busca, Contexto e Atividade do agente não tinham lugar fixo na barra; agora têm, sem passar de 5 itens na barra. Há hápticos, Reduzir Movimento e VoiceOver. |
| Mac | **Adaptada** | Barra lateral com os mesmos destinos e o radial no pé (⌘K). |

## Telas

| # | Tela | Estado | Notas |
|---|---|---|---|
| 011 | Conversas | **Feita** | Busca, botão +, filtros Todas/Pessoas/Agentes/Grupos e banner de pedidos. O filtro "Comunidades" virou "Grupos": as comunidades já têm destino próprio na barra. |
| 015 | Grupo de trabalho | **Feita** | O Item fixado (o plano mais recente) fica no topo da conversa. |
| 016 | Participantes | **Feita** | Pessoas e Agentes com papel e dono, atalho para Permissões e configuração da conversa. |
| 024 | Notificações | **Adaptada** | Fica em **Atividade**, com as pílulas Menções/Tarefas/Aprovações. Os pedidos chegam em lote com *Aprovar todos*, e a análise pediu menos ruído. |
| 025 | Busca unificada | **Adaptada** | Fica no radial e na barra lateral do Mac. É local, sem acento e sem maiúsculas, em mensagens e Itens. |
| 026 | Biblioteca de agentes | **Feita** | Seus agentes e os de outras pessoas, com confiança, orçamento e estado. |
| 027 | Perfil e modalidades | **Adaptada** | O perfil do agente traz a confiança Ouvir/Sugerir/Agir/Autônomo e a tabela "O que isso muda". Não há modalidades de sessão. |
| 030 | Atividade e limites | **Feita** | A atividade do agente com **custo por ação** é lida do log assinado (`agent_activity` no núcleo). O orçamento mensal aparece em anel. |
| 036 | DM com agentes | **Feita** | A conversa com o Zoen, onde uma frase vira um plano. |
| 038 | Permissões do agente no chat | **Feita** | A autonomia é real (núcleo) e as ferramentas são avaliadas pelo núcleo (`preview_decisions`). "Quem pode acionar" e o histórico mostram as regras fixas do protótipo. |
| 045 | Revisar uma ação | **Feita** | Aprovar, Editar no plano e Recusar. A aprovação fica presa ao hash do conteúdo. |
| 048 | Recibo de ação externa | **Adaptada** | O agente avisa na conversa e o texto deixa explícito que é uma simulação, sem pagamento real. |
| 052 | Resultado da tarefa e artefato | **Feita** | O plano nasce como cartão (Item) na conversa. |
| 067 | Seus arquivos | **Adaptada** | Itens viram arquivos e Espaços viram pastas. Há escopos Todos/Pessoal/Compartilhados/De agentes. Não existe sistema de arquivos à parte: tudo é Item versionado. |
| 073 | Histórico e checkpoints | **Feita** | Versões do Item com Desfazer e Restaurar, que nunca apagam nada. |
| 082 | Cofre pessoal | **Adaptada** | Virou **Seu contexto**: identidade, Seus dados, Concessões de acesso, Registro assinado e Configurações. |
| 091 | Descobrir comunidades | **Adaptada** | "Suas" mostra os grupos reais da demo. Descobrir e Criar aparecem como nota de fase, porque ainda não há diretório público nem moderação. |
| 121 | Edição invalida aprovação anterior | **Feita** | Editar a linha deixa o pedido *desatualizado*; desfazer a edição faz o pedido valer de novo. |
| 132 | Configurações gerais | **Adaptada** | Fica dentro de Seu contexto, com a origem da IA, o fallback e Recomeçar a demo. |

## Ainda não feitas

- **001–010 · Identidade, backup e dispositivos.** A análise chamou de barreira de entrada. O protótipo entra direto. No produto, a ideia é passkey + PRF, com o backup oferecido depois do primeiro valor.
- **012–014 · Nova conversa e criar grupo.** O botão + mostra as duas opções desativadas ("em breve"); o núcleo ainda não expõe criar Espaço pela UI.
- **017–023 · Papéis por conversa, anexos, voz, envio pendente, bloquear/reportar.**
- **028–035 · Criar agente, conhecimento, release, publicação e fork de agente.**
- **037, 039–044, 046–047, 049–051 · Admissão de agente, sessões, threads e coordenação entre agentes.**
- **053–066 · Jam: voz, câmera, compartilhar tela e editor multiplayer.**
- **068–072 · Workspace do agente, operações de arquivo, importação e compartilhamento.**
- **074–081 · Cenários e forks.** O log já guarda o que é preciso; falta o Loro (CRDT) com bifurcações.
- **083–090 · Importar fontes e wiki de conhecimento.**
- **092–114 · Página da comunidade, canais, tiers e pagamentos.**
- **115–120 · Rede pública e Studio.** Ficaram para depois por risco de moderação, como apontou a análise.
- **122–129 · Offline, sync, migração e provedores.** Ainda não há rede: tudo é local.
- **130–131, 133 · Privacidade e dispositivos, Aparência.** O app segue o tema e o tamanho de texto do sistema.
