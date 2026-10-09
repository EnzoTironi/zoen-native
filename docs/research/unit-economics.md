# Zoen: custo por usuário ativo vs. receita sem cobrar o consumidor

Pesquisa de 9/out/2026. Todos os preços foram lidos nas páginas oficiais nessa data (links na
seção Fontes). Onde não achei dado público, está escrito "não encontrado". As contas estão em
[`unit_economics_calc.py`](unit_economics_calc.py) para quem quiser mexer nas premissas.

> **Resumo em uma frase.** No cenário médio (5 chamadas de IA por dia num modelo barato, tipo
> GPT-6 Luna), um usuário ativo custa **≈ US$ 0,05 por mês (≈ R$ 0,25)**. O banner em comunidade
> rende, no Brasil, **≈ US$ 0,01–0,02 por usuário por mês** nas premissas médias. **Só anúncio
> não fecha a conta**, a não ser que a IA na nuvem fique rara (5 chamadas por mês) ou que o CPM
> seja vendido direto, bem acima do programático. Quem fecha a conta é o lado empresa: bots
> pagos para negócios, mensagens de empresa e comissão na loja.
>
> **Gringos (seção 15).** Nem nos EUA o banner médio cobre o custo médio (US$ 0,029 contra US$ 0,047).
> Com as alavancas de empresa, EUA e Europa rendem ≈ US$ 0,17–0,18 por usuário por mês, a América
> Latina ≈ US$ 0,10 e a **Índia ≈ US$ 0,03, que fica abaixo do custo**. Na mistura global (B, metade
> Índia e emergentes) dá ≈ US$ 0,09 por usuário: fecha, mas com pouca folga. **Em mercado de CPM
> baixo, a IA na nuvem precisa de um teto menor** (modelo no aparelho primeiro).

## 1. Premissas

- **MAU** = usuário ativo no mês. Telegram tem cerca de 500 milhões de ativos por dia para 1
  bilhão por mês ([FT via Yahoo Finance](https://finance.yahoo.com/news/telegram-revenue-surges-65-870m-160000131.html)),
  então uso **DAU/MAU ≈ 0,5** para converter números "por DAU" em "por MAU".
- **Câmbio:** R$ 5,15 por dólar, implícito na tabela oficial da Meta para o Brasil (R$ 0,3217 =
  US$ 0,0625 por mensagem de marketing, out/2026).
- **"5 chamadas" de IA:** como não ficou claro se era por dia ou por mês, calculei os dois:
  - **5/mês**: a IA na nuvem só entra quando o modelo do aparelho não dá conta;
  - **5/dia (150/mês)**: uso diário do assistente.
- **Tamanho da chamada:**
  - **leve**: 1.500 tokens de entrada e 300 de saída;
  - **pesada**: 4.000 de entrada e 800 de saída.
  - "Com cache" supõe 70% da entrada reaproveitada (instruções e ferramentas fixas), cobrada
    pelo preço de cache.
- **Nomes que o Enzo citou:**
  - **"GPT-6 Luna"** existe: é o tier barato atual da OpenAI.
  - **"LM 5.3 Flash"** não existe com esse nome. O mais próximo é a família Gemini Flash
    (3.5 Flash-Lite, 3.8 Flash), e incluí as duas.

## 2. Custo de IA na nuvem (preços oficiais, US$ por milhão de tokens)

| modelo | entrada | entrada em cache | saída | fonte |
|---|---|---|---|---|
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
| GPT-6 Luna | US$ 0,00030 | US$ 0,0010 | **US$ 0,031** | US$ 0,082 |
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

## 3. Modelo no aparelho ("bem burrinho")

- **iPhone e Mac:** o framework Foundation Models da Apple dá acesso grátis ao modelo do
  sistema, sem aumentar o tamanho do app, nos aparelhos com Apple Intelligence
  ([Apple](https://developer.apple.com/documentation/foundationmodels)).
- **Android:** o Gemini Nano roda no aparelho via ML Kit GenAI e AICore, sem custo de nuvem, mas
  só em aparelhos compatíveis
  ([Google](https://developers.google.com/ml-kit/genai), [Android](https://developer.android.com/ai/gemini-nano)).
- **Custo marginal para o Zoen: US$ 0.** O usuário paga com bateria e processamento do aparelho.
- **Aparelhos sem modelo do sistema** (Android de entrada, comum no Brasil) precisariam baixar
  pesos próprios. Não encontrei dado público de quantos aparelhos no Brasil têm Gemini Nano;
  isso precisa ser medido. Um modelo de cerca de 1 bilhão de parâmetros, quantizado em 4 bits,
  tem uns 0,5–0,7 GB. Essa é uma conta minha, não um dado publicado.
  - **No Cloudflare R2:** saída grátis e armazenamento a US$ 0,015/GB-mês, com operações de
    leitura a US$ 0,36 por milhão ([R2](https://developers.cloudflare.com/r2/pricing/)).
    **Custo ≈ US$ 0.**
  - **Servindo pelo Fly:** US$ 0,04/GB de saída na América do Sul (preço do ADR 0022), o que dá
    uns US$ 0,03 por aparelho, uma vez só. Por isso os pesos devem ir pelo R2.

## 4. "Assinatura do ChatGPT"

Há três leituras possíveis.

1. **O usuário usa o próprio plano do ChatGPT ("Sign in with ChatGPT").** Existe desde 2026.
   - O usuário entra com a conta do ChatGPT e pode, se quiser, deixar o app usar a franquia do
     plano dele. Não dá acesso às conversas nem à memória
     ([OpenAI Help](https://help.openai.com/en/articles/20001410-sign-in-with-chatgpt),
     [quickstart](https://developers.openai.com/siwc/quickstart)).
   - **Custo para o Zoen: US$ 0.**
   - **Limites:**
     - só vale para planos **Plus e Pro**;
     - "usar o plano" está aberto para projetos open source e locais e para **apps privados
       selecionados**;
     - apps comerciais ou hospedados **entram por lista de espera**
       ([interest form](https://developers.openai.com/siwc/request-client-id)).
   - **Ação:** pedir acesso já. É um "traga sua IA" que não custa nada ao Zoen e atende o usuário
     avançado.
2. **O Zoen paga um assento do ChatGPT por usuário.** O ChatGPT Business custa US$ 20–25 por
   mês por assento ([OpenAI](https://openai.com/api/pricing/)), 400 vezes o custo médio da seção
   5. **Inviável para o consumidor gratuito.**
3. **O Zoen paga a API da OpenAI.** É a mesma coisa que a linha de IA na nuvem da seção 2
   (GPT-6 Luna).

**Decisão sugerida:**
- **padrão:** modelo do aparelho;
- **quando precisar de mais:** Luna ou Haiku 5.5 na nuvem, pago pelo Zoen, com limite por
  usuário;
- **usuário com Plus ou Pro:** pode ligar o próprio ChatGPT e tirar o custo do Zoen.

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

## 6. Custo total por usuário ativo por mês

| linha | baixo | médio | alto |
|---|---|---|---|
| IA na nuvem | 5/mês leve, Luna: US$ 0,001 | 5/dia leve, Luna: **US$ 0,031** | 5/dia pesada, Gemini 3.8 Flash 2027: US$ 1,233 |
| modelo no aparelho | US$ 0 | US$ 0 | US$ 0 (US$ 0,03 uma vez, se baixar pelo Fly) |
| ChatGPT do usuário | US$ 0 | US$ 0 | US$ 0 |
| infraestrutura de mensagens e mídia | US$ 0,002 | US$ 0,010 | US$ 0,029 (padrão Signal, já com SMS) |
| cadastro (SMS, amortizado) | US$ 0 (passkey) | US$ 0,0055 | incluso acima |
| **total por MAU por mês** | **≈ US$ 0,003 (R$ 0,02)** | **≈ US$ 0,047 (R$ 0,24)** | **≈ US$ 1,26 (R$ 6,50)** |

No cenário alto, com o preço promocional do Gemini (até dez/2026), o total cai para ≈ US$ 0,65.

## 7. Receita por usuário: referências públicas

| empresa | métrica | por usuário por mês | fonte |
|---|---|---|---|
| **Telegram, só anúncios** (H1/2025: US$ 125 milhões; 1 bilhão de MAU) | anúncio ÷ MAU | **US$ 0,021** | FT via Yahoo/Cryptonews |
| Telegram, receita total (H1/2025: US$ 870 milhões) | total ÷ MAU | US$ 0,145 | idem |
| Telegram Premium | 15 milhões de pagantes, ≈ 1,5% dos MAU | — | idem |
| **Reddit internacional** (2º tri/2026: ARPU de US$ 2,26 por trimestre) | por DAU | **US$ 0,75** (≈ US$ 0,175 por usuário semanal) | [Reddit Q2'26 (SEC)](https://www.sec.gov/Archives/edgar/data/1713445/000171344526000098/exhibit992q226.htm) |
| Reddit EUA (US$ 11,85 por trimestre) | por DAU | US$ 3,95 | idem |
| Meta, média mundial (ARPP de US$ 16,86 por trimestre, 2º tri/2026) | por pessoa ativa por dia | US$ 5,62 | [Meta Q2'26](https://investor.atmeta.com/investor-news/press-release-details/2026/Meta-Reports-Second-Quarter-2026-Results/) |
| Facebook, "Rest of World" (inclui Brasil; último dado, 4º tri/2023: US$ 4,50 por trimestre) | por usuário | US$ 1,50 | [MacroMicro](https://en.macromicro.me/charts/32591/fb-arpu) (série descontinuada) |
| Discord (2025: US$ 561 milhões estimados; 250 milhões de MAU; ≈ 7,3 milhões no Nitro) | receita ÷ MAU | ≈ US$ 0,19 | [estimativa de terceiros](https://valueaddvc.com/blog/discord-ipo-2026-valuation-s1-filing-when-is-it-happening), sem S-1 público |
| WhatsApp, ARPU no Brasil | — | **não encontrado** (a Meta não separa por app nem por país) | — |

**O dado que mais importa:** o Telegram é o caso mais parecido (anúncio patrocinado em canal,
sem rastrear a pessoa) e fatura **2 centavos de dólar por usuário por mês com anúncio**, e ainda
repassa 50% ao dono do canal ([Telegram](https://core.telegram.org/api/revenue)). O Telegram só
fecha a conta com assinatura Premium e cripto.

## 8. CPM: quanto vale mil exibições

| inventário | CPM | fonte e data |
|---|---|---|
| Banner em app, América Latina (Android e iOS) | **US$ 0,10** | Appodeal, 4º tri/2024, via [YCBT (set/2026)](https://youcanbuildtech.com/studies/how-much-money-can-you-make-with-google-admob/) |
| Banner em app, América do Norte | US$ 0,40–0,50 | idem |
| Intersticial e vídeo premiado, América Latina | US$ 1,30–3,40 | idem (formatos de tela cheia, contra a regra de não anunciar para humanos) |
| Telegram, lance mínimo | 0,1 TON/Gram ≈ US$ 0,13–0,14 | [Neurounit (ago/2026)](https://neurounit.ai/blog/en/how-to-run-telegram-ads/), [yosefk (set/2026)](https://yosefk.me/blog/telegram-ads-complete-guide-2026/) |
| Telegram, mercados emergentes / EUA e Europa | US$ 0,30–0,60 / US$ 10–12 | yosefk (guia de agência, fonte fraca) |
| **Telegram medido, contas em euro** (03/10/2026) | **Brasil €2,22 (≈ US$ 2,48)**, Índia €0,25 (≈ US$ 0,28); média €2,97 | [Adsly (out/2026)](https://adsly.pro/guides/telegram-ads-cpm-by-country/) |
| Meta Ads no Brasil (feed, com segmentação comportamental) | R$ 8–35 ≈ **US$ 1,55–6,80** | [Trafius (2026)](https://trafius.com.br/blog/quanto-custa-anunciar-facebook-instagram) (benchmark de agência) |
| CPM de comunidade tipo Reddit ou Discord no Brasil | **não encontrado** | — |

**O Zoen não terá a segmentação da Meta** (criptografia de ponta a ponta e a regra de não usar o
conteúdo das conversas). Por isso o CPM realista fica entre o banner programático da América
Latina (US$ 0,10) e o Telegram nos emergentes (US$ 0,30–0,60). A loja do bairro comprando direto
pode pagar um pouco mais (US$ 1–2), porque o público é local e não existe outro canal parecido.

## 9. Anúncio por usuário: cenários (Brasil)

Receita = % dos MAU em comunidades × dias ativos × sessões por dia em comunidade × banners por
sessão × taxa de preenchimento × CPM ÷ 1000.

| | baixo | médio | alto |
|---|---|---|---|
| MAU que entram em comunidades | 40% | 60% | 80% |
| dias ativos por mês | 15 | 20 | 25 |
| sessões em comunidade por dia | 1 | 3 | 6 |
| banners por sessão | 1 | 2 | 3 |
| taxa de preenchimento | 30% | 60% | 90% |
| CPM | US$ 0,10 | US$ 0,50 | US$ 2,00 |
| **impressões por MAU por mês** | 1,8 | 43 | 324 |
| **receita bruta por MAU por mês** | **US$ 0,0002** | **US$ 0,022** | **US$ 0,65** |
| líquido, se 50% vai ao dono da comunidade (modelo Telegram) | US$ 0,0001 | US$ 0,011 | US$ 0,32 |

**Mistura global** (versão rápida; a análise por região está na seção 15) (70% Brasil + 30% de mercados pagando ~4,5 vezes o CPM, como sugerem a razão
de banner América do Norte/América Latina da Appodeal e a razão de ARPU EUA/internacional do
Reddit): bruto de **US$ 0,0004 / 0,044 / 1,33**.

O cenário médio bruto (US$ 0,022) bate com o anúncio do Telegram (US$ 0,021), o que sugere que a
premissa é realista.

## 10. Ponto de equilíbrio: impressões necessárias por MAU por mês

Impressões = custo × 1000 ÷ CPM. Se o dono da comunidade ficar com 50%, dobre.

| custo por MAU | CPM US$ 0,10 | US$ 0,50 | US$ 1,00 | US$ 2,00 | US$ 5,00 |
|---|---|---|---|---|---|
| baixo, US$ 0,003 | 30 (1/dia) | 6 | 3 | 1,5 | 0,6 |
| **médio, US$ 0,047** | 470 (16/dia) | **94 (3/dia)** | 47 | 24 | 9 |
| alto, US$ 1,26 | 12.600 (420/dia) | 2.520 (84/dia) | 1.260 (42/dia) | 630 (21/dia) | 252 (8/dia) |

## 11. Comissões e take rate (dados públicos)

| plataforma | taxa | fonte |
|---|---|---|
| Apple no Brasil, compra dentro do app | **21%** + 5% de processamento se usar o pagamento da Apple; **10%** para Small Business, **Mini Apps Partner Program** e assinaturas após o 1º ano; link para a web: 15% (ou 10%); loja alternativa: 5% | [Apple Developer, Brasil](https://developer.apple.com/support/app-distribution-in-brazil/) |
| Google Play (EUA, Reino Unido e Espaço Econômico Europeu desde 30/06/2026) | 10% de serviço no primeiro US$ 1 milhão e em assinaturas, + 5% se usar o faturamento do Google | [Android Developers](https://developer.android.com/blog/posts/expanded-billing-choice-and-lower-fees-on-google-play) |
| Google Play no Brasil | não confirmado se a regra acima vale para o Brasil | — |
| Discord, assinaturas de servidor | Discord fica com 10% (o criador com 90%); no iOS, a Apple cobra mais 30% | [Discord](https://creator-support.discord.com/hc/en-us/articles/10423011974551-Server-Shop-For-Server-Owners-and-Admins) |
| Hotmart | 9,9% + R$ 1,00 por venda (R$ 2,49 desde 21/09/2026); 20% em vendas de até R$ 10 | [Tactus](https://tactus.com.br/taxas-da-hotmart-para-produtor/) |
| iFood | Plano Básico: 12% + 3,2% de pagamento online; Plano Entrega: 23% + 3,2% | [iFood Parceiros](https://blog-parceiros.ifood.com.br/taxas-ifood/) |
| Shopee (vendedor com CNPJ) | 20% + R$ 4,50 até R$ 79,99; acima disso, 14% + taxa fixa | [Shopee Seller](https://seller.shopee.com.br/edu/article/26839/Comissao-para-vendedores-CNPJ-e-CPF-em-2026) |
| Telegram, anúncio em canal | 50% para o dono do canal | [Telegram](https://core.telegram.org/api/revenue) |
| **WhatsApp Business no Brasil (a Meta cobra a empresa, não o consumidor)** | marketing R$ 0,3217; utilidade, autenticação e atendimento (após 1.000 por número) R$ 0,035 por mensagem entregue | [Meta](https://developers.facebook.com/documentation/business-messaging/whatsapp/pricing), [Strat Academy (out/2026)](https://stratacademy.com.br/blog/novos-precos-da-api-do-whatsapp-o-que-mudou-em-outubro-de-2026/) |

**Implicações:**
- **Bens digitais** vendidos dentro do app (bots premium, mini-apps) pagam a Apple antes do
  Zoen. A Apple tem um programa específico para mini-apps com 10% no Brasil, e precisamos
  entrar nele.
- **Bens e serviços físicos** (a padaria vendendo pelo bot) **não pagam a Apple**. É ali que uma
  comissão de 5–15% ao estilo iFood ou Hotmart é viável.
- **A Meta já prova o modelo** que respeita a regra "nunca cobrar o consumidor": cobrar a
  **empresa** por mensagem.

## 12. Outras receitas por MAU (premissas minhas, marcadas para validar)

| alavanca | baixo | médio | alto | premissa |
|---|---|---|---|---|
| comissão na loja (bens digitais) | US$ 0,002 | US$ 0,018 | US$ 0,08 | 1/3/5% dos MAU compram US$ 2/4/8 por mês; o Zoen fica com 10/15/20%. Referências de conversão: Telegram Premium ≈ 1,5%, Discord ≈ 2,9% |
| bot pago para empresa ("Bot Pro") | US$ 0,01 | US$ 0,05 | US$ 0,20 | 2/5/10 empresas pagantes por 1.000 MAU, pagando US$ 5/10/20 por mês (≈ R$ 26/51/103) |
| mensagens iniciadas pela empresa | US$ 0,004 | US$ 0,02 | US$ 0,054 | 2/4/8 mensagens de empresa por MAU por mês, a US$ 0,002/0,005/0,0068 (o teto é o preço de utilidade da Meta) |

O número de empresas por 1.000 MAU **não foi encontrado em fonte pública** e precisa ser medido
no piloto.

## 13. Veredito

- **O anúncio sozinho cobre o cenário baixo** (US$ 0,003): bastam ~6 banners por usuário por mês
  a CPM de US$ 0,50.
- **O anúncio sozinho não cobre o cenário médio** (US$ 0,047):
  - o anúncio médio rende US$ 0,022 bruto, ou US$ 0,011 se dividir com o dono da comunidade;
  - o Telegram, com 1 bilhão de usuários, tira US$ 0,021 por usuário com anúncio;
  - fechar exigiria ~94 banners por usuário por mês a US$ 0,50 de CPM (≈ 3 por dia, para toda a
    base), sem dividir nada com o dono da comunidade.
- **O cenário alto** (Flash pesado, 5 vezes por dia) **não fecha com anúncio em lugar nenhum**:
  daria 21 a 84 banners por dia por usuário.
- **Somando as alavancas de empresa no cenário médio:**
  - anúncio líquido US$ 0,011;
  - loja US$ 0,018;
  - Bot Pro US$ 0,05;
  - mensagens de empresa US$ 0,02.
  - Total ≈ **US$ 0,10 por MAU, cerca de 2 vezes o custo médio**.

## 14. O que precisamos para o modelo fechar

1. **Teto de IA na nuvem por usuário gratuito:**
   - o aparelho primeiro;
   - na nuvem, o tier mais barato (Luna, Haiku 5.5, nano ou 2.5 Flash-Lite) com cache e batch;
   - limite de cerca de US$ 0,03 por MAU por mês.
   O Flash "de verdade" só para empresas pagantes ou para quem trouxe o próprio ChatGPT.
2. **Sign in with ChatGPT:** pedir acesso agora para tirar o usuário avançado da nossa conta.
3. **Cobrar a empresa, nunca o consumidor:**
   - bot grátis para pequeno negócio até X conversas por mês;
   - Bot Pro pago acima disso;
   - tarifa por mensagem iniciada pela empresa, abaixo da Meta (R$ 0,035 em utilidade e R$ 0,32
     em marketing). Esse é o "WhatsApp 2" com preço justo.
4. **Comissão sobre comércio físico** pelos bots (pedido, agendamento, Pix): 3–8%, bem abaixo
   do iFood (15–26%). Não paga a Apple.
5. **Loja de apps de IA:** entrar no Mini Apps Partner Program da Apple (10% no Brasil) e cobrar
   10–20% do desenvolvedor, como o Discord (10%) e a Hotmart (~10%).
6. **Anúncio em comunidade:**
   - venda direta para lojas locais por bairro e tema, a CPM de US$ 1–2;
   - divisão com o dono da comunidade (o Telegram dá 50%; começar com 30–50%).
   O programático a US$ 0,10 não vale o espaço.
7. **Métricas para medir desde o primeiro dia:**
   - DAU/MAU;
   - sessões em comunidade por dia;
   - banners vistos por sessão;
   - preenchimento e CPM por bairro;
   - chamadas de IA na nuvem por usuário (e % resolvidas no aparelho);
   - custo de IA por MAU;
   - empresas pagantes por 1.000 MAU;
   - mensagens de empresa por MAU;
   - GMV e take rate;
   - % de cadastros sem SMS.

## 15. Internacional ("Pense nos gringos")

Mesmo modelo das seções 9 e 12, região por região. O engajamento é o mesmo em todas as regiões
(1,8 / 43 / 324 banners por MAU por mês). O que muda é o CPM e quanto as empresas pagam.
Câmbio: €1 = US$ 1,1186 (BCE, 08/10/2026); 1 TON/Gram ≈ US$ 1,30 (meados de set/2026).

### 15.1 CPM de banner e anúncio nativo por região (US$)

| região | baixo | médio | alto | de onde vem |
|---|---|---|---|---|
| EUA | 0,40 | 0,68 | 2,00 | baixo: Appodeal, banner América do Norte (4º tri/2024); médio: SpinX, banner Android nos EUA (2025); alto: MWM, banner iOS nos EUA, topo da faixa (2026) |
| Europa | 0,20 | 0,35 | 1,50 | baixo: Appodeal Europa; médio: Telegram no Reino Unido e Alemanha, 0,27 TON (Adsly, out/2026); alto: **estimativa minha** (MWM diz "parecido com os EUA, um pouco abaixo") |
| Índia | 0,10 | 0,28 | 0,50 | baixo: Appodeal APAC; médio: Telegram Índia €0,25 (Adsly); alto: MWM, banner Android Índia/Brasil, topo da faixa |
| América Latina | 0,10 | 0,50 | 2,48 | baixo: Appodeal América Latina; médio: MWM Índia/Brasil, topo; alto: Telegram Brasil €2,22 (Adsly) |

**Cuidados com a tabela:**
- O "alto" da América Latina (Telegram medido) passa o "alto" dos EUA (banner). São fontes e
  formatos diferentes, não uma prova de que o Brasil paga mais que os EUA.
- O Telegram nos EUA e na Europa só foi medido em conta TON, cujo leilão é bem mais barato.
- Anúncio nativo costuma render 2–3 vezes o banner (MWM).
- **Não encontrado:** CPM de banner em comunidade por país em fonte pública auditada.

### 15.2 Quanto cada região rende hoje para outras empresas (referências de ARPU)

| região | Facebook, ARPU mensal (4º tri/2023, último publicado) | Reddit, ARPU mensal por usuário diário (2º tri/2026) | WhatsApp, mensagem de marketing / utilidade (out/2026) |
|---|---|---|---|
| EUA e Canadá | US$ 22,81 | US$ 3,95 | US$ 0,0250 / 0,0034 |
| Europa | US$ 7,71 | internacional: US$ 0,75 | Alemanha 0,1365 / 0,0550; França 0,0859 / 0,0300 |
| Ásia-Pacífico (inclui Índia) | US$ 1,84 | idem | Índia 0,0118 / 0,0014 |
| Resto do mundo (inclui América Latina) | US$ 1,50 | idem | Brasil 0,0625 / 0,0068; México 0,0397 / 0,0085 |

- Na receita de anúncio da Meta no 2º tri/2026, EUA e Canadá são 44,1%, Europa 23,5%,
  Ásia-Pacífico 18,4% e resto do mundo 14,0% (apresentação de resultados).
- A diferença de ARPU entre EUA e Índia ou América Latina é de **~12–15 vezes**. A do CPM de
  banner é de só **2–7 vezes**: banner é barato em todo lugar.

### 15.3 Receita por MAU por mês, por região

**Premissas por região (minhas, para validar):**
- **Bot Pro** (2/5/10 empresas pagantes por 1.000 MAU), mensalidade:
  - EUA US$ 10/20/40;
  - Europa US$ 8/15/30;
  - América Latina US$ 5/10/20;
  - Índia US$ 1/3/6.
- **Loja**, gasto por comprador por mês:
  - EUA US$ 4/8/16;
  - Europa US$ 3/6/12;
  - América Latina US$ 2/4/8;
  - Índia US$ 0,5/1/2.
- **Mensagens de empresa:** o Zoen cobra **metade da tarifa de utilidade da Meta em cada país**
  (Europa pela tarifa da França e da Itália).

| região | cenário | anúncio | loja | Bot Pro | mensagens de empresa | **total** |
|---|---|---|---|---|---|---|
| EUA | baixo | 0,0007 | 0,0040 | 0,020 | 0,0034 | **0,028** |
| EUA | médio | **0,029** | 0,036 | 0,100 | 0,0068 | **0,172** |
| EUA | alto | 0,648 | 0,160 | 0,400 | 0,0136 | **1,22** |
| Europa | baixo | 0,0004 | 0,0030 | 0,016 | 0,030 | **0,049** |
| Europa | médio | **0,015** | 0,027 | 0,075 | 0,060 | **0,177** |
| Europa | alto | 0,486 | 0,120 | 0,300 | 0,120 | **1,03** |
| Índia | baixo | 0,0002 | 0,0005 | 0,002 | 0,0014 | **0,004** |
| Índia | médio | **0,012** | 0,0045 | 0,015 | 0,0028 | **0,034** |
| Índia | alto | 0,162 | 0,020 | 0,060 | 0,0056 | **0,25** |
| América Latina | baixo | 0,0002 | 0,0020 | 0,010 | 0,0068 | **0,019** |
| América Latina | médio | **0,022** | 0,018 | 0,050 | 0,0136 | **0,103** |
| América Latina | alto | 0,804 | 0,080 | 0,200 | 0,0272 | **1,11** |

Valores em US$, sem descontar a parte do dono da comunidade (de 0 a 50% do anúncio).

### 15.4 Mistura global

| mistura de usuários | anúncio, médio | total, médio | anúncio, alto | total, alto |
|---|---|---|---|---|
| A: Brasil primeiro (70% América Latina, 10% EUA, 10% Europa, 10% Índia) | 0,021 | **0,111** | 0,69 | 1,03 |
| B: global tipo WhatsApp (20% América Latina, 10% EUA, 20% Europa, 50% Índia e emergentes, usando o CPM da Índia) | 0,016 | **0,090** | 0,40 | 0,67 |
| C: ocidental (20% América Latina, 40% EUA, 40% Europa) | 0,022 | **0,160** | 0,61 | 1,12 |

### 15.5 Veredito internacional

- **Só anúncio não cobre o custo médio (US$ 0,047) em nenhuma região** com o engajamento médio.
  - EUA: US$ 0,029, o melhor caso;
  - Europa: US$ 0,015;
  - Índia: US$ 0,012.
  - Nos EUA, fechar exige ~70 banners por usuário por mês a US$ 0,68. Na Índia, ~170 a US$ 0,28.
- **Com as alavancas de empresa:**
  - **EUA, Europa e América Latina fecham com folga** (2 a 4 vezes o custo médio);
  - **a Índia não fecha** (US$ 0,034 contra US$ 0,047).
  Em mercados assim, o usuário gratuito precisa de um teto de IA na nuvem bem menor, ~US$ 0,01
  por mês, com quase tudo resolvido pelo modelo no aparelho.
- **Na Europa, o que mais rende são as mensagens de empresa**: a tarifa de utilidade da Meta na
  Alemanha e na França é de US$ 0,03–0,055, 4 a 8 vezes a do Brasil. Um preço abaixo da Meta tem
  muito espaço por lá.
- **Na mistura global B**, a receita fica em ≈ US$ 0,09 por usuário por mês: cobre o custo
  médio, mas com pouca folga. Só o anúncio (US$ 0,016) cobre um terço do custo.
- **O que fazer:**
  - teto de IA por região, ligado ao CPM e ao ARPU locais;
  - preço de Bot Pro e de mensagens de empresa por país, ancorado na tabela da Meta;
  - venda direta de anúncio primeiro nos EUA e na Europa, onde o CPM médio é maior.

## Fontes (lidas em 09/out/2026)

- OpenAI API pricing: https://platform.openai.com/docs/pricing
- OpenAI ChatGPT Business: https://openai.com/api/pricing/
- Google Gemini API pricing (atualizada em 07/10/2026): https://ai.google.dev/gemini-api/docs/pricing
- Anthropic pricing: https://docs.anthropic.com/en/docs/about-claude/pricing
- DeepSeek pricing: https://api-docs.deepseek.com/quick_start/pricing
- xAI models: https://docs.x.ai/docs/models
- Sign in with ChatGPT: https://help.openai.com/en/articles/20001410-sign-in-with-chatgpt, https://developers.openai.com/siwc/quickstart, https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt
- Apple Foundation Models: https://developer.apple.com/documentation/foundationmodels
- Gemini Nano e ML Kit GenAI: https://developers.google.com/ml-kit/genai, https://developer.android.com/ai/gemini-nano
- Cloudflare R2: https://developers.cloudflare.com/r2/pricing/
- Signal: https://signal.org/blog/signal-is-expensive/ (nov/2023); TechCrunch: https://techcrunch.com/2023/11/17/signal-costs/
- Twilio Verify: https://www.twilio.com/en-us/verify/pricing; SMS no Brasil: https://www.twilio.com/en-us/sms/pricing/br
- Telegram, finanças (FT, jan/2026): https://finance.yahoo.com/news/telegram-revenue-surges-65-870m-160000131.html, https://cryptonews.com/news/telegram-revenue-870m-toncoin-bond-freeze/
- Telegram Ads: https://core.telegram.org/api/revenue, https://neurounit.ai/blog/en/how-to-run-telegram-ads/, https://yosefk.me/blog/telegram-ads-complete-guide-2026/
- Reddit, carta aos acionistas do 2º tri/2026 (SEC): https://www.sec.gov/Archives/edgar/data/1713445/000171344526000098/exhibit992q226.htm
- Meta, 2º tri/2026: https://investor.atmeta.com/investor-news/press-release-details/2026/Meta-Reports-Second-Quarter-2026-Results/; ARPU regional (descontinuado): https://en.macromicro.me/charts/32591/fb-arpu
- Discord (estimativas): https://valueaddvc.com/blog/discord-ipo-2026-valuation-s1-filing-when-is-it-happening; Server Subscriptions: https://creator-support.discord.com/hc/en-us/articles/10423011974551-Server-Shop-For-Server-Owners-and-Admins
- Appodeal eCPM, 4º tri/2024, via YCBT: https://youcanbuildtech.com/studies/how-much-money-can-you-make-with-google-admob/
- Meta Ads, CPM no Brasil: https://trafius.com.br/blog/quanto-custa-anunciar-facebook-instagram
- Apple no Brasil: https://developer.apple.com/support/app-distribution-in-brazil/
- Google Play: https://developer.android.com/blog/posts/expanded-billing-choice-and-lower-fees-on-google-play
- Hotmart: https://tactus.com.br/taxas-da-hotmart-para-produtor/; iFood: https://blog-parceiros.ifood.com.br/taxas-ifood/; Shopee: https://seller.shopee.com.br/edu/article/26839/Comissao-para-vendedores-CNPJ-e-CPF-em-2026
- WhatsApp Business: https://developers.facebook.com/documentation/business-messaging/whatsapp/pricing, https://stratacademy.com.br/blog/novos-precos-da-api-do-whatsapp-o-que-mudou-em-outubro-de-2026/
- Telegram Ads, CPM medido por país (03/10/2026): https://adsly.pro/guides/telegram-ads-cpm-by-country/, https://adsly.pro/guides/telegram-ads-in-brazil/
- Banner por região em 2026: MWM https://mwm.ai/glossary/banner-ad; SpinX (2025) https://spinx.io/blog/ecpm-by-country-in-2025-the-map-every-mobile-publisher-needs
- WhatsApp, tarifas por país (out/2026): https://monochat.ai/blog/whatsapp-business-api-pricing, https://instantdm.com/blog/whatsapp-business-api-pricing-2026-india-october-update
- Meta, receita por região no 2º tri/2026: https://s21.q4cdn.com/399680738/files/doc_financials/2026/q2/Earnings-Presentation-Q2-2026.pdf
- Apple Mini Apps Partner Program (15% fora do Brasil): https://developer.apple.com/programs/mini-apps-partner/
- Câmbio do BCE via Frankfurter (08/10/2026): https://api.frankfurter.dev/v1/latest?from=EUR&to=USD
- Internas: docs/adr/0022-capacity.md, docs/cost-model.md

**Não encontrado:**
- ARPU do WhatsApp no Brasil;
- custo de infraestrutura do WhatsApp e do Telegram;
- CPM de banner em comunidade (estilo Reddit ou Discord) no Brasil;
- o modelo "LM 5.3 Flash";
- o tier barato da xAI;
- a parcela de aparelhos no Brasil com Gemini Nano;
- se a nova taxa do Google Play vale para o Brasil;
- empresas pagantes por 1.000 usuários em mensageiros;
- CPM auditado de banner em comunidade por país;
- CPM do Telegram nos EUA e na Europa em conta euro.

## Como reproduzir

`python3 docs/research/unit_economics_calc.py` imprime todas as tabelas de custo, anúncio e
ponto de equilíbrio a partir das premissas no topo do arquivo.
