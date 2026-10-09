# Métricas do Zoen

Duas métricas dizem se o Zoen está dando certo:

1. **Usuários ativos no mês (MAU).**
2. **Mensagens enviadas por usuário ativo.**

Todo o resto existe para explicar por que essas duas sobem ou descem.

Como medimos (detalhes técnicos na [ADR 0043](../adr/0043-product-metrics.md)):
- **Nunca lemos conteúdo.** O servidor só conta que uma mensagem criptografada passou, não o que ela diz.
- **Sem nomes nem ids.** Cada conta vira um pseudônimo que só abre com uma chave secreta.
- **35 dias no máximo.** As linhas por conta são apagadas depois disso. Ficam só os totais do dia.
- **Pessoas, agentes e QA separados.** O número principal conta só pessoas. Agentes aparecem à parte, e as contas de QA (por prefixo do @, como `qa_`) só entram na visão "todo mundo".
- **Dias em UTC.**

## Onde ver

- **Ao vivo:** `https://relay.tryzoen.com/admin`. Cole o token de admin, que fica no Fly como `ZOEN_ADMIN_TOKEN`. O JSON está em `GET /admin/metrics` com `Authorization: Bearer <token>`, e `?population=everyone` inclui as contas de QA.
- **Histórico:** painel "Zoen · métricas" no PostHog. O relay manda um evento `zoen_daily_metrics` por dia fechado, só com números, sem nenhuma pessoa.

## As duas principais

| Métrica | Definição | Por que importa |
|---|---|---|
| **MAU** | Contas de pessoas distintas que, nos últimos 30 dias (janela móvel), enviaram ≥ 1 mensagem **ou** abriram o app e sincronizaram. | É o tamanho real da rede. Mensageiro vale pelo número de pessoas que você encontra lá. |
| **MAU de remetentes** (variante estrita) | O mesmo, mas só quem enviou ≥ 1 mensagem. | Separa quem só olha de quem conversa. A diferença entre os dois é o público para ativar. |
| **Mensagens por usuário ativo** (dia, semana, mês) | Mensagens enviadas no período ÷ remetentes distintos no período. | Mede se o Zoen virou hábito. Rede grande com pouca conversa morre. |

**Mensagem** é um envio de conteúdo: um `MessagePosted` legível ou um envelope criptografado do tipo "aplicação" (MLS). Entrada no grupo, troca de chaves, checkpoint e criação de Space não contam. Edições e itens dentro do chat também viajam como "aplicação", e o servidor não consegue nem deve diferenciá-los.

## Engajamento

| Métrica | Definição | Por que importa |
|---|---|---|
| DAU / WAU | Ativos no dia / nos últimos 7 dias. | Mostra o ritmo e acusa quedas antes do MAU. |
| **DAU/MAU** (stickiness) | DAU ÷ MAU. | Quantos dias por mês a pessoa volta. Mensageiros bons passam de 50%. |
| Sessões por DAU | Logins autenticados no dia ÷ DAU. | Quantas vezes por dia o app é aberto. |
| Spaces criados | Por tipo (direto, grupo, comunidade, pessoal), por dia. | Novos lugares de conversa: é o motor da rede. |
| Spaces ativos e membros médios | Spaces com ≥ 1 mensagem no dia e sua média de membros. | Diz se as conversas acontecem em dupla ou em grupo. |
| Uso de agentes | Mensagens de agentes ÷ MAU, e agentes ativos em 30 dias. | Mede se os agentes ajudam ou só ocupam espaço. |
| Aprovações sim / não | Pedidos de agentes aprovados e negados. Só em Spaces legíveis: nos criptografados, a resposta é segredo. | Muito "não" indica agente mal calibrado. |
| Mídias enviadas | Arquivos criptografados guardados por dia. | Foto e áudio são sinal de conversa de verdade. |

## Crescimento e ativação

| Métrica | Definição | Por que importa |
|---|---|---|
| **Cadastros por dia** | Contas novas de pessoas, sem QA. | É a entrada do funil. |
| **Ativação em 24 h** | Dos cadastrados num dia, quantos % enviaram a 1ª mensagem em até 24 h. | Quem não conversa no primeiro dia raramente volta. É o "momento mágico". |
| **Ativação social na 1ª semana** | Dos cadastrados num dia, quantos % escreveram para ≥ 2 pessoas diferentes nos 7 primeiros dias. | Com duas pessoas, o app passa a ter rede para aquela pessoa. O servidor guarda só um "rascunho" de 64 bits de para quem ela escreveu, que diz "duas ou mais" e nada além. |
| Convites criados e aceitos | Por dia. | Mede o boca a boca em si. |
| **K viral** | Convites aceitos ÷ cadastros, em 30 dias. | Acima de 1, cada pessoa nova traz mais de uma. Abaixo, o crescimento depende de aquisição. |
| Tempo até a 1ª mensagem do convidado | Mediana entre o cadastro e a 1ª mensagem de quem entrou por convite. | Convite bom leva direto à conversa. Se demora, o caminho do convite está quebrado. |

## Retenção

| Métrica | Definição | Por que importa |
|---|---|---|
| **D1, D7, D30** | Dos cadastrados num dia, quantos % estavam ativos 1, 7 e 30 dias depois, somando os dias dos últimos 30. | É a métrica que mata ou salva o produto. |
| **Curva por coorte semanal** | Para quem se cadastrou há 0 a 4 semanas, quantos % estavam ativos em cada semana seguinte. | A curva precisa **achatar**. Se ela vai a zero, não há produto, e crescer só enche um balde furado. Achatada em 30–40% na 4ª semana, há algo para escalar. |

## Confiabilidade (guardrails)

| Métrica | Definição | Por que importa |
|---|---|---|
| Latência de envio p50 / p95 | Do envelope chegar no relay até ser aceito e ordenado, medida no servidor e agrupada em faixas. | Mensagem lenta parece quebrada. |
| Tempo até o 1º sync p50 / p95 | Do login até o fim da primeira sincronização da sessão. | É o que a pessoa espera ao abrir o app. |
| Envios recusados | % de envelopes recusados (limite de taxa, assinatura, permissão). | Se sobe, algo quebrou no app ou no servidor. |
| Sessões sem crash | % de sessões do app sem fechar sozinho. Ainda não medida: depende de relatório do app, opcional e agregado (ADR 0044). | Crash derruba retenção mais rápido que qualquer recurso. |

## O que fica de fora de propósito

- Conteúdo, texto, nomes de chat, anexos e com quem a pessoa conversa, além do "≥ 2 pessoas".
- Qualquer evento por pessoa no PostHog ou em outro fornecedor.
- Fingerprinting, IP ou id de anúncio.

## Origem do usuário e onboarding

Detalhes na [ADR 0044](../adr/0044-experiments-remote-config-onboarding.md).

| Métrica | Definição | Por que importa |
|---|---|---|
| **Origem** | Primeiro link que abriu o app: `friend` (link de um amigo), `space` (link de Space), `campaign` (anúncio ou criador, com o id da campanha) ou `organic`. Só o tipo e o id da campanha vão ao servidor. Quem convidou e o código ficam no aparelho. | Diz de onde vem quem fica, não só quem chega. |
| **Funil por origem** | Cadastros, % que enviou a 1ª mensagem, ativação em 24 h, D1 e D7 por origem e campanha. | Mostra onde investir. Convite de amigo costuma reter muito mais que anúncio. |
| **Funil por braço do onboarding** | O mesmo, separado pelo braço do experimento de onboarding. | Mostra se o fluxo novo ativa e retém mais. |

Não usamos fingerprinting. Quando a pessoa clicou num convite antes de instalar, a primeira tela oferece "Colar" do sistema, e é ela quem escolhe trazer o link. Anúncios pagos, quando existirem, são medidos pelo AdAttributionKit/SKAdNetwork da Apple, agregado por campanha.

## Experimentos (A/B)

- **Como funciona:** a config remota (`/v1/config`, trocada em `PUT /admin/config`) define flags, braços e pesos. O aparelho sorteia o braço sozinho, sempre igual para a mesma instalação. 5% das instalações ficam num **holdout** sem nenhum experimento.
- **Exposição:** só conta quem **viu** o braço, como a tela renderizada ou o texto lido.
- **Por braço:** unidades, mensagens por dia exposto (com CUPED, que desconta o quanto a pessoa já mandava antes), D1 e sessões sem crash.
- **Contra o controle:** diferença, p-valor e **p-valor sempre válido** (mSPRT). Dá para olhar todo dia sem se enganar com sorte.
- **Guardrails:** retenção D1, sessões sem crash e latência de envio. A latência é medida no servidor para todos, então protege versões, não braços. Se um braço piora um guardrail com significância, a decisão vira "stop".
- **Limite:** os dados por instalação duram 35 dias. Cada experimento é lido em até 4 semanas, e o holdout mostra o efeito somado de longo prazo.

Experimento no ar: `onboarding_friend_v1`.
- **`control`:** quem chega por link de amigo vê o onboarding padrão de 8 telas.
- **`direct`:** quem chega por link de amigo vê 3 telas ("@ana te chamou pro Zoen") e cai direto no chat com quem convidou.
- **Métrica principal:** mensagens por dia.
- **Guardrails:** D1 e crash.
