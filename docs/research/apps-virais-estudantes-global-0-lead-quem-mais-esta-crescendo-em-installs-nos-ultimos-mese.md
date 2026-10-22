[Research index](apps-virais-estudantes-global.md).

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
