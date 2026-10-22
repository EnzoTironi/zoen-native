[Research index](unit-economics.md).

## 17. LGPD, dados na China e rodar o GLM nas nossas GPUs

### 17.1 API da Z.ai e de outras empresas chinesas

**O que a lei exige:**
- A LGPD só permite enviar dados para fora do Brasil com uma base legal do art. 33.
- A ANPD **não reconheceu nenhum país como adequado**, nem a China.
- Na prática, o caminho são as **cláusulas-padrão da Resolução CD/ANPD nº 19/2024**, adotadas sem
  alteração. O prazo de adaptação terminou em 23/08/2025
  ([ANPD](https://www.gov.br/anpd/pt-br/assuntos/assuntos-internacionais/transferencia-internacional-de-dados),
  [Mayer Brown](https://www.mayerbrown.com/pt/insights/publications/2025/08/end-of-grace-period-implementation-of-brazils-standard-contractual-clauses-in-international-transfers-of-personal-data)).

**Os riscos:**
- Mandar conteúdo de conversa para a API da Z.ai, DeepSeek, Moonshot ou MiniMax é transferência
  internacional, e o fornecedor teria de assinar essas cláusulas. Não verifiquei se algum deles
  assina.
- Essas empresas estão sob a jurisdição chinesa de acesso governamental a dados. É um risco de
  reputação e de confiança para um app cujo diferencial é a privacidade.
- **Isso entra em conflito com a criptografia de ponta a ponta:** qualquer IA na nuvem vê o texto
  que recebe. A IA na nuvem tem de ser escolha explícita do usuário, nunca o padrão nas conversas
  privadas.

**Saídas, sem deixar o GLM:**
1. **GLM-5.3-Flash pelo Cloudflare Workers AI**, mesmo preço (US$ 0,15 / 0,03 / 0,50): o
   processamento fica com uma empresa americana que já é fornecedora do Zoen (R2). Ainda é
   transferência internacional, mas sem passar por fornecedor chinês. Falta confirmar se o
   Cloudflare roda inferência no Brasil.
2. **Qwen-Flash na região dos EUA (Virgínia) da Alibaba**, US$ 0,05 / 0,40. Reduz o risco, mas a
   Alibaba continua sendo empresa chinesa.
3. **Rodar o modelo aberto do GLM (licença MIT) nas nossas GPUs**, de preferência em São Paulo: os
   dados não saem do nosso controle. Custo na seção 17.2.

### 17.2 Custo de rodar o GLM-5.3-Flash nas nossas GPUs

- **Hardware:** o modelo inteiro não cabe numa H100 em qualidade boa (precisa de ~215 GB em Q4)
  ([willitrunai](https://willitrunai.com/can-run/glm-5.3-flash-on-h100-80gb)). Usei **4 × H100**
  com a versão quantizada W4A16.
- **Velocidade:** 689–1.161 tokens de saída por segundo com 32 pedidos simultâneos, cada um com 8k
  de entrada e 1k de saída. É um teste da comunidade, não da Z.ai
  ([canada-quant/GLM-5.3-Flash-W4A16-MTP](https://huggingface.co/canada-quant/GLM-5.3-Flash-W4A16-MTP/commit/65c3233705daabd93415fa6586a12b25b81940de)).
- **Preço da H100:** US$ 2,69 por hora (Runpod SXM community) a US$ 3,99 (Lambda)
  ([Runpod](https://www.runpod.io/pricing), [CloudZero, ago/2026](https://www.cloudzero.com/blog/h100-gpu-cost/)).
- **Como fiz a conta:** o custo de uma chamada é proporcional ao total de tokens (leve = 1,8k de
  9,2k do teste). É uma aproximação minha.

| configuração | custo por chamada leve | 150 chamadas por MAU por mês |
|---|---|---|
| API Z.ai ou Cloudflare (com cache) | US$ 0,00025 | **US$ 0,037** |
| GPUs próprias, US$ 2,69/h, 1.161 tok/s, 100% de uso | US$ 0,00051 | US$ 0,077 |
| GPUs próprias, US$ 2,69/h, 1.161 tok/s, 50% de uso | US$ 0,00103 | US$ 0,154 |
| GPUs próprias, US$ 3,99/h, 689 tok/s, 50% de uso | US$ 0,00257 | US$ 0,386 |

**Leitura:**
- No tamanho de hoje, rodar nas próprias GPUs custa **2 a 10 vezes mais** que a API.
- Só se aproxima da API com GPUs reservadas ou próprias, uso alto e mais pedidos simultâneos que
  os 32 do teste. Não achei medição pública acima de 32.

**Recomendação:**
- **para o começo:** API pelo Cloudflare, com consentimento explícito e sem conversas privadas;
- **depois:** GPUs próprias em São Paulo quando a escala sustentar uso alto, ou quando um cliente
  empresa exigir que os dados fiquem no Brasil (empresa paga o premium);
- **sempre:** o modelo do aparelho primeiro.
