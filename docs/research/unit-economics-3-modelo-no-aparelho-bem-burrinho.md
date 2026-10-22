[Research index](unit-economics.md).

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
