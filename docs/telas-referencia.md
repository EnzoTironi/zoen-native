# Telas de referência e progresso atual

Atualizado em 10/10/2026. As 133 referências de design descrevem o produto completo. A existência de uma tela não comprova que transporte, permissões e operações de produção estejam prontos. O [roadmap](roadmap-status.md) registra versões e validações atuais.

## Navegação

Chats reúne conversas diretas, grupos e comunidades. Filtros opcionais ajudam a encontrar uma conversa; permissões e recursos compartilhados ficam dentro dela. A separação visual entre Grupos e Espaços foi removida no PR 44. Atividade/notificações abre em cartões, com botões para ir à lista e voltar aos cartões.

Desktop e web seguem os temas do iOS. O chat usa seu próprio cabeçalho, com widgets fixados sobre a conversa e espaço para os controles nativos de janela do Mac. Essas mudanças estão no [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), ainda em revisão.

## Cobertura verificada

| Referência ou fluxo | Implementação e evidência | Falta para concluir |
| --- | --- | --- |
| Conversas, grupos e comunidades | Inbox único e filtros no PR 44; jornada nativa de comunidade passou. | Integrar a versão validada e concluir descoberta/moderação/recursos com permissões. |
| Ler uma conversa | Cinco jornadas de posição de leitura e indicação de mensagens novas passaram no código combinado dos PRs 38/44. | CI da versão final e cobertura de release entre dispositivos. |
| Notificações e aprovações | Cartões como padrão e ida/volta explícita para lista; vídeo nativo no PR 44. | Runtime durável de agentes e reconciliação das decisões entre dispositivos no PR 46. |
| Mini-apps e widgets fixados | Rolagem com widget fixo, abertura/fechamento e edição/reordenação persistente passaram. | Catálogo assinado e interoperabilidade com servidores MCP remotos. |
| Identidade, backup e dispositivos | Núcleo de vínculo/revogação e recuperação criptografada integrado. | UI nativa completa e jornadas de restauração/vínculo/revogação em cada plataforma. |
| Arquivos, páginas e versões | Editor nativo por blocos, Loro, histórico e visualizadores implementados. | [Páginas vivas](product/live-pages.md), comentários, colaboração, páginas aninhadas e agentes atualizando seções. |
| Anexos e voz | Transporte de blobs criptografados; fluxos Android em revisão. | Envio real de foto/voz Apple, interrupção/retomada e leitura por outra conta. |
| Convites e aquisição | Convites, atribuição e experimentos no núcleo. | Fluxos completos de links e onboarding por origem. |
| Agentes, conhecimento e execução | Confiança, orçamento, grants, ferramentas e sandboxes implementados. | Agente autenticado como membro, execução durável, cobrança e publicação. |
| Comunidades, tiers e pagamentos | Fundamentos de conversa e design do produto. | Moderação, denúncia, integrações comerciais e ledger de pagamentos/créditos. |
| Offline, migração e privacidade | Rede real, outbox durável, MLS e estado local protegido. | Upgrades compatíveis, recuperação completa, push e falhas de produção. |

[Imagens e vídeos nativos de 10/10](https://github.com/EnzoTironi/zoen-native/pull/44#issuecomment-6098239059) documentam a interação, usando uma conta de demonstração isolada. Os pôsteres originais e as capturas antigas continuam sendo referências históricas, sem representar a cobertura funcional atual.
