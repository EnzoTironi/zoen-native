[Índice](../repensado.md) · [Anterior](00-resumo.md) · [Próximo](01-tese.md)

## Decisões (registro no estilo ADR)

O Enzo delegou todas as decisões. Elas estão tomadas abaixo, cada uma com o porquê e com **o que nos faria rever**.

| # | Decisão | Por quê (uma linha) | Revemos se… |
|---|---|---|---|
| D1 | **Público do v1:** você + seu agente + grupos pequenos (família, amigos, times de até 50), em PT/EN/ES | O momento mágico funciona sozinho e cria rede pelo convite; equipes vêm em seguida | Na semana 8, mais de 60% do uso for em grupos de trabalho → antecipar Equipes |
| D2 | **Plataforma:** iOS 26+ e macOS 26+ nativos (SwiftUI). Web é só visualizador de convite. Android em 2027 (Compose + núcleo Rust) | Liquid Glass e integrações de sistema (NSE, CallKit, App Intents) são o nosso fosso de qualidade | Mais de 40% dos convites forem abertos em Android e a conversão no web cair abaixo de 15% → antecipar Android |
| D3 | **Soberania invisível:** passkey + chaves por baixo, sem "chave privada" na UI. Exportar/assinador ficam em Avançado | Soberania é garantia, não onboarding | Os usuários avançados forem um segmento relevante → expor um "modo soberano" |
| D4 | **Agentes em E2EE:** Espaços privados são E2EE (MLS), e o agente é um **membro-leitor declarado**. O runtime decifra em memória na nossa nuvem, com enclaves em 2027 e a opção "só agentes locais" | Honestidade sobre quem lê é o diferencial frente a Ando e Slack, que leem tudo | A qualidade dos agentes cair por falta de contexto, ou enclaves com GPU ficarem baratos antes |
| D5 | **Preço:** Grátis (R$ 3/mês de IA inclusos, 5 GB) · **Plus R$ 29/mês ou R$ 290/ano** (R$ 15 de IA, 100 GB, Ao vivo com agentes) · **Max R$ 79/mês** (R$ 50 de IA) · **Equipes R$ 39/pessoa** (R$ 15 de IA por pessoa, num pool) · recargas de R$ 20/R$ 50 · criadores pagam **8%** · **zero anúncios** | Cobrar por pessoa com orçamento de IA embutido é previsível (a filosofia do Ando) e protege a margem | A margem bruta ficar abaixo de 60%, ou a conversão para Plus ficar abaixo de 3% em 90 dias |
| D6 | **Casa e nome:** produto novo, **Zoen**. O signal-rust vira o motor. **Zoen** vira o agente padrão (renomeável) e é a ponte com os usuários atuais do tryzoen | O nome Zoen é humano, curto, global e combina com o logo de anéis; o Zoen ganha o papel que já sabe fazer | A marca ou o domínio estiverem indisponíveis (INPI/USPTO/.app) *(a verificar)* |
| D7 | **Protocolos:** MLS (OpenMLS, MIT) para privados · **Zoen Sync** próprio para escala · **AT Protocol** para a camada pública (fase 4) · Nostr/Marmot como ponte opcional | Cada camada usa o protocolo que é o melhor naquilo (ver §Protocolos) | O Marmot amadurecer (spec estável, auditoria) → avaliar grupos soberanos via Nostr |
| D8 | **Servidor em Rust** (tokio/axum), com os mesmos crates do cliente | Segurança de memória ao lidar com entrada hostil, latência sem GC e tipos compartilhados | Contratação em Rust virar gargalo (improvável no Brasil, que tem comunidade Rust ativa) |
| D9 | **Armazenamento:** Postgres particionado por Espaço → **FoundationDB** quando passar de ~30 mil escritas/s sustentadas ou ~10 TB quentes. Segmentos frios vão para object storage | Postgres é simples hoje; FDB dá ordem transacional por Espaço (versionstamps) sem um serviço sequenciador | A operação do FDB pesar demais → ScyllaDB (o caminho do Discord) |
| D10 | **Comunidades em 3 níveis:** 🔒 Privado E2EE (até 1.000 membros) · 🛡️ Fechada (legível pelo servidor, para moderar) · 🌐 Pública | O MLS não escala com conforto para 100 mil membros, e moderar exige ler | Split/Partial Commits do MLS maturarem → subir o limite do E2EE |
| D11 | **Feed:** cronológico, de quem você segue e das suas comunidades. "Para você" só depois, com sinais explicados e feeds escolhíveis | Confiança antes de engajamento; nada de anúncios | Retenção do Explorar for baixa → testar ranking transparente |
| D12 | **Pagamentos:** Brasil = IAP (obrigatório oferecer) **+ Pix/cartão via processador alternativo** dentro do app (regras do iOS 26.5 no Brasil). EUA = IAP + link para checkout web. Resto = IAP | Pelas regras do Brasil, o processador alternativo custa 10% (pequenas empresas) contra 10% + 5% da IAP | As regras da Apple mudarem |
| D13 | **Nomes próprios:** "Ao vivo" no lugar de Jam e "Rascunhos" no lugar de Studio | Os dois nomes já são recursos do Ando | — |
| D14 | **Código proprietário:** todo o código, inclusive a spec do protocolo e o núcleo cliente, é de todos os direitos reservados (ver `LICENSE`); o repositório é público só para leitura. Abrir a spec é uma decisão para depois | Transparência para quem lê o código sem abrir mão de nenhum direito | Leitura pública facilita copiar ideias (o valor está em UX, agentes e rede) |
| D15 | **Idade mínima de 18 anos no v1**, com política de 13+ só na fase 3, quando a moderação existir | O ECA Digital (Lei 15.211/2025) exige proteção robusta de menores | A pilha de moderação e verificação de idade ficar pronta |
| D16 | **Equipe inicial:** 4 pessoas (2 Swift/Apple, 1 Rust de núcleo/servidor, 1 produto/design) + Enzo | O WhatsApp de 2014 tinha ~25 engenheiros para 465M de usuários | Velocidade abaixo do plano das 12 semanas |

---

