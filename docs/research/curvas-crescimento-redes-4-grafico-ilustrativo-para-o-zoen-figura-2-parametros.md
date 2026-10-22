[Research index](curvas-crescimento-redes.md).

## 4. Gráfico ilustrativo para o Zoen (Figura 2): parâmetros

**Não são dados reais.** É um modelo para ilustrar a forma esperada. Script que gera as duas figuras: [`scripts/curvas_charts.py`](scripts/curvas_charts.py) (matplotlib).

- **Base:** curva em S logística com teto de 30 mi de MAU, inflexão no ano 3 e início de ~40 mil usuários (equivalente a alguns campi). O teto é conservador diante de 150 mi de identidades em redes sociais no Brasil ([DataReportal](https://datareportal.com/reports/digital-2026-brazil)).
- **Pico de lançamento → vale de −55%:** calibrado pelo Threads (−70% de DAU) e pelo Clubhouse (−72% de downloads), suavizado porque o Zoen nasce em grupos fechados.
- **Degraus de semestre** (fev/mar e ago): o calendário acadêmico brasileiro. Cada degrau retém 70% do ganho, porque calouros entram em turmas que já estão no Zoen.
- **Picos de concurso/ENEM:** grupos de estudo explodem antes das provas e retêm ~40%. Referência de tamanho: o CNU de 2024 teve 2,14 mi de inscritos ([G1](https://g1.globo.com/trabalho-e-carreira/concursos/noticia/2024/02/23/enem-dos-concursos-governo-detalha-concorrencia-e-perfil-de-inscritos.ghtml)).
- **Collab com criador:** um pico que retém ~45%.
- **Choque externo** (apagão ou bloqueio do líder): retém 30%, calibrado pelo Bluesky (33%) e pelo Mastodon (~48%).
- **Sazonalidade:** −7% em janeiro e −4% em julho (férias), mais ruído semanal de ~3,5%.
- **Fases:** 1) rede atômica, com campi e grupos de concurso; 2) nichos vizinhos, como gamers e criadores; 3) abertura geral, só depois de a retenção dos nichos estar estável.

---
