[Research index](unit-economics.md).

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
  - **"LM 5.3 Flash"** é o **GLM-5.3-Flash** da Zhipu (Z.ai), confirmado pelo Enzo e na página de
    preços da Z.ai. É um modelo aberto (licença MIT) de 320 bilhões de parâmetros, dos quais 18
    bilhões ficam ativos por vez. É o **modelo padrão do plano grátis**.
