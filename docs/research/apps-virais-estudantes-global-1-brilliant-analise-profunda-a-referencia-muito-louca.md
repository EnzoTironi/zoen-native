[Research index](apps-virais-estudantes-global.md).

## 1. BRILLIANT: análise profunda (a "referência muito louca")

### 1.1 Números-chave

| Métrica | Valor | Fonte / data |
|---|---|---|
| Fundação | 2012, Sue Khim (São Francisco) | Wikipedia "Brilliant.org" |
| Usuários | 100K (2013) → **10M+** (2025) | Wikipedia |
| Avaliações | "over 100,000 5-star reviews" | brilliant.org/subscribe (09/10/2026) |
| App Store EUA | **32.873 avaliações, nota 4,73**: https://apps.apple.com/us/app/brilliant-learn-math-coding/id913335252 | iTunes Lookup, 09/10/2026 |
| Google Play | selo 5M+, **7,30M installs reais**, nota 4,52: https://play.google.com/store/apps/details?id=org.brilliant.android | google-play-scraper, 09/10/2026 |
| Installs Android nos últimos 30 dias | **93K** (1,3% da base) | AppBrain, 09/10/2026 |
| Ranking iOS | **#78 grossing Education EUA**; **#31 grossing Education Índia**; fora do top 100 free EUA | Apple RSS, 09/10/2026 00:47 BRT |
| Melhor ranking Android | #20 grossing Education Índia | AppBrain |
| Receita | US$14,3M ARR (2024, GetLatka); US$18,3M (Growjo) | **estimativa de terceiro, não verificada** |
| Funding | US$15M (2020); Série C (mai/2022, IFC, Next Play); total ~US$25–33M conforme a fonte; valuation de US$50M em abr/2019 | Wikipedia, imprensa |
| Origem do tráfego web | 48% direto, 21% busca, 15% social, 3% pago | Crustdata, estimativa de terceiro |

### 1.2 Preço e paywall

- **Modelo:** freemium com limite diário. O free tier dá acesso a lições com **limite diário** e um "preview limitado" do tutor de IA **Koji**. O Premium libera lições ilimitadas, pular para qualquer lição, Koji completo e uso sem anúncios (Help Center, atualizado em 28/09/2026: https://brilliant.org/help/pricing-and-plans/what-is-brilliant-premium-and-how-do-i-subscribe/).
- **Planos:** mensal, anual (desconto) e **Family anual** com até 6 membros. A página oficial não exibe preço no HTML; ele aparece só no checkout.
- **Preços citados:** US$24,99/mês ou US$299,88/ano (Wikipedia, 2025). Terceiros citam ~US$20/mês no plano anual (US$240), US$30 no mensal e ~US$40 no Family (Brighterly). Isso é **estimativa de terceiro, não verificada**, e pode variar por país e promoção.
- **Trial:** "sometimes offers free trial promotions", ou seja, nem sempre há trial, e ele exige cartão.
- **Educação:** professores e alunos de K-12 podem ganhar Premium grátis (Brilliant for Educators). É uma porta de entrada institucional.

### 1.3 Por que funciona: o design das lições

1. **Problema primeiro, nome depois.** Cada lição começa com um desafio "na borda" do que o aluno sabe, resolvido por manipulação (arrastar, soltar, girar, montar equações com peças). Só depois o conceito ganha nome. É aprendizagem por fazer, não por ler.
2. **"Solvables" com retry e feedback imediato.** A redesign documentada por Paige Ormiston (https://www.paigeormiston.com/brilliant) padronizou perguntas interativas que dão XP e permitem tentar de novo, com banners de feedback. Resultado reportado: **+8,1% em daily active learners e +7% em conclusão de lições**.
3. **"Game Feel" (com a ustwo).** A North Star foi fazer o app *parecer um jogo* sem virar jogo: trilha de aprendizagem, "Level Gameboard", celebrações e um companheiro de aprendizagem. O foco era retenção depois da primeira semana; a Brilliant reportou ganho de engajamento dia a dia e semana a semana, principalmente no mobile (https://ustwo.com/work/brilliant/).
4. **Streaks animados (Rive).** As animações de streak, as celebrações e as trilhas por cor rodam iguais em iOS, Android e web a partir de um único asset (https://rive.app/blog/how-brilliant-org-motivates-learners-with-rive-animations).
5. **IA gerando puzzles com evals de 100% de correção.** O blog de engenharia (27/02/2025) descreve LLMs gerando milhares de puzzles interativos (balanças, equações com peças, lógica, código drag-and-drop), filtrados por *evals* automáticos: correção matemática, solução única, clareza visual, estados impossíveis e plausibilidade física. Os puzzles "foundational" passam em 80–90% e os reprovados nem chegam à revisão humana. "O que levava semanas agora leva horas" (https://blog.brilliant.org/when-almost-right-is-catastrophically-wrong-evals-for-ai-learning-games/).
6. **Koji, um tutor que não entrega a resposta.** Ele vê o estado do puzzle e guia o raciocínio passo a passo.

### 1.4 Marketing: a máquina de patrocínio no YouTube

- **SponsorRadar:** **1.905 deals rastreados em 378 canais** desde mar/2021, ~45 por mês; 53% vão para canais com mais de 1M de inscritos; o último deal rastreado é de set/2026 (https://sponsorradar.com/brands/brilliant-org/get-sponsored).
- **Influencer Advisory:** 1.983 posts pagos com 572 criadores. Recorrentes: Newsthink (72) e Sabine Hossenfelder (33 desde 2024).
- **Flywheel:** 342 vídeos de 30 criadores, 20 deles recorrentes. **Só a Veritasium tem 36 vídeos e 579M de views.**
- **Formato:** patrocina desde 2017, muito em *post-roll*, com a oferta-padrão "**20% off para os primeiros 200**" no plano anual via link próprio do criador (ThoughtLeaders).
- **Redes próprias (09/10/2026):** YouTube @BrilliantOrg **82,6K inscritos**; TikTok @brilliantorg **124 seguidores** (praticamente inativo); Instagram **não encontrado** (exige login, e o Viralist não indexa). **Conclusão: a Brilliant não constrói audiência própria. Ela aluga a audiência de criadores de ciência e matemática**, e por isso o tráfego "direto" é de 48%.

### 1.5 O que Zoen pode copiar da Brilliant

1. **Mini-apps de lição interativa dentro do chat ou do Space.** Um "Solvable" curto (arrastar, montar, escolher), com feedback na hora e retry, no lugar de um texto do agente. O agente gera e um *eval* automático valida antes de mostrar, como no pipeline da Brilliant.
2. **"Problema antes da teoria" como padrão dos agentes de estudo.** O agente propõe um desafio na borda do nível do aluno e só depois explica.
3. **Game feel leve:** streak animado, trilha visual por Space e celebração ao fechar uma meta, sem virar joguinho.
4. **Paywall por limite diário, não por feature:** grátis com N lições ou sessões de agente por dia; Premium ilimitado mais tutor completo. Plano Family/Grupo para até 6 pessoas, que no nosso caso vira um "plano de grupo de estudo".
5. **Patrocínio de criadores com cupom rastreável** ("20% para os primeiros 200"), mirando os YouTubers de concurso e vestibular no Brasil em vez de construir audiência própria do zero.
6. **Porta institucional:** grátis para professores e cursinhos parceiros, que trazem as turmas.

---
