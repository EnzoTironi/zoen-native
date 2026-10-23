[Índice](../repensado.md) · [Anterior](07-protocolos.md) · [Próximo](09-roadmap.md)

## 8. Signal como inspiração?

**Veredito:** **sim, para a segurança. Não, para o produto.** Copiamos do Signal o rigor e a minimalidade de dados. Do resto do mercado, pegamos produto e escala.

| Copiar do Signal | Não copiar do Signal |
|---|---|
| Rigor criptográfico: specs públicas, provas formais, PQ cedo (PQXDH, SPQR) | Produto minimalista sem recursos de servidor (sem agentes, sem comunidades grandes, sem busca no servidor) |
| Minimização de metadados: sealed sender, nada de grafo social no servidor | Grupos pequenos como teto (~1.000) |
| Cliente e protocolo abertos, com auditorias públicas | Multi-aparelho historicamente dependente do celular principal (aparelhos "vinculados") |
| Sem anúncios e sem rastreamento | Conta ancorada no telefone (melhorou com usernames em 2024) |
| Pedidos de mensagem de desconhecidos | Velocidade de produto lenta e modelo de fundação/doações |
| Retenção mínima no servidor (TTL) | Stack Java + Redis/DynamoDB (não é o nosso) |

**De outros, em vez disso:**
- **WhatsApp:** simplicidade obsessiva, time minúsculo, multi-aparelho com chaves por aparelho, **key transparency (AKD)** e backups E2EE.
- **Telegram:** velocidade de produto, canais gigantes, bots de primeira e sync instantâneo. Não copiamos o E2EE fora do padrão.
- **Discord:** comunidades com canais, papéis granulares, AutoMod e voz sempre à mão; Rust e Scylla nos caminhos quentes; MLS nas chamadas (DAVE).
- **Slack:** dono por canal com hash consistente, gateways regionais, threads, integrações e espaços entre empresas.
- **Ando:** agentes como membros, proatividade por canal e contexto visível.
- **iMessage:** integração com o sistema, PQ3 e Contact Key Verification.

**Licença:**
- **libsignal e o Signal-Server são AGPL-3.0.** Usar no app obrigaria a abrir o código do cliente sob AGPL, e no servidor gera obrigações de copyleft de rede.
- **OpenMLS (MIT), AKD (MIT/Apache), Loro (MIT), LiveKit (Apache-2.0), NATS (Apache-2.0) e Cedar (Apache-2.0)** são permissivas.
- A licença do Restate é *(a verificar: possivelmente BSL)*; a alternativa é o Temporal (MIT).

---

