# Zoen: caminho até o produto completo

Entregar um lugar onde pessoas e agentes conversam e trabalham juntos, com privacidade, continuidade e resultados recuperáveis. **Agora: agentes e conversação no iOS**, seguindo as interações selecionadas do Telegram iOS e a confiabilidade do Signal. Apps, Páginas, ArtCraft, editores e expansão do host/SDK estão pausados; seus dados, código e capacidades finais permanecem preservados.

Abrir a [visão navegável](index.html), o [manifesto desta revisão](manifest.json) ou a [meta completa](../dev/zoen-complete-product-goal.md). O inventário e catálogo desta revisão permanecem em **26/10/2026**, fonte `477ac2f3`; o avanço de execução abaixo é atualizado separadamente. Os links dos itens abrem os critérios e restantes; nenhum checkbox deste documento muda o produto.

**Enzo responde pela direção de produto. Codex responde pelos requisitos técnicos derivados**, pela implementação e por ligar cada requisito ao problema e ao issue correspondente; cada entrega mantém seu responsável nominal no issue. O [issue #170](https://github.com/EnzoTironi/zoen-native/issues/170) governa o trabalho. O [acceptance.json](../../apps/zoen/acceptance.json) e suas partes governam o aceite. Este roteiro e seu manifesto são uma visão datada dessas fontes.

## O que já avançou

Os PRs [#635](https://github.com/EnzoTironi/zoen-native/pull/635), [#636](https://github.com/EnzoTironi/zoen-native/pull/636) e [#637](https://github.com/EnzoTironi/zoen-native/pull/637) foram integrados após #632/#633. Os fechamentos #217/#615/#622/#440 cobrem paginação, reconexão/aprovação, ordem sob rate limit e AutoSky original v544, respectivamente; os limites constam no registro canônico, sem aceite de jornada.

O App732 tem fonte `9f724c1e` e reutiliza Core/UniFFI genuíno de `c4b8f7ce` (F), que inclui #632, após igualdade dos inputs nativos. CLI `4ff0ca77` ficou preparado; a QA usou G2 `f0a01c1f` nas leituras canônicas/bytes. Relay: `4057a0af`/schema40. Build e assinatura passaram. A QA732 confirmou larguras nativas, empilhamento em fonte ampliada e um Cancelar preservando ambos os rascunhos, sem Enviar; o tamanho do texto foi restaurado. A [referência canônica do artefato](../../apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json) separa essas evidências; esse recorte não demonstra encaminhamento entregue, aceite físico ou jornada completa.

Os **506 itens Telegram revisados e as 77 famílias de telas** medem cobertura de leitura de fonte e inventário. Não demonstram paridade. Fonte inspecionada, teste automatizado, build, execução no Simulator e execução em aparelho físico são evidências distintas; nenhuma substitui a próxima. As três flags globais continuam `false`, e nenhuma das 30 jornadas completas está aceita.

Provas anteriores, com recibos na referência canônica: F724 encaminhou texto/mídia uma vez cada, preservando rascunhos e a imagem de 1.268.382 bytes (SHA256 `5864d328…`); 727 passou busca e destino no primeiro toque. Não são retestes732; CLI4ff foi apenas preparado/help-only.

## Ordem de entrega

A [revisão visual histórica](visual-review.json) reúne 38 capturas (37 hashes distintos) do Simulator, no App718/Core707. Busca global coberta pelo teclado, texto sob o cabeçalho do Agent e preferências de grupo muito compactas foram achados nessa revisão histórica; correções posteriores exigem sua própria evidência, sem relabelar essas imagens. Isso não qualifica todas as 77 famílias. As imagens ficam locais, fora do Git; o painel as mostra neste checkout.

1. Fechar conversa e agentes: enviar e receber entre pessoas independentes; teclado, busca, ações e mídia preservam conteúdo/destino; o agente conclui pedido → aprovação → efeito → resultado → retomada. Backend e integridade acompanham cada lote. [#172][i172] · [#173][i173] · [#175][i175] · [#176][i176].
2. Completar identidade/recuperação e ciclo de vida físico: provedores reais, vínculo/revogação, push, chamadas, acessibilidade e distribuição. Dependem da mesma autoridade e das garantias de entrega. [#177][i177] · [#178][i178] · [#181][i181] · [#183][i183].
3. Após o recorte ativo, retomar Apps e criação e concluir suas jornadas; a pausa muda a sequência, sem retirar requisitos do lançamento iOS. [#179][i179] · [#180][i180].
4. Aceitar o lançamento iOS com backend público compatível, provedores reais, release físico e perfil iOS/backend congelado antes da medição. Depois qualificar Web e Android; em seguida macOS, Windows e Linux e o aceite final. [#182][i182] · [#184][i184].

## As 30 jornadas

Cada caixa só será marcada quando **a jornada inteira** passar no escopo e nos artefatos exigidos. “Prova parcial” conserva avanços, sem fechar o item. A fase indica a ordem atual; segurança, backend e recuperação atravessam todas elas.

### Agora — conversa e agentes no iOS

- [ ] **04. [Mensagens criptografadas iOS entre clientes canônicos](journeys/journeys-01-10.json)** — Prova parcial. Qualificar remetente e destinatário independentes no artefato atual; Verificar conteúdo e contexto contra backend compatível. [#172][i172] · [#176][i176].
- [ ] **05. [Enviar offline e retomar sem duplicação após reabrir](journeys/journeys-01-10.json)** — Prova parcial. Resolver ordenação sob rate limit e reabertura; Provar perda de ACK, reconexão e reinício sem perda ou duplicação. [#176][i176] · [#622][i622].
- [ ] **06. [Responder, citar, navegar, criar thread, editar, excluir, reagir e encaminhar](journeys/journeys-01-10.json)** — Prova parcial. Completar matriz de ações e gestos com conteúdo real; Verificar permissões, falhas, acessibilidade e recepção independente. [#172][i172].
- [ ] **07. [Ler histórico longo preservando posição, teclado e IME](journeys/journeys-01-10.json)** — Prova parcial. Qualificar posição e histórico longo com teclado/IME; Validar detent do carrossel após fechar teclado e callbacks de escopo. [#172][i172] · [#518][i518].
- [ ] **08. [Buscar, filtrar e controlar não lidas e recibos](journeys/journeys-01-10.json)** — Prova parcial. Qualificar busca com resultados e filtros; Validar perda de associação, preferências e recibos reais. [#172][i172] · [#529][i529] · [#199][i199].
- [ ] **09. [Usar HUD, navegar resultados e assistente temporário](journeys/journeys-01-10.json)** — Prova parcial. Completar resultados e retorno ao contexto da conversa; Qualificar agente temporário sem resultados e aposentadoria do escopo. [#172][i172] · [#175][i175].
- [ ] **12. [Entregar fotos, câmera, documentos, voz e vídeo criptografados](journeys/journeys-11-20.json)** — Prova parcial. Qualificar revisão, Cancelar, recuperação e envio explícito de cada mídia; Verificar bytes no destinatário e ciclo de vida físico/permissões. [#173][i173] · [#446][i446] · [#489][i489] · [#494][i494].
- [ ] **13. [Gerir agentes, permissões, orçamentos e aprovações](journeys/journeys-11-20.json)** — Prova parcial. Qualificar retomada durável e provedores reais com resultado atribuído; Concluir controles, orçamento, aprovação e efeitos sem repetição; Não confundir Pausar/Retomar com cancelamento de turno ou recibo durável. [#175][i175] · [#615][i615] · [#419][i419].
- [ ] **16. [Gerir comunidades, convites e permissões](journeys/journeys-11-20.json)** — Prova parcial. Concluir telas, associação, administradores e convites; Provar revogação consistente no front e backend. [#181][i181].
- [ ] **19. [Aplicar céu local/manual e temas acessíveis](journeys/journeys-11-20.json)** — Prova parcial. Qualificar Auto, fases manuais e retorno ao preset salvo; Validar contraste, movimento reduzido e comportamento nativo. [#181][i181] · [#440][i440].
- [ ] **20. [Usar iPhone com VoiceOver, Dynamic Type e teclado](journeys/journeys-11-20.json)** — Pendente. Qualificar navegação, foco, leitura e tamanhos em todo recorte iOS; Validar em iPhone físico com os mesmos artefatos. [#181][i181] · [#183][i183].
- [ ] **23. [Usar interface e formatação consistentes em português e inglês](journeys/journeys-21-30.json)** — Pendente. Revisar todas as telas e erros EN/PT-BR; Qualificar formatação de provedores, acessibilidade e contexto. [#181][i181].
- [ ] **24. [Detectar log danificado e apresentar integridade e auditoria](journeys/journeys-21-30.json)** — Prova parcial. Qualificar avisos e auditoria na UI atual; Cobrir corrupção, ciphertext e sucessores sem sucesso inventado. [#176][i176] · [#182][i182].

### Depois no iOS — confiança, sistema e release

- [ ] **01. [Entrar com Apple/Google, editar perfil e recuperar em novo dispositivo](journeys/journeys-01-10.json)** — Pendente. Qualificar provedores reais e provisionamento em uma ação; Recuperar histórico autorizado em dispositivo independente. [#177][i177].
- [ ] **02. [Criar conta, guardar chaves e reabrir a frio](journeys/journeys-01-10.json)** — Prova parcial. Qualificar armazenamento seguro e reabertura no artefato final; Cobrir falhas e continuidade das chaves. [#177][i177].
- [ ] **03. [Vincular e revogar dispositivos com histórico recuperável](journeys/journeys-01-10.json)** — Prova parcial. Concluir gestão nativa e revogação vinculada ao dono; Provar recuperação independente sem liberar histórico indevido. [#177][i177] · [#360][i360] · [#365][i365].
- [ ] **17. [Receber push privado e abrir destinos autorizados](journeys/journeys-11-20.json)** — Pendente. Qualificar APNs físico, refresh, revogação e deep links; Cobrir app bloqueado/encerrado e falha de entrega sem perder eventos. [#178][i178] · [#199][i199].
- [ ] **18. [Usar widgets, Live Activities e App Intents com escopo](journeys/journeys-11-20.json)** — Pendente. Inspecionar e qualificar contratos existentes por extensão; Verificar aposentadoria após troca de conta e ações autorizadas. [#178][i178].
- [ ] **21. [Instalar release sem Metro e reproduzir no CI](journeys/journeys-21-30.json)** — Prova parcial. Produzir release físico assinado com fonte e Core correspondentes; Qualificar instalação, atualização, migração e rollback reproduzíveis. [#183][i183] · [#200][i200].
- [ ] **22. [Sair e apagar chaves, dados e cache locais](journeys/journeys-21-30.json)** — Pendente. Qualificar encerramento de donos e limpeza local completa; Preservar dados duráveis e bloquear trabalho tardio de sessão aposentada. [#177][i177].
- [ ] **25. [Fazer chamadas privadas de voz e vídeo sob falhas e ciclo de vida](journeys/journeys-21-30.json)** — Pendente. Concluir admissão TURN, criptografia e chamadas reais; Qualificar câmera/mic, interrupções, reconexão, background e cold delivery físicos. [#178][i178] · [#266][i266] · [#288][i288] · [#296][i296] · [#312][i312] · [#305][i305].

### Pausadas — Apps e criação, escopo preservado

- [ ] **10. [Editar Páginas com colaboração, recuperação e histórico](journeys/journeys-01-10.json)** — Prova parcial. Retomar somente após conclusão do recorte ativo; Concluir blocos, CRDT, undo, importação/exportação e restauração colaborativa. [#179][i179] · [#249][i249] · [#583][i583].
- [ ] **11. [Editar Planos com valores exatos, conflito e aprovação](journeys/journeys-11-20.json)** — Pendente. Retomar após o recorte ativo; Qualificar dinheiro exato, conflito, aprovação, histórico e restauração. [#179][i179].
- [ ] **14. [Executar Apps nativos e Web com capacidades isoladas](journeys/journeys-11-20.json)** — Prova parcial. Retomar host/SDK e integração apenas após o recorte ativo; Qualificar pacote externo fixado, isolamento, permissões e recuperação. [#180][i180].
- [ ] **15. [Organizar cards na Home e trilhos de conversa por conta/dispositivo](journeys/journeys-11-20.json)** — Prova parcial. Preservar pausa da expansão de Apps; trilho conversacional permanece ativo; Qualificar Pin/menu/escopo e persistência da jornada completa. [#180][i180] · [#172][i172].

### Desktop — após Web e Android

- [ ] **26. [Instalar desktop com assinatura, atualizações e integração nativa](journeys/journeys-21-30.json)** — Pendente. Após Web/Android, qualificar macOS, Windows e Linux; Concluir instalação, assinatura, atualização e integrações por sistema. [#184][i184].

### Produção — acompanha os lotes e fecha a entrega

- [ ] **27. [Interoperar com backend público completo de produção](journeys/journeys-21-30.json)** — Pendente. Implantar backend Rust compatível e provedores reais; Qualificar protocolo completo com artefatos finais sem extrapolar provas locais. [#182][i182].
- [ ] **28. [Recuperar múltiplos dispositivos offline sob falhas declaradas](journeys/journeys-21-30.json)** — Prova parcial. Qualificar matriz de falhas e dispositivos independentes; Provar zero perda confirmada, zero efeitos duplicados e recuperação autorizada. [#176][i176] · [#177][i177] · [#182][i182].
- [ ] **29. [Comprovar carga, latência, disponibilidade, recuperação e custo](journeys/journeys-21-30.json)** — Pendente. Congelar inventário e hashes antes de duas execuções completas; Medir metas do envelope por plataforma/local e observar disponibilidade por 30 dias. [#182][i182] · [#184][i184].
- [ ] **30. [Operar observabilidade, proteção contra abuso, incidentes e rollback](journeys/journeys-21-30.json)** — Pendente. Qualificar alertas privados, quotas, pressão e resposta a incidentes; Exercitar backup/restauração, atualização, migração e rollback com durabilidade. [#182][i182].

## As dez capacidades de criação

**Todas pausadas por Enzo**, aguardando a conclusão e o aceite nativo do recorte ativo. Ao retomar, pessoas e agentes usam o mesmo artefato, permissões e histórico. A integração externa dos editores não transfere a autoridade durável para o App. [#179][i179] · [#180][i180].

- [ ] **C01. [Organizar espaços e itens filhos/recentes](journeys/creation-capabilities.json)**.
- [ ] **C02. [Editar Páginas vivas com pessoas e agentes](journeys/creation-capabilities.json)**.
- [ ] **C03. [Criar, editar, apresentar e exportar Slides](journeys/creation-capabilities.json)**.
- [ ] **C04. [Criar Sites com publicação explícita e rollback](journeys/creation-capabilities.json)**.
- [ ] **C05. [Gerar, editar e guardar Imagens](journeys/creation-capabilities.json)**.
- [ ] **C06. [Guardar e compartilhar Arquivos privados](journeys/creation-capabilities.json)**.
- [ ] **C07. [Buscar e compartilhar artefatos com papéis e revogação](journeys/creation-capabilities.json)**.
- [ ] **C08. [Restaurar lixeira e excluir permanentemente](journeys/creation-capabilities.json)**.
- [ ] **C09. [Revisar alterações atribuídas de agentes por artefato](journeys/creation-capabilities.json)**.
- [ ] **C10. [Reabrir offline e recuperar conflitos sem duplicação](journeys/creation-capabilities.json)**.

## Seis plataformas

O iOS abre a sequência; Web e Android dependem do marco iOS; desktop depende da expansão Web/Android. Todas preservam as jornadas e capacidades aplicáveis e interoperam pelo mesmo backend. [#184][i184].

- [ ] **iOS** — primeiro marco, com release e iPhone físico; [critérios de plataforma](journeys/crosscut-gates.json).
- [ ] **Web** — após o iOS, incluindo interoperabilidade e recuperação; [critérios de plataforma](journeys/crosscut-gates.json).
- [ ] **Android** — após o iOS, incluindo interoperabilidade e recuperação; [critérios de plataforma](journeys/crosscut-gates.json).
- [ ] **macOS** — após Web/Android, incluindo instalação, assinatura e atualização; [critérios de plataforma](journeys/crosscut-gates.json).
- [ ] **Windows** — após Web/Android, incluindo instalação, assinatura e atualização; [critérios de plataforma](journeys/crosscut-gates.json).
- [ ] **Linux** — após Web/Android, incluindo instalação, assinatura e atualização; [critérios de plataforma](journeys/crosscut-gates.json).

## Nove condições para concluir

Estas condições atravessam as jornadas; um frontend hospedado, um teste de carga isolado ou uma build aprovada não as encerram. [#182][i182] · [#183][i183] · [#184][i184].

- [ ] [Matriz completa de produto e comparação por jornada, versão e plataforma](journeys/crosscut-gates.json).
- [ ] [Artefatos finais exatos nas seis plataformas, com origem e distribuição verificáveis](journeys/crosscut-gates.json).
- [ ] [Backend público completo e provedores reais interoperando com os clientes](journeys/crosscut-gates.json).
- [ ] [Integridade criptográfica, operação offline e recuperação segura](journeys/crosscut-gates.json).
- [ ] [Carga, latência, disponibilidade, recuperação e custo medidos no perfil declarado](journeys/crosscut-gates.json).
- [ ] [Verificação de carga, falhas, segurança e acessibilidade sem lacunas críticas](journeys/crosscut-gates.json).
- [ ] [Observabilidade privada, proteção contra abuso e resposta a incidentes](journeys/crosscut-gates.json).
- [ ] [Releases reproduzíveis, migrações, restauração e rollback exercitados](journeys/crosscut-gates.json).
- [ ] [Arquitetura sustentável, autoridade única e remoção de legado comprovadamente sem uso](journeys/crosscut-gates.json).

O [envelope numérico](../dev/zoen-operating-envelope-v1.json) permanece **declarado, não medido**: 200 conexões sustentadas, pico de reconexão de 1.000 dispositivos e 20/100 gravações por segundo, além das metas de mídia, chamadas, latência, durabilidade e custo. Fixar hashes e inventário antes de duas execuções completas independentes; disponibilidade mensal exige **30 dias observados**. O perfil próprio de lançamento iOS não aprova por aproximação o perfil final das seis plataformas.

As falhas, limites e provas históricas continuam ligados às fontes originais. O próximo avanço deve terminar uma jornada e registrar o que passou, o que falhou e o que ainda falta no issue responsável.

[i172]: https://github.com/EnzoTironi/zoen-native/issues/172
[i173]: https://github.com/EnzoTironi/zoen-native/issues/173
[i175]: https://github.com/EnzoTironi/zoen-native/issues/175
[i176]: https://github.com/EnzoTironi/zoen-native/issues/176
[i177]: https://github.com/EnzoTironi/zoen-native/issues/177
[i178]: https://github.com/EnzoTironi/zoen-native/issues/178
[i179]: https://github.com/EnzoTironi/zoen-native/issues/179
[i180]: https://github.com/EnzoTironi/zoen-native/issues/180
[i181]: https://github.com/EnzoTironi/zoen-native/issues/181
[i182]: https://github.com/EnzoTironi/zoen-native/issues/182
[i183]: https://github.com/EnzoTironi/zoen-native/issues/183
[i184]: https://github.com/EnzoTironi/zoen-native/issues/184
[i199]: https://github.com/EnzoTironi/zoen-native/issues/199
[i200]: https://github.com/EnzoTironi/zoen-native/issues/200
[i249]: https://github.com/EnzoTironi/zoen-native/issues/249
[i266]: https://github.com/EnzoTironi/zoen-native/issues/266
[i288]: https://github.com/EnzoTironi/zoen-native/issues/288
[i296]: https://github.com/EnzoTironi/zoen-native/issues/296
[i305]: https://github.com/EnzoTironi/zoen-native/issues/305
[i312]: https://github.com/EnzoTironi/zoen-native/issues/312
[i360]: https://github.com/EnzoTironi/zoen-native/issues/360
[i365]: https://github.com/EnzoTironi/zoen-native/issues/365
[i419]: https://github.com/EnzoTironi/zoen-native/issues/419
[i440]: https://github.com/EnzoTironi/zoen-native/issues/440
[i446]: https://github.com/EnzoTironi/zoen-native/issues/446
[i489]: https://github.com/EnzoTironi/zoen-native/issues/489
[i494]: https://github.com/EnzoTironi/zoen-native/issues/494
[i518]: https://github.com/EnzoTironi/zoen-native/issues/518
[i529]: https://github.com/EnzoTironi/zoen-native/issues/529
[i583]: https://github.com/EnzoTironi/zoen-native/issues/583
[i615]: https://github.com/EnzoTironi/zoen-native/issues/615
[i622]: https://github.com/EnzoTironi/zoen-native/issues/622
