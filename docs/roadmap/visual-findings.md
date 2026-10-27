# Revisão das telas existentes

26/10/2026 · App718 `b7ab2265` · Core707 `eb62cdaf` · iOS Simulator.

Foram percorridos estados acessíveis de Home, Atividade, Chats, conversa, busca,
menu da mensagem, anexos, grupo/participantes, perfil, configurações, temas,
dispositivos, permissões, controles de agente e Biblioteca. São 38 capturas,
37 hashes distintos; estados e repetições não viram novas telas.
O [manifesto visual](visual-review.json) registra os limites de cada captura.
As imagens e o recibo ficam em `output/roadmap-review-v723`, apenas localmente.

| Prioridade | Evidência observada | Resultado a entregar | Issue |
| --- | --- | --- | --- |
| Alta | Busca global: teclado cobre o estado vazio (31). | Estado vazio e resultados ocupam a área útil real; abrir, digitar, limpar e fechar preservam foco e navegação. | [#172](https://github.com/EnzoTironi/zoen-native/issues/172) |
| Alta | Controles e histórico do Agent passam sob o título (28, 36). | Cabeçalho legível durante toda a rolagem, com a mesma superfície nativa usada nas telas correspondentes de chat. | [#419](https://github.com/EnzoTironi/zoen-native/issues/419) |
| Média | Grupo: preferências muito compactas e switches no meio do cartão (07, 08). | Linhas e controles alinhados, alvos acessíveis e consistência com o perfil. A captura não mede a área real de toque. | [#181](https://github.com/EnzoTironi/zoen-native/issues/181) |
| Média | Notificações: explicações técnicas de cadastro/servidor dominam o conteúdo (20). | Estado e ação principal claros; detalhes técnicos ficam em diagnóstico. Preservar significado da permissão e do registro. | [#178](https://github.com/EnzoTironi/zoen-native/issues/178) |
| Revisão | Texto antigo permanece visível atrás do HUD/cards (04); o menu de mensagens e o anexo usam apresentações distintas (06, 11–14). | Preservar a transparência solicitada, mas conferir contraste, transições e área de leitura. Uma imagem isolada não prova falha da animação nem justifica remover os cards. | [#172](https://github.com/EnzoTironi/zoen-native/issues/172), [#173](https://github.com/EnzoTironi/zoen-native/issues/173) |

Os seis temas foram vistos sem mudar preferências. Não houve envio, digitação,
seleção de mídia, mudança de permissão, aprovação ou abertura de editor.
O Simulator terminou em Home/retrato; driver e runners foram encerrados.

Continuam sem prova neste lote: iPhone físico, VoiceOver, tamanhos de texto,
chamadas, câmera/microfone, permissões do sistema, push, login/recuperação,
resultados de busca digitada e ações efetivas do menu. O detalhe do dispositivo
apareceu na árvore de acessibilidade depois da captura37; essa imagem mostra
somente a lista. A captura10 repete09, e28 mostra histórico, apesar do nome do
arquivo. Apps e editores continuam pausados.

Esta revisão registra problemas e estados reais, sem afirmar comparação visual
pixel a pixel com Telegram ou aceite funcional. Fonte, teste, build, Simulator e
aparelho físico mantêm suas evidências separadas.
