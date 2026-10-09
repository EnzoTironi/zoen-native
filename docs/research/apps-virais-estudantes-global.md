# Apps virais para estudantes e "concurseiros" fora do Brasil (EUA, Índia, China, Coreia, Japão) + Brilliant como referência

> Pesquisa web pública (sem login, sem ferramentas pagas). Coleta em **09/10/2026 (BRT)**.
> Complementa o relatório Brasil: [`apps-virais-estudantes-concurseiros.md`](./apps-virais-estudantes-concurseiros.md).
> Regra: nenhum número inventado. O que não foi achado em fonte pública está marcado **não encontrado**. Estimativas de terceiros (GetLatka, Growjo etc.) estão marcadas **estimativa de terceiro, não verificada**.

---

## 0. LEAD: quem mais está crescendo em installs nos últimos meses (mai–out/2026)

### 0.1 O que dá (e o que não dá) para medir com fonte pública

- **Série mensal de downloads mai–out/2026 por país (Brasil/EUA/Índia): não encontrado** em fonte aberta. Sensor Tower, Appfigures, AppMagic, Apptopia e data.ai só mostram isso atrás de paywall/login; o Peekly fica atrás de um checkpoint anti-bot, e não contornamos.
- O melhor proxy público e reproduzível foi o **AppBrain**, que mostra por app Android **"Installs, last 30 days"** (≈ 09/set → 09/out/2026, **global**, só Google Play) e o total acumulado. Lemos tudo em 09/10/2026. URL de cada app: `https://www.appbrain.com/app/<pacote>`.
- O **"% do total em 30 dias"** (installs nos últimos 30 dias ÷ installs acumulados) mede *viralidade recente*: app novo explodindo tem % alto, e app maduro tem % baixo mesmo com volume alto. Esse indicador é cálculo nosso sobre os números do AppBrain.
- Também usamos: snapshots dos rankings Apple (RSS oficial, 09/10/2026 00:47 BRT), marcos de crescimento divulgados por imprensa ou fundadores, e um post público da Sensor Tower.

### 0.2 Global/EUA/Índia: ranking por installs Android nos últimos 30 dias (AppBrain, lido em 09/10/2026)

| # | App (foco) | Installs últimos 30 dias | Total acumulado | % do total em 30 dias | Sinal extra |
|---|---|---|---|---|---|
| 1 | Duolingo (idiomas, referência) | 17M | 980M | 1,7% | #1 free Education EUA, #2 grossing Índia |
| 2 | **Gauth** (ByteDance, foto→resposta com IA) | **4,7M** | 120M | 3,9% | #3 free / #11 grossing Education EUA |
| 3 | **Astra AI** (Eslovênia, tutor IA, forte no BR) | **1,4M** | 6,6M | **21%** (o maior da lista) | #23 free Education BR |
| 4 | Physics Wallah (Índia, JEE/NEET) | 1,2M | 59M | 2,0% | #10 free / #5 grossing Education Índia |
| 5 | Notion (produtividade) | 890K | 44M | 2,0% | — |
| 6 | Knowunity (resumos de alunos + IA) | 850K | 24M | 3,5% | #27 free Education EUA |
| 7 | Quizlet | 810K | 63M | 1,3% | #9 free / **#4 grossing** Education EUA |
| 8 | Photomath | 790K | 280M | 0,3% | #56 free / #31 grossing EUA |
| 9 | Khan Academy | 620K | 34M | 1,8% | #60 free EUA |
| 10 | Testbook (Índia, concursos) | 600K | 54M | 1,1% | #11 free Education Índia |
| 11 | AnkiDroid | 530K | 24M | 2,2% | AnkiMobile (iOS) #43 grossing EUA, #20 Índia, #13 Coreia |
| 12 | Forest (foco) | 420K | 49M | 0,9% | — |
| 13 | Adda247 (Índia, concursos) | 340K | 34M | 1,0% | #23 free Education Índia |
| 14 | **Gizmo** (flashcards IA, EUA/UK) | 290K | 5,3M | **5,5%** | #13 free / #26 grossing EUA |
| 15 | YPT / 열품타 (timer + grupos de estudo, Coreia) | 250K | 7,6M | 3,3% | #18 grossing Education Coreia |
| 16 | Brainly | 250K | 300M | 0,1% | — |
| 17 | Unacademy (Índia) | 190K | 110M | 0,2% | #77 free Índia (em queda) |
| 18 | Solvely (foto→resposta IA) | 150K | 6,1M | 2,5% | #33 free / #41 grossing EUA |
| 19 | **Brilliant** | **93K** | 7,3M | 1,3% | #78 grossing EUA, #31 grossing Índia |
| 20 | Turbo AI (notas de aula com IA) | 79K | 3M | 2,6% | #64 free EUA |
| 21 | Studocu | 54K | 3,6M | 1,5% | — |
| 22 | StudyFetch | 48K | 880K | **5,5%** | #90 free EUA |
| 23 | Marrow (NEET PG) | 48K | 2,4M | 2,0% | **#3 grossing** Education Índia |
| 24 | Question.AI | 49K | 27M | 0,2% | #40 grossing EUA (receita vem do iOS) |
| 25 | QANDA (Coreia) | 49K | 82M | 0,06% | #12 grossing Coreia |
| 26 | Coconote (gravação de aula → notas) | 43K | 570K | **7,5%** | #47 grossing EUA |
| 27 | BYJU'S | 18K | 130M | 0,01% | colapso (ver §3) |
| 28 | Chegg Study | 8,8K | 11M | 0,08% | #53 grossing EUA, em queda livre |

Fonte: páginas públicas do AppBrain, por exemplo https://www.appbrain.com/app/org.brilliant.android, lidas em 09/10/2026. Os rankings vêm do Apple RSS (`https://itunes.apple.com/{us|in|cn|kr|jp}/rss/top{free|grossing}applications/limit=100/genre=6017/json`), capturados em 09/10/2026 00:47 BRT.

**Leitura rápida:**
1. **A onda é "IA que resolve, resume ou grava a aula".** Gauth, Astra, Gizmo, Coconote, StudyFetch e Turbo AI concentram o maior % de crescimento recente. Quem cresce mais rápido em termos relativos é Astra AI (21% da base instalada em um único mês), seguido de Coconote (7,5%), Gizmo e StudyFetch (5,5%).
2. **Gauth é a máquina de volume.** A Sensor Tower publicou que, de 22/out a 21/nov/2025, o Gauth foi o **#1 em downloads de educação nos EUA (~1,3M)**, à frente do Duolingo (~1,1M), mas com ~5,9M WAU contra ~19M do Duolingo (https://develop.sensortower.com/blog/top-5-education-apps-us-20251121). Ou seja: baixa muito e retém menos.
3. **Na Índia, o dinheiro está no concurso médico.** ZyNerd (NEET) é **#1 grossing** Education Índia, Marrow (NEET PG) é #3, PW #5, Cerebellum #12, DocTutorials #18 e PW MedEd #19. Os apps generalistas (Unacademy, BYJU'S) estão afundando.
4. **Os incumbentes "pergunta→resposta" estão derretendo.** Chegg e Brainly somam menos de 0,1% da base em 30 dias. O Chegg perdeu 40% dos assinantes em um ano (ver §2).

**Marcos públicos de crescimento (mai–out/2026 e entorno):**

| App | Marco | Fonte / data |
|---|---|---|
| Astra AI | 1,5M usuários (jan/2026) → 4M (mar) → **12M** (set, "3 anos de Astra AI, 12M users") → 13M (13/set); o CEO fala em "+150 mil novos usuários/dia" (out) | Posts do CEO Andrej P. Skraba no LinkedIn, ex.: https://www.linkedin.com/posts/andrejskraba_3-years-of-building-astra-ai-12m-users-activity-7503691406864134144-Wkm6. Números autodeclarados. |
| Yik Yak + Sidechat | ~2,2M usuários únicos por semestre, "~15% de todos os graduandos dos EUA" | Northeastern Global News, 26/05/2026: https://news.northeastern.edu/2026/05/26/ceo-yik-yak-social-media/ |
| Fizz | 700+ campi (dez/2025), usuários dobraram desde o outono de 2024 | AdExchanger, dez/2025 |
| Fizz | 620+ campi, US$41M captados, DAU quadruplicou em um mês no back-to-school de 2025 | TechCrunch, 03/09/2025: https://techcrunch.com/2025/09/03/college-social-app-fizz-expands-into-grocery-delivery/ |
| Gauth | ~1,3M downloads nos EUA em 30 dias (#1 Education) | Sensor Tower, nov/2025 (link acima) |
| Downloads mensais por app, mai–out/2026 | **não encontrado** (dados pagos) | — |

### 0.3 Brasil: ranking por installs recentes (mesma metodologia)

Para apps focados no Brasil, o número "global" do AppBrain é, na prática, quase todo brasileiro. Os rankings BR vêm do relatório Brasil (Apple RSS BR, 09/10/2026 00:34 BRT).

| # | App | Installs últimos 30 dias | Total | % em 30 dias | Sinal BR |
|---|---|---|---|---|---|
| 1 | Gauth (global; fatia BR não encontrada) | 4,7M | 120M | 3,9% | #65 free / #94 grossing Education BR |
| 2 | **Astra AI** (global, forte no BR) | 1,4M | 6,6M | 21% | #23 free / #72 grossing Education BR (iOS) |
| 3 | Brainly | 250K | 300M | 0,1% | melhor rank AppBrain: #6 grossing Education BR |
| 4 | **Thea** (estudo IA) | 120K | 1,5M | **8%** | #39 free / #66 grossing BR; melhor rank Android: #26 grossing |
| 5 | Qconcursos | 79K | 3,3M | 2,4% | melhor rank Android: **#2 grossing** Education BR |
| 6 | Gran Questões | 44K | 3,6M | 1,2% | #21 grossing BR |
| 7 | Gran Online | 28K | 2,8M | 1,0% | — |
| 8 | Umatch (dating universitário) | 17K | 1,4M | 1,2% | #17 grossing Dating BR |
| 9 | Tec Concursos | 16K | 410K | 3,9% | #10 grossing BR |
| 10 | Noji (flashcards) | 16K | 2,4M | 0,7% | — |
| 11 | EmÁudio (concurso em áudio) | 16K | 840K | 1,9% | — |
| 12 | Estratégia | 12K | 1M | 1,2% | — |
| 13 | Aprovado | 9,5K | 1,7M | 0,6% | — |
| 14 | Gênio IA | 4,4K | 370K | 1,2% | — |
| 15 | Passei Direto | 3,8K | 17M | 0,02% | estagnado |
| — | Descomplica | não encontrado | 2,7M | — | — |

**Leitura BR:** os apps de IA estrangeiros (Astra, Gauth) e a Thea, nacional, crescem bem mais rápido que os de concurso. Os apps de concurso crescem pouco em installs, mas **monetizam muito** (Qconcursos chegou a #2 grossing). Série mensal mai–out/2026 por app no Brasil: **não encontrado**.

---

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

## 2. Universitários: apps virais fora do Brasil

### 2.1 Tabela-resumo

| App | O que é | Escala (fonte/data) | Rank iOS 09/10/2026 | Links | Redes (09/10/2026) |
|---|---|---|---|---|---|
| **Fizz** | Rede anônima por campus, verificada por .edu, com marketplace | 700+ campi (AdExchanger, dez/2025); US$41M captados (TechCrunch, set/2025) | fora do top 100 Social EUA | https://apps.apple.com/us/app/fizz/id1548207780 (22.951 avaliações); Android irrelevante (5.000+) | TikTok/YouTube oficiais: não encontrado |
| **Yik Yak / Sidechat** | Feed anônimo por campus, .edu | ~2,2M únicos por semestre, ~15% dos graduandos dos EUA (Northeastern, mai/2026) | **#28 free Social EUA** | https://apps.apple.com/us/app/yik-yak/id1573043816 (51.199 avaliações, 4,0★) | TikTok: não encontrado |
| **Gas** (morto) | Enquetes de elogio anônimo para o ensino médio | 7,4M installs e ~US$7M de gasto em ~6 meses; ~3M MAU em dez/2022; comprado pelo Discord em jan/2023 (TechCrunch: https://techcrunch.com/2023/01/17/discord-acquires-gas-a-compliments-based-social-media-app-for-teens/; Sensor Tower: https://sensortower.com/blog/monday-mobile-memo-2023-01-23) | — | — | — |
| **tbh** (morto) | Precursor do Gas | Vendido ao Facebook por ~US$30M em 2017 e depois fechado | — | — | — |
| **Saturn** | Calendário social do ensino médio | US$44M captados (Forbes, 2021); 80% dos high schools dos EUA suportados; comprado pela Snap (jun/2025) | não aparece no top 100 | https://apps.apple.com/us/app/saturn-calendar-for-school/id1454483188 (67.845 avaliações) | TikTok @joinsaturn 9.925 seguidores, 877,7K curtidas |
| **Marriage Pact** | Questionário de compatibilidade anual por campus | 109 campi, 628.977 participantes, 314.488 matches (Daily Tar Heel, 2025: https://dailytarheel.com/328003/university/university-marriage-pact-summary-2025/) | — | app "Checkmate" id6443729738 | TikTok @marriagepact 6.382 seguidores, 1,2M curtidas |
| **Quizlet** | Flashcards + IA | ~60M MAU e ~US$139M de receita (2025; **estimativa de terceiro, não verificada**) | #9 free / **#4 grossing** Education EUA; #78 grossing Índia | https://apps.apple.com/us/app/quizlet/id546473125 (1.128.473 avaliações) | TikTok @quizlet **619,6K**, 13,6M curtidas |
| **Chegg** | Respostas e tutoria | Assinantes: 3,6M (4T24) → 3,2M (1T25) → **2,6M (2T25, −40% a/a)**; a empresa culpa o Google AI Overviews; demitiu 22% em mai/2025 e ~45% em out/2025 (https://investor.chegg.com/Press-Releases/press-release-details/2025/Chegg-Reports-2025-Second-Quarter-Earnings/default.aspx) | #53 grossing EUA | id385758163 (204.651 avaliações); Play 10,5M, 3,18★ | TikTok @chegg 92,3K; YouTube 168K; IG ~49,8K (Viralist) |
| **Gauth** (ByteDance) | Foto→resposta com IA | 120M installs Android; #1 em downloads de educação nos EUA em nov/2025 | **#3 free / #11 grossing** EUA | id1542571008 (**1.518.796 avaliações**) | TikTok @gauthofficial **876,9K** |
| **Photomath** (Google) | Foto→passo a passo de matemática | 280M installs Android | #56 free / #31 grossing EUA | id919087726 (733.150 avaliações) | TikTok @photomath 360,5K, 7,5M curtidas |
| **Knowunity** (Alemanha) | Resumos de alunos + IA | 20M+ usuários, 380K criadores, Série B de €27M | #27 free EUA | id1484296272 | TikTok @knowunity (EUA) 18,3K; YouTube 2,4K |
| **Studocu** (Holanda) | Resumos e provas antigas; acesso por upload | 60M+ usuários mensais e 50M+ documentos (Biz Journals, nov/2025) | — | id1460235511 (5.816 avaliações); Play 3,56M | TikTok @studocu 18,8K, 524,5K curtidas; YouTube 1,9K |
| **Gizmo** (UK) | Flashcards e quizzes gerados por IA | 5,3M installs Android, 5,5% em 30 dias | #13 free / #26 grossing EUA | id1610516671 | não coletado |
| **Coconote / Turbo AI / StudyFetch** | Gravar aula → notas, flashcards, quiz | ver §0.2 | #47 grossing / #64 free / #90 free EUA | — | — |
| **유니브 AI (Coreia)** | Gravação de aula, resumo de PDF, quiz | não encontrado | **#8 grossing** Education Coreia | — | não encontrado |
| **열품타 YPT (Coreia)** | Timer de estudo + grupos com ranking | 7,6M installs Android | #18 grossing Coreia | — | — |
| **作业帮 Zuoyebang (China)** | Foto→resposta, K-12 | financeiro confiável: não encontrado | **#5 grossing** Education China | id803781859 (**2.886.882 avaliações**) | — |
| **小猿搜题 / 小猿AI (China)** | Foto→resposta e tutor IA (Yuanfudao) | não encontrado | #15 e #6 grossing China | id906995758 (2.252.373 avaliações) | — |
| **大学搜题酱 (China)** | Busca de respostas para universitários | não encontrado | #29 grossing China | — | — |

Social adjacente popular entre estudantes (EUA, free, 09/10/2026): Discord #7, **GroupMe #8** (o grupo de turma padrão), Locket #18, Yik Yak #28, rednote/Xiaohongshu #33 e BeReal #54 em Social; Lemon8 #41 e NGL #63 em Lifestyle.

### 2.2 Por que viralizaram (padrões)

- **Identidade verificada por campus + anonimato** (Fizz, Yik Yak/Sidechat): a verificação por .edu cria pertencimento e o anonimato cria conteúdo. O Yik Yak diz levar 46 s em média entre denúncia e ação, porque moderação é a condição de sobrevivência.
- **Gatilho de ego** (Gas, tbh, NGL): "alguém te elogiou". O paywall "God Mode" vendia pistas de quem mandou o elogio. Mas morre rápido: os convites por usuário caem ~20% a cada ano de idade dos 13 aos 18 (Nikita Bier).
- **Utilidade single-player primeiro, social depois** (Saturn): o calendário funciona sozinho e fica melhor com amigos. D30 em torno de 35% (Lenny's Newsletter: https://www.lennysnewsletter.com/p/lessons-on-building-a-viral-consumer).
- **Evento com data, escassez e revelação** (Marriage Pact): uma janela de inscrição por ano e um "drop" de matches no mesmo dia para o campus inteiro.
- **IA que faz o dever** (Gauth, Question.AI, Solvely, Astra): crescimento via TikTok, com o app virando verbo ("gauth it"). Retém pior que apps de hábito (Gauth ~5,9M WAU contra ~19M do Duolingo com volume de downloads parecido).
- **Conteúdo gerado por alunos com troca** (Studocu, Knowunity): "suba um resumo para desbloquear". A base de conteúdo cresce sozinha.

---

## 3. Concurseiros e vestibulandos fora do Brasil (exam prep)

### 3.1 Índia (UPSC, JEE, NEET, SSC, bancos)

| App | Dados | Fonte |
|---|---|---|
| **Physics Wallah (PW)** | Receita operacional FY25 de **₹2.886,6 crore (+48,7%)**; **4,46M usuários pagantes** (4,13M online); prejuízo caiu de ₹1.131 cr para ₹243 cr; **ACPU online ₹3.682,79**; **ARPU offline ₹40.404,56**. Ranking: #10 free / #5 grossing Education Índia; 58,8M installs Android; YouTube "Physics Wallah - Alakh Pandey" **14,4M inscritos**; IG ~3,6M (Viralist) | Entrackr: https://entrackr.com/fintrackr/physicswallah-spent-rs-1426-cr-on-salaries-in-fy25-offline-arpu-stood-at-rs-40405-10059672; Financial Express; prospecto do IPO (JPMorgan): https://indiaipo.jpmorgan.com/content/dam/jpmorgan/documents/india-private-limited/physicswallah-basis-for-offer-price.pdf |
| **Unacademy** | FY25: receita de ₹701,8 cr (−16,4%), prejuízo de ₹435,4 cr; 109M installs Android (3,16★), só 190K nos últimos 30 dias; #77 free Índia; YouTube Unacademy NEET 2,92M | Entrackr / ET |
| **BYJU'S** | De uma valuation de US$22B à insolvência (CIRP, jul/2024); default de term loan de US$1,2B; disputa de US$533M; 128M installs Android, mas só 18K nos últimos 30 dias | Reuters / Economic Times |
| **Testbook** | 54M installs Android (600K em 30 dias); #11 free Índia; YouTube **4,31M** | AppBrain / YouTube |
| **Adda247** | 33,5M installs Android (340K em 30 dias); #23 free Índia; YouTube Adda247 Banking Exams **9,45M** | AppBrain / YouTube |
| **ZyNerd, Marrow, Cerebellum, DocTutorials, PW MedEd** | Preparação para NEET e NEET PG (medicina): **#1, #3, #12, #18 e #19 grossing** Education Índia | Apple RSS IN, 09/10/2026 |

**Padrão PW:** professor-celebridade no YouTube grátis (Alakh Pandey) → app de baixo preço → centros offline com ARPU 11x maior que o online. É o mesmo funil que Estratégia e Gran usam no Brasil, com o YouTube como canal principal.

### 3.2 China (Gaokao, 公务员/civil service)

| App | Dados | Fonte |
|---|---|---|
| **粉笔 Fenbi** (HKEX 2469; concursos públicos) | Receita de RMB 3,02 bi (2023) → 2,79 bi (2024) → **2,68 bi (2025, −4,1%)**; **~9,12M MAU médio**; 2,38M usuários pagantes de produtos de IA (jan–out/2025); curso escrito de 公务员 a partir de **RMB 7.000** e entrevista a partir de **RMB 8.000**; #20 grossing Education China; App Store: 254.337 avaliações | Relatório anual HKEX: https://www.hkexnews.hk/listedco/listconews/sehk/2026/0331/2026033102787.pdf |
| **华图 Huatu** | Financeiro recente: **não encontrado**; app id940376535 | — |
| **作业帮 / 小猿 (Yuanfudao)** | #5, #6 e #15 grossing China; financeiros: **não encontrado** (empresas fechadas) | Apple RSS CN |
| **考试宝** (banco de questões) | #18 grossing China | Apple RSS CN |

### 3.3 EUA (SAT, LSAT, MCAT, bar, GRE)

| App | Dados | Fonte |
|---|---|---|
| **Khan Academy** | Ano 2024–25: **189,6M usuários registrados**, 104,9M aprendizes ativos no ano; Khanmigo ~2,0M (**+731%**); YouTube **9,48M**; TikTok 79,4K; IG ~309K (Viralist); #60 free EUA; parceria oficial do SAT com o College Board (BigFuture School é #2 free Education EUA) | Relatório anual Khan Academy |
| **UWorld** | Receita de US$77M–105M (**estimativa de terceiro, não verificada**); YouTube 28,5K | — |
| **Magoosh** | Receita de ~US$3,9M (**estimativa de terceiro, não verificada**); YouTube GRE 191K | — |
| **Simuladores DMV** (prova de habilitação) | #16 e #35 grossing Education EUA, mostrando que "simulado de prova oficial" monetiza muito | Apple RSS US |

### 3.4 Coreia e Japão

- **Coreia:** **오르조 (Orzo)**, com provas anteriores (inclusive 공무원/concurso), é **#7 grossing** Education. 유니브 AI é #8, QANDA #12 (82M installs Android), AnkiMobile #13, Quizlet #15 e 열품타/YPT #18. YPT mostra o poder do "estudar junto com ranking". Dados financeiros: **não encontrado**.
- **Japão:** スタディサプリ (Recruit) aparece em #6 e #12 grossing, e um app de questões anteriores de **宅建** (corretor de imóveis, concurso de licença) está em #18 grossing. Detalhes do mercado de 公務員試験: **não encontrado**.

---

## 4. Playbooks campus a campus (o que de fato foi feito)

| Playbook | Táticas concretas | Resultado documentado |
|---|---|---|
| **Facebook (2004)** | Abriu Harvard e depois campus por campus, só com e-mail .edu, entrando onde já existia uma rede rival para "virar o padrão" | (história clássica; números por campus: não encontrado) |
| **Saturn** | Começou como app de uma escola só ("iStaples"); depois **apps white-label por escola**, com a letra da escola no ícone; embaixadores alunos mapeando os horários locais; waitlist | iWeston: mais da metade da escola em **3 horas**; Darien 25% e Ridgefield 15% em 3 h; waitlist de 100K alunos de 10K escolas em 90 dias; #4 geral e #2 Social em ago/2023 (Lenny's) |
| **Fizz** | Fundadores panfletando às 6 h; **1.000 donuts** em Pepperdine; o fundador visitou 220+ campi; hoje os alunos *pedem* o próprio campus; um feed entre campi | +50 campi em um mês sem marketing; DAU da coorte do feed entre campi +50% em 3 semanas (Forbes, abr/2025); recorde de cadastros 12 dias seguidos e DAU 4x no back-to-school de 2025 (TechCrunch). Problema: 25% da base se forma todo ano, então é preciso reconquistar cada calouro |
| **Gas / tbh** | Lançamento **escola por escola**, com geofence e anúncios nas contas de Instagram das escolas; convites com contatos | 7,4M installs e ~US$7M em ~6 meses; vendido em seguida (Discord/Facebook). Morre rápido sem retenção |
| **Marriage Pact** | Janela de inscrição curta e anual; marketing anônimo estilo "astroturfing"; **e-mails de pista** (iniciais, curso do match) que geram mais inscrições; relatório de dados do campus depois do evento | Williams: 1.360 inscritos (~60% dos alunos); Duke 2024–25: 2.776 (43%), e cerca de 2/3 dos graduandos no 1º ano; Brown: 3.158 (~45% na 1ª semana) |
| **Yik Yak** | .edu + anonimato; "Yak Match" de Dia dos Namorados (questionário de 40 perguntas → grupos por afinidade) | ~15% dos graduandos dos EUA por semestre |

---

## 5. Diferenças vs Brasil (poder de preço e ARPU)

| Mercado | Preço/ARPU observável | Fonte |
|---|---|---|
| EUA, Brilliant | US$240–300/ano (~R$1.300–1.650 no câmbio de referência de R$5,5, conversão nossa) | Wikipedia / terceiros |
| EUA, Chegg | Assinantes caindo 40% a/a, mas o ARPU subiu, segundo a empresa | Chegg 2T25 |
| China, Fenbi | Curso de concurso a partir de RMB 7.000–8.000 (~US$1.000+) | Relatório anual |
| Índia, PW | ACPU online de **₹3.682,79** (~US$42) e ARPU offline de **₹40.404,56** (~US$460) | Entrackr / IPO |
| Brasil, concurseiro | 64% gastam **menos de R$126/mês** (relatório BR) | relatório Brasil |

**Leitura:**
1. **EUA e China pagam caro por resultado de prova** (MCAT, bar, 公务员, Gaokao). A Índia paga pouco no online e muito no presencial.
2. O Brasil se parece com a Índia: **volume alto, ticket baixo online e prêmio pela "aprovação"**. O espaço premium no Brasil é a mentoria e o acompanhamento, não o conteúdo.
3. **Nos EUA, a IA "que responde" canibalizou o modelo de resposta paga** (o colapso do Chegg). Vender *respostas* não é defensável; vender *hábito, prática e comunidade* é (Quizlet #4 grossing, Brilliant, Duolingo).
4. **Universitário dos EUA ≠ universitário brasileiro:** lá, a identidade por campus e a moradia no campus dão densidade (Fizz, Yik Yak). No Brasil a densidade está em **turmas e grupos de WhatsApp**, e é aí que Zoen deve plugar.

---

## 6. O que Zoen pode copiar (prioridade)

1. **Lição interativa estilo Brilliant como mini-app dentro do chat/Space.** O agente de estudo gera um desafio manipulável e um eval valida antes de enviar, com retry, XP e streak. Isso diferencia o produto de "mais um chat com IA".
2. **Agentes de estudo que não entregam a resposta** (Koji, Khanmigo). O Gauth mostra que "dar a resposta" gera downloads mas não retém; o Chegg mostra que isso não se defende.
3. **Lançamento por turma ou cursinho, não por país** (Saturn, Fizz). Um Space white-label por turma ou cursinho, com embaixador aluno; meta de 25–50% da turma em 3 horas.
4. **Evento sazonal com data e revelação** (Marriage Pact, Yak Match). Ex.: "Pacto de estudo" na véspera do edital ou do ENEM, que casa parceiros de estudo por banca, cargo e horário, com revelação no mesmo dia.
5. **Estudar junto com ranking** (YPT, Forest): sala de foco do Space com timer e placar semanal.
6. **Gravar aula → notas, flashcards e quiz** (Coconote, Turbo, StudyFetch, 유니브 AI): é a categoria que mais cresce em % e cabe como agente dentro do Space.
7. **Paywall por limite diário + plano de grupo** (Brilliant Family até 6 → "plano turma").
8. **Distribuição: patrocinar criadores** (Brilliant/YouTube, PW/Alakh Pandey) com cupom rastreável, em vez de mídia paga.

---

## 7. Metodologia e fontes

- **Rankings:** Apple RSS oficial, Education (6017), Social (6005) e Lifestyle (6012); países us, in, cn, kr, jp; capturados em 09/10/2026 00:47 BRT (BR às 00:34 BRT).
- **Avaliações iOS:** iTunes Search/Lookup API (EUA, salvo indicação).
- **Google Play:** google-play-scraper (`realInstalls` é global).
- **Installs nos últimos 30 dias:** páginas públicas do AppBrain, lidas em 09/10/2026, só Android e globais.
- **Redes:** perfis públicos do TikTok (seguidores e curtidas), busca de canais do YouTube (inscritos) e Viralist para o Instagram (aproximado, já que o Instagram exige login).
- **Bloqueados / não usados:** Peekly (checkpoint anti-bot) e painéis pagos (Sensor Tower, Appfigures, AppMagic, data.ai, Apptopia).
- **Não encontrado:** séries mensais de downloads mai–out/2026 por país; financeiros de Zuoyebang, Yuanfudao e Huatu; dados da Coreia e do Japão sobre 공무원/公務員; Instagram oficial de Brilliant, Fizz, Quizlet e Marriage Pact; apps de campus específicos da Índia.
- Fontes principais: TechCrunch (Fizz, Gas), Forbes (Fizz, Saturn), AdExchanger (Fizz), Northeastern Global News (Yik Yak), Lenny's Newsletter (Saturn), Daily Tar Heel (Marriage Pact), Chegg IR/SEC, Entrackr / Financial Express / prospecto do IPO (PW), Reuters/ET (BYJU'S), HKEX (Fenbi), Khan Academy Annual Report, Sensor Tower blog (Gauth), SponsorRadar, ThoughtLeaders, Influencer Advisory e Flywheel (patrocínios da Brilliant), ustwo, Rive, Paige Ormiston e o blog da Brilliant (design), Wikipedia (Brilliant), Brilliant Help Center.
