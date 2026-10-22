[Research index](unit-economics.md).

## 5. Infraestrutura do mensageiro (sem IA)

| referência | custo por usuário por mês | fonte |
|---|---|---|
| **Zoen, núcleo medido** (ADR 0022: relay, FoundationDB, saída; 1 bilhão de usuários) | **US$ 0,0005 por MAU** | ADR 0022 (bench S8, 08/out/2026) |
| **Zoen, com mídia** (cost-model: fotos, CDN, Postgres) | US$ 0,0042–0,010 por DAU, ≈ US$ 0,002–0,005 por MAU | docs/cost-model.md |
| **Signal** (infraestrutura de US$ 14 milhões/ano, nov/2023, ÷ ~40 milhões de MAU estimados) | ≈ US$ 0,029; **US$ 0,017 sem os US$ 6 milhões de SMS de cadastro** | [Signal blog](https://signal.org/blog/signal-is-expensive/), MAU estimado por terceiros ([TechCrunch](https://techcrunch.com/2023/11/17/signal-costs/)) |
| WhatsApp e Telegram | não encontrado (não publicam custo de infraestrutura) | — |

**SMS de cadastro** foi o maior item do Signal (US$ 6 milhões de US$ 14 milhões).
- Via Twilio Verify, o Brasil custa US$ 0,0599 por SMS + US$ 0,05 por verificação, ≈ **US$ 0,11
  por cadastro** ([Twilio](https://www.twilio.com/en-us/verify/pricing)).
- Um código de autenticação enviado pelo WhatsApp custa US$ 0,0068 no Brasil (tabela da Meta).
- Com passkey ou login por um aparelho já conectado, custa zero.
- Se 5% dos MAU forem cadastros novos por mês, o SMS custa **≈ US$ 0,0055 por MAU por mês**.

Uso **US$ 0,002 / 0,01 / 0,03 por MAU** como infraestrutura nos cenários baixo, médio e alto:
- **baixo:** o nosso modelo em escala;
- **médio:** o nosso modelo em escala pequena, perto do Signal sem SMS;
- **alto:** o custo real do Signal, que inclui chamadas de voz e vídeo.
