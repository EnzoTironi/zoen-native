[Índice](../repensado.md) · [Anterior](11-inconsistencias.md) · [Próximo](apendice-telas.md)

## 12. Como isso se encaixa no Zoen e no signal-rust (decidido)

**Decisão (D6):** **Zoen** é o produto, o **signal-rust é o motor** e o **Zoen é o primeiro agente**.

**Zoen (EnzoTironi/tryzoen)**
- O companion 1:1 vira o agente padrão do Zoen e o momento mágico. Metas e Ideias viram Itens (tarefa e nota), Library vira Você › Itens e Activity vira a aba Atividade.
- As integrações de Telegram e WhatsApp continuam como **pontes de aquisição**: fale com o Zoen pelo WhatsApp e migre para o Zoen quando quiser grupos ou documentos.
- O PR 211 (Zoen Night + Liquid Glass) vira a base do DesignSystem nativo, agora em SwiftUI.
- O tryzoen (Next.js) segue no ar até a migração. Sua landing vira a landing do Zoen, com "Zoen incluso".

**signal-rust (EnzoTironi/signal-rust)**
- Vira o repositório do núcleo (`zoen-native/`). O plano do PR #34 era Nostr-first com Flutter.
- **Ajustes decididos:** UI nativa (SwiftUI agora, Compose em 2027) no lugar do Flutter; servidor próprio primeiro; MLS via OpenMLS; ATProto para o público; Nostr/Marmot como ponte.
- Ressalva: não li o código do signal-rust. Confirmar o estado real antes de reaproveitar.

---

