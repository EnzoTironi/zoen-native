[Índice](../repensado.md) · [Anterior](01-tese.md) · [Próximo](03-experiencia.md)

## 2. O modelo mínimo

### Os 5 primitivos (+ 1 substrato)

| Primitivo | O que é | Exemplos que ele absorve |
|---|---|---|
| **Identidade** | Um par de chaves com nome e rosto. Pessoas, agentes e Espaços têm uma. Assina tudo que produz. | Conta, perfil, perfil de agente, perfil de comunidade, aparelhos (subchaves), assinador externo |
| **Espaço** | Um lugar com membros, regras e histórico. Pode ser privado ou público e pode ter Espaços-filhos. | DM, grupo, thread, canal, comunidade, workspace, sessão reservada, perfil público, seu "contexto" pessoal |
| **Membro** | Uma Identidade dentro de um Espaço, com papel. **Pessoa e agente são o mesmo objeto**; o agente tem, a mais, *dono*, *nível de confiança* e *orçamento*. | Participantes, papéis, moderadores, agentes no chat, agentes no "ao vivo", agentes públicos |
| **Item** | Qualquer coisa produzida num Espaço: mensagem, arquivo, documento, tarefa, post, resumo ou definição de agente. **Todo Item é versionado, bifurcável e tem origem.** | Anexos, Plano.md, cenários (bifurcação), checkpoints (versões), releases de agente, wiki, publicações, rascunhos, recibos |
| **Concessão** | Um objeto assinado que diz "*quem* pode fazer *o quê* em *qual* Espaço ou Item, com *quais limites* (orçamento, prazo)". Pode ser pedida, dada, vendida e revogada. | Permissões, papéis, nível de confiança do agente, convites, links de compartilhamento, planos e tiers, compras, assinaturas, aprovações, aparelhos autorizados, retenção |
| *Evento (substrato)* | O log append-only e assinado de cada Espaço. Nenhuma tela o mostra; ele é a fonte da verdade de tudo. | Histórico, sincronização, offline, desfazer, auditoria, portabilidade, migração |

Dois modos que **não são objetos novos**:
- **Ao vivo:** qualquer Espaço pode entrar em modo voz/vídeo/tela. Quando termina, o modo produz Itens (resumo, transcrição). Isso substitui "Jam" como entidade.
- **Oferta:** uma Concessão-modelo com preço. Tier, assinatura, compra avulsa e agente pago são todos Ofertas.

### Diagrama
Ver `repensado-diagrama.png` / `repensado.html`.

```
Identidade ──é──▶ Membro ──participa de──▶ Espaço ──contém──▶ Item (versões, bifurcações, origem)
     │                │                       │   └─▶ Espaço-filho (thread, canal, cenário)
     └──assina──▶ Concessão ◀── governa ──────┴── e Item          Oferta = Concessão com preço
Tudo vira Evento assinado no log do Espaço → sync, offline, desfazer, auditoria, portabilidade
```

### Como os 11 capítulos colapsam

| Capítulo (telas) | Primitivos | O que some ou se funde | Fica como tela | Vira estado/componente | Some |
|---|---|---|---|---|---|
| 1. Sua identidade (10) | Identidade + Concessão (aparelho) | Boas-vindas e "escolher experiência" somem. Backup é adiado e vira um cartão. Restaurar e recuperar viram uma tela só, "Entrar". | 2 | 6 | 2 |
| 2. Conversas e grupos (15) | Espaço + Membro + Item | Nova conversa e criar grupo são a **mesma folha**. Revisar anexo e revisar foto somem (envia e desfaz). Papéis viram um toque no membro. | 5 | 8 | 2 |
| 3. Seus agentes (10) | Membro (agente) + Item (definição) + Concessão (orçamento) | Release, rascunho, rollback e fork do agente viram **Versões** e **Duplicar**, que valem para qualquer Item. O perfil do agente usa o mesmo template de perfil de pessoa. | 1 | 8 | 1 |
| 4. Trabalhar com agentes (17) | Membro + Concessão + Item (tarefa) | "Solicitar admissão" some (adicionar agente = adicionar membro). Permissões viram **um controle de confiança**. "Revisar ação", "recusada", "incerta" e "recibo" viram **um componente**: o cartão de pedido. "Contexto da tarefa" some, porque o contexto é o próprio Espaço. | 0 | 14 | 3 |
| 5. Colaboração ao vivo (14) | Espaço em modo Ao vivo + Item | Criar Jam some: "Ao vivo" é um botão. Diálogos do sistema e o seletor de tela não precisam de design próprio. O acesso a documentos herda do Espaço. | 2 | 7 | 5 |
| 6. Arquivos e cenários (15) | Item (versões e bifurcações) + Espaço-filho | Workspace do agente, tela de impacto e resultado da restauração somem (restaurar é reversível). Cenários = bifurcações. Consolidar = Juntar. | 3 | 9 | 3 |
| 7. Conhecimento (9) | Item + Concessão (escopo de trecho) | "Promover conhecimento" com pendente, negado e aprovado é só **Compartilhar** com um pedido de Concessão. | 1 | 8 | 0 |
| 8. Comunidades e criadores (12) | Espaço público + Espaços-filhos + Ofertas | Ativar e revisar comunidade somem: publicar um Espaço é um toggle. Canais = Espaços-filhos. Membros e papéis usam a mesma tela de Info. | 1 | 9 | 2 |
| 9. Conteúdo e acessos (12) | Oferta + Concessão | Revisar assinatura some (o Apple Pay, o Pix ou o checkout já confirmam). Expirado e negado são **um componente "trancado"**. A biblioteca de compras some: o que você comprou aparece em Itens. | 1 | 9 | 2 |
| 10. Presença pública e Studio (7) | Espaço público + Item (post) + Concessão (publicar) | Studio deixa de ser uma tela: rascunhos ficam no seu perfil e as sugestões dos agentes vão para Atividade. "Edição invalida aprovação" vira uma regra automática (a aprovação fica presa ao hash da versão). | 1 | 5 | 1 |
| 11. Portabilidade e preferências (12) | Evento (log) + Identidade | Réplicas, conflito e migração somem da UI: o CRDT e o log resolvem, e a migração é um assistente avançado. A tela de Ajustes duplicada some. | 2 | 5 | 5 |
| **Total (133)** | | | **19** | **88** | **26** |

**Em números:** 114 das 133 telas (86%) deixam de precisar de design próprio. 26 (20%) somem por completo, porque a regra do sistema as torna desnecessárias. 88 viram estados de ~12 componentes reutilizáveis: cartão de pedido, recibo, estado trancado, pílula de privacidade, folha Nova, folha Conceder, Versões, Comparar, medidor de orçamento, banner offline, toast de desfazer e checkout. Somando a aba nova "Você", o app inteiro cabe em **20 telas**.

---

