[Research index](unit-economics.md).

## 2. Custo de IA na nuvem (preços oficiais, US$ por milhão de tokens)

| modelo | entrada | entrada em cache | saída | fonte |
|---|---|---|---|---|
| **GLM-5.3-Flash (padrão)** | **0,15** | **0,03** | **0,50** | [Z.ai pricing](https://docs.z.ai/guides/overview/pricing); igual no [Cloudflare Workers AI](https://developers.cloudflare.com/workers-ai/models/glm-5.3-flash/) |
| GLM-5.3-FlashX | 0,37 | 0,075 | 1,25 | Z.ai |
| GLM-4.7-FlashX | 0,07 | 0,01 | 0,40 | Z.ai |
| GLM-4.7-Flash / GLM-4.5-Flash | **grátis** | grátis | grátis | Z.ai (não achei limites de uso publicados nem SLA) |
| Qwen-Flash (Alibaba, internacional, ≤256k) | 0,05 | 0,01 | 0,40 | [Alibaba Model Studio](https://www.alibabacloud.com/help/en/model-studio/qwen-flash) |
| MiniMax-M3 (≤512k) | 0,30 | 0,06 | 1,20 | [MiniMax](https://platform.minimax.io/docs/pricing/overview) |
| Kimi K3 (Moonshot) | 3,00 | 0,30 | 15,00 | [Kimi pricing](https://platform.kimi.com/docs/pricing/chat), via notcheapai/anotherwrapper (não li a página oficial) |
| GPT-6 Sol / Claude Sonnet 5 (premium) | 2,00 | 0,20 | 10,00 | OpenAI / Anthropic pricing |
| GPT-6 Luna | 0,10 | 0,01 | 0,50 | OpenAI pricing |
| GPT-5 nano | 0,05 | 0,005 | 0,40 | OpenAI pricing |
| Claude Haiku 5.5 (prompt ≤100k) | 0,10 | 0,01 | 0,50 | Anthropic pricing |
| Gemini 2.5 Flash-Lite | 0,10 | 0,01 | 0,40 | Google pricing |
| Gemini 3.1 Flash-Lite | 0,25 | 0,025 | 1,50 | Google pricing |
| Gemini 3.5 Flash-Lite | 0,30 | 0,03 | 2,50 | Google pricing |
| DeepSeek Flash (V4.1), horário de pico / fora de pico | 0,30 / 0,15 | 0,006 / 0,003 | 1,20 / 0,60 | DeepSeek pricing |
| Gemini 3.8 Flash, promoção até 31/12/2026 | 0,75 | 0,075 | 3,75 | Google pricing |
| Gemini 3.8 Flash, a partir de 1/1/2027 | 1,50 | 0,15 | 7,50 | Google pricing |
| xAI (mais barato listado: grok-build-0.1) | 1,00 | 0,20 | 2,00 | xAI models |

Na xAI não encontrei um tier "fast/mini" barato publicado na página de modelos em out/2026.

**Descontos:**
- **Batch (processamento assíncrono)** custa 50% menos na OpenAI, Anthropic e Google. Serve para
  resumos e triagem que não precisam de resposta imediata.
- **Flex** na OpenAI também custa 50% menos.
- **Fora do horário de pico**, o DeepSeek cobra metade.

### Custo por usuário por mês, só IA na nuvem

| modelo | chamada leve | 5/mês leve (cache) | 5/dia leve (cache) | 5/dia pesada (cache) |
|---|---|---|---|---|
| **GLM-5.3-Flash (padrão)** | US$ 0,00038 | US$ 0,0012 | **US$ 0,037** | US$ 0,100 |
| GLM-4.7-FlashX | US$ 0,00022 | US$ 0,0008 | US$ 0,024 | US$ 0,065 |
| Qwen-Flash | US$ 0,00019 | US$ 0,0008 | US$ 0,023 | US$ 0,061 |
| MiniMax-M3 | US$ 0,00081 | US$ 0,0028 | US$ 0,084 | US$ 0,223 |
| Kimi K3 | US$ 0,00900 | US$ 0,031 | US$ 0,925 | US$ 2,47 |
| GPT-6 Sol / Claude Sonnet 5 | US$ 0,00600 | US$ 0,021 | US$ 0,616 | US$ 1,64 |
| GPT-6 Luna | US$ 0,00030 | US$ 0,0010 | US$ 0,031 | US$ 0,082 |
| GPT-5 nano | US$ 0,00019 | US$ 0,0007 | US$ 0,022 | US$ 0,059 |
| Claude Haiku 5.5 | US$ 0,00030 | US$ 0,0010 | US$ 0,031 | US$ 0,082 |
| Gemini 2.5 Flash-Lite | US$ 0,00027 | US$ 0,0009 | US$ 0,026 | US$ 0,070 |
| Gemini 3.1 Flash-Lite | US$ 0,00082 | US$ 0,0029 | US$ 0,088 | US$ 0,235 |
| Gemini 3.5 Flash-Lite | US$ 0,00120 | US$ 0,0046 | US$ 0,137 | US$ 0,367 |
| DeepSeek Flash (fora de pico) | US$ 0,00040 | US$ 0,0013 | US$ 0,038 | US$ 0,100 |
| Gemini 3.8 Flash (promoção) | US$ 0,00225 | US$ 0,0077 | US$ 0,231 | US$ 0,616 |
| Gemini 3.8 Flash (2027) | US$ 0,00450 | US$ 0,0154 | US$ 0,462 | **US$ 1,233** |

**O que isso diz:**
- No tier mais barato (Luna, Haiku 5.5, nano, 2.5 Flash-Lite), 5 chamadas por dia saem por
  2 a 3 centavos de dólar por usuário por mês.
- Num Flash de verdade, saem por 23 centavos a US$ 1,23, ou seja, 10 a 40 vezes mais.
- **A escolha do modelo pesa mais do que qualquer outra premissa.**
- O GLM-5.3-Flash **não é o mais barato**: o Luna sai ~20% abaixo, e o Qwen-Flash e o GLM-4.7-FlashX
  ~35% abaixo. Ele fica no mesmo nível de preço com um modelo bem mais forte: a Z.ai diz que ele
  "se aproxima do Claude Opus 4.8" em testes de código e agentes (essa comparação é dela).
- O Kimi K3 **não está nessa faixa**: custa o mesmo que o Sonnet. Serve para o premium, não para o
  plano grátis.
