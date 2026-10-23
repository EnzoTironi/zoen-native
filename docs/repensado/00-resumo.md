[Índice](../repensado.md) · [Próximo](decisoes.md)

## 0. Resumo

- **Produto:** **Zoen**. É um app novo e nativo da Apple. O motor é o núcleo em Rust que nasce do signal-rust. **Zoen** é o agente padrão que vem junto (e pode ser renomeado).
- **Tese:** a conversa é onde o trabalho e a vida acontecem, e agentes são membros dela com a mesma dignidade das pessoas. Tudo que nasce ali é seu: versionado, compartilhável e portátil.
- **Momento mágico:** em menos de 60 segundos, sem cadastro, você fala uma frase, o seu agente transforma isso em algo concreto dentro da conversa, e você chama alguém com um link (App Clip no iPhone, navegador no resto).
- **5 primitivos:** Identidade, Espaço, Membro, Item e Concessão, todos registrados num **log de eventos assinado** por Espaço. **133 telas viram 20.**
- **Cliente:** 100% Swift/SwiftUI para iOS 26+ e macOS 26+, com Liquid Glass de verdade. O núcleo Rust fica por baixo via UniFFI. Web só como visualizador de convite. Android em 2027, com Compose sobre o mesmo núcleo.
- **Protocolos em camadas:**
  - **Espaços privados:** MLS (OpenMLS).
  - **Escala:** protocolo próprio, o "Zoen Sync" (eventos assinados, sequenciados por Espaço).
  - **Camada pública, depois:** AT Protocol (Bluesky). Nostr/Marmot fica como saída de soberania.
- **Escala:** relay em Rust, ordem por Espaço com dono definido por hash consistente (como os Channel Servers do Slack), NATS para entrega por interesse, Postgres particionado e, depois, FoundationDB. São Paulo primeiro.
- **Segurança:** MLS com caminho para pós-quântico, key transparency (AKD), aparelhos com chave própria, backup cifrado por passkey, agentes declarados como leitores e message franking para denúncias.
- **Preço:** Grátis · **Plus R$ 29/mês** · **Max R$ 79/mês** · **Equipes R$ 39/pessoa**. Criadores pagam **8%** sobre a receita. **Nunca anúncios.**
- **Primeiras 12 semanas:** demo da semana 4 = momento mágico no iPhone e no Mac; demo da semana 8 = convite, E2EE, pedidos de agente e Live Activity; semana 12 = TestFlight com 300 pessoas.

---

