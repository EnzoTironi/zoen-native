[Índice](../repensado.md) · [Anterior](12-zoen-signal-rust.md)

## Apêndice: destino de cada uma das 133 telas

Legenda: **Tela** = continua como tela própria (19). **Estado/componente** = vira estado ou componente de outra tela (88). **Some** = desnecessária no novo modelo (26).

| # | Tela original | Destino | Onde/por quê |
|---|---|---|---|
| 001 | Boas-vindas | Some | A conversa com seu agente é a primeira tela |
| 002 | Identidade e perfil | Estado/componente | Nome e foto pedidos no fluxo, depois |
| 003 | Escolher sua experiência | Some | Sem escolher 'experiência': tudo é Espaço |
| 004 | Criar e proteger backup | Estado/componente | Vira o cartão 'Proteger' quando já há algo seu |
| 005 | Entrar e recuperar | Tela | Entrar: passkey, outro aparelho ou código |
| 006 | Restaurar backup protegido | Estado/componente | Parte de Entrar |
| 007 | Identidade recuperada | Estado/componente | Confirmação vira toast |
| 008 | Conectar dispositivo | Tela | Conectar aparelho (QR) |
| 009 | Conexão de dispositivo expirada | Estado/componente | Estado de erro |
| 010 | Dispositivo conectado | Estado/componente | Toast |
| 011 | Conversas | Tela | Conversas (aba) |
| 012 | Iniciar uma conversa | Estado/componente | Folha 'Nova': mesma para pessoa, agente ou grupo |
| 013 | Criar grupo | Estado/componente | Mesma folha 'Nova' com 2+ membros |
| 014 | Grupo novo e convites pendentes | Estado/componente | Estado vazio da conversa |
| 015 | Grupo de trabalho | Tela | Tela do Espaço (conversa) |
| 016 | Participantes | Tela | Info do Espaço: membros, itens, regras |
| 017 | Papéis humanos por conversa | Estado/componente | Toque no membro → nível |
| 018 | Escolher mídia e arquivos | Estado/componente | Menu + do composer |
| 019 | Revisar um anexo | Some | Envia direto, com desfazer |
| 020 | Revisar foto capturada | Some | Prévia nativa da câmera basta |
| 021 | Revisar mensagem de voz | Estado/componente | Estado do composer |
| 022 | Envio pendente e tentativa posterior | Estado/componente | Estado da mensagem |
| 023 | Bloquear e reportar | Estado/componente | Folha do membro |
| 024 | Notificações | Tela | Atividade (aba) |
| 025 | Busca unificada | Tela | Busca (botão de vidro) |
| 026 | Biblioteca de agentes | Estado/componente | Você › Agentes (lista) |
| 027 | Perfil e modalidades | Estado/componente | Perfil universal |
| 028 | Criar e configurar agente | Tela | Criar agente (conversando) |
| 029 | Conhecimento e ferramentas | Estado/componente | Seção do perfil do agente |
| 030 | Atividade e limites | Estado/componente | Medidor de orçamento no perfil |
| 031 | Rascunho e teste do agente | Estado/componente | Rascunho = versão do Item 'definição' |
| 032 | Revisar publicação do release | Some | Publicar = nova versão, com desfazer |
| 033 | Release em uso | Estado/componente | Metadado 'v4' na resposta |
| 034 | Atualizar ou restaurar conhecimento | Estado/componente | Versões (universal) |
| 035 | Fork com nova identidade de agente | Estado/componente | 'Duplicar' (universal) |
| 036 | DM com agentes | Estado/componente | Mesma tela do Espaço |
| 037 | Escolher agentes | Estado/componente | Folha 'Adicionar membro' |
| 038 | Permissões do agente no chat | Estado/componente | Um controle: nível de confiança |
| 039 | Solicitar admissão de agente | Some | Adicionar agente = adicionar membro |
| 040 | Admissão de agente pendente | Some | Se a regra exigir, vira pedido em Atividade |
| 041 | Sessão reservada com agente | Estado/componente | Pílula de privacidade no cabeçalho |
| 042 | Sessão acompanhada | Estado/componente | Mesma pílula |
| 043 | Thread dentro da DM | Estado/componente | Espaço-filho, mesma tela |
| 044 | Coordenação entre agentes | Estado/componente | Conteúdo da conversa |
| 045 | Revisar uma ação | Estado/componente | Cartão de pedido (inline e em lote) |
| 046 | Ação recusada | Estado/componente | Estado do cartão |
| 047 | Resultado externo incerto | Estado/componente | Estado do cartão |
| 048 | Recibo de ação externa | Estado/componente | Recibo (componente) |
| 049 | Thread colaborativa | Estado/componente | Mesma tela do Espaço |
| 050 | Contexto da tarefa | Some | Contexto = o Espaço; ajuste por chip |
| 051 | Pessoas e agentes trabalhando | Estado/componente | Item 'tarefa' ao vivo |
| 052 | Resultado da tarefa e artefato | Estado/componente | Item |
| 053 | Criar ou agendar Jam | Some | 'Ao vivo' é um botão de qualquer Espaço |
| 054 | Antes de entrar | Estado/componente | Folha pré-entrada |
| 055 | Permissões do sistema para mídia | Some | Diálogo do sistema |
| 056 | Voz e câmera | Tela | Ao vivo (chamada/Jam) |
| 057 | Chamadas e falhas | Some | Chamadas ficam no histórico do Espaço |
| 058 | Reentrar no Jam | Estado/componente | Banner 'voltar' |
| 059 | Escolher compartilhamento de tela | Some | Seletor do sistema |
| 060 | Compartilhar tela | Estado/componente | Estado do Ao vivo |
| 061 | Compartilhamento ativo e encerramento | Estado/componente | Estado do Ao vivo |
| 062 | Segundo agente e acesso ao documento | Some | Herda as concessões do Espaço |
| 063 | Jam e documento colaborativo | Estado/componente | Ao vivo sobre um Item |
| 064 | Editor multiplayer | Tela | Item (documento/arquivo) |
| 065 | Editor com teclado e texto grande | Estado/componente | Estado do editor |
| 066 | Resumo autorizado | Estado/componente | Resumo vira Item automático |
| 067 | Seus arquivos | Estado/componente | Busca/Você › Itens (filtro) |
| 068 | Workspace próprio do agente | Some | Itens do agente vivem nos Espaços dele |
| 069 | Operações de arquivo | Estado/componente | Menu de contexto |
| 070 | Importação e fila de transferências | Estado/componente | Progresso inline |
| 071 | Prévia e resultados | Estado/componente | Item |
| 072 | Compartilhar arquivo ou pasta | Tela | Compartilhar = Conceder (universal) |
| 073 | Histórico e checkpoints | Tela | Versões (universal) |
| 074 | Cenários da thread | Estado/componente | Bifurcações aparecem em Versões |
| 075 | Criar alternativa | Estado/componente | Um toque: 'Bifurcar' |
| 076 | Comparar cenários | Tela | Comparar (universal) |
| 077 | Resolver divergências | Estado/componente | Dentro de Comparar |
| 078 | Consolidar ou restaurar | Estado/componente | 'Juntar' dentro de Comparar |
| 079 | Impacto da restauração | Some | Restaurar é reversível: sem tela de impacto |
| 080 | Resultado da restauração | Some | Toast |
| 081 | Fork independente do workspace | Estado/componente | 'Bifurcar' em novo Espaço |
| 082 | Cofre pessoal | Tela | Você › Memória (o que os agentes sabem) |
| 083 | Importar uma fonte | Estado/componente | Adicionar Item |
| 084 | Origem e restrições | Estado/componente | Info do Item (origem) |
| 085 | Wiki de conhecimento | Estado/componente | Item doc num Espaço |
| 086 | Promover conhecimento | Estado/componente | Compartilhar = Conceder |
| 087 | Promoção de conhecimento pendente | Estado/componente | Pedido em Atividade |
| 088 | Promoção de conhecimento negada | Estado/componente | Estado do pedido |
| 089 | Conhecimento aprovado e incorporado | Estado/componente | Estado do pedido |
| 090 | Fonte revogada e derivações restritas | Estado/componente | Estado do Item |
| 091 | Descobrir comunidades | Tela | Explorar (aba) |
| 092 | Página da comunidade | Estado/componente | Perfil universal (Espaço público) |
| 093 | Ativar comunidade preparada | Some | Publicar o Espaço é um toggle |
| 094 | Revisão final da comunidade preparada | Some | Idem |
| 095 | Ativação falhou ou foi recusada | Estado/componente | Estado de erro |
| 096 | Criar comunidade manualmente | Estado/componente | Folha 'Nova' + 'público' |
| 097 | Aceitar convite da comunidade | Estado/componente | Perfil do Espaço com convite |
| 098 | Canais e threads | Estado/componente | Espaços-filhos na mesma lista |
| 099 | Criar canal da comunidade | Estado/componente | Folha 'Nova' dentro do Espaço |
| 100 | Membros e papéis | Estado/componente | Info do Espaço (= 16) |
| 101 | Regras e tiers | Estado/componente | Seção 'Acesso' do perfil |
| 102 | Administrar comunidade | Estado/componente | Info do Espaço (admin) |
| 103 | Configurar tiers | Tela | Ofertas (concessões com preço) |
| 104 | Contratar acesso | Estado/componente | Folha de checkout nativa |
| 105 | Revisar assinatura e pagamento | Some | Apple Pay/Pix/Stripe já confirmam |
| 106 | Pagamento pendente | Estado/componente | Estado do checkout |
| 107 | Pagamento recusado | Estado/componente | Estado do checkout |
| 108 | Pagamento confirmado e acesso | Estado/componente | Toast + acesso liberado |
| 109 | Assinatura e uso | Estado/componente | Você › Acessos |
| 110 | Revisar cancelamento da renovação | Estado/componente | Você › Acessos (gerenciar) |
| 111 | Membresia expirada | Estado/componente | Estado 'trancado' (componente) |
| 112 | Recurso negado ou expirado | Estado/componente | Mesmo componente |
| 113 | Comprar conteúdo individual | Estado/componente | Mesmo checkout |
| 114 | Conteúdo adquirido na biblioteca | Some | Comprados aparecem em Itens |
| 115 | Feed público | Estado/componente | Explorar › Seguindo |
| 116 | Pessoa e seus agentes | Tela | Perfil universal (pessoa, agente, Espaço) |
| 117 | Conversa pública | Estado/componente | Espaço público, mesma tela |
| 118 | Studio do criador | Estado/componente | Rascunhos no seu perfil + sugestões em Atividade |
| 119 | Compor e aprovar | Estado/componente | Composer com 'Publicar em' |
| 120 | Publicação agendada | Estado/componente | Estado do Item |
| 121 | Edição invalida aprovação anterior | Some | Aprovação presa ao hash: regra automática |
| 122 | Offline e autorização | Estado/componente | Banner offline |
| 123 | Réplicas incompletas | Some | Infra escondida |
| 124 | Conflito de sincronização | Some | CRDT mescla; raro conflito vai inline |
| 125 | Exportar e restaurar | Estado/componente | Você › Dados |
| 126 | Migração interrompida | Some | Avançado, guiado pelo sistema |
| 127 | Migração retomada e cliente atualizado | Some | Idem |
| 128 | Mudar de provedores | Estado/componente | Ajustes › Avançado › Provedores |
| 129 | Concessão revogada após reconexão | Estado/componente | Estado do Item |
| 130 | Privacidade e dispositivos | Tela | Você › Segurança (aparelhos, chave, backup) |
| 131 | Navegar até as preferências | Some | Ajustes duplicada |
| 132 | Ajustes gerais | Tela | Você › Ajustes (única) |
| 133 | Aparência e acessibilidade | Estado/componente | Seção de Ajustes |
