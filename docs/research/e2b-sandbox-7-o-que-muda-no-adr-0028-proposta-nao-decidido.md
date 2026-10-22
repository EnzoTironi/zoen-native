[Research index](e2b-sandbox.md).

## 7. O que muda no ADR 0028 (proposta, não decidido)
1. Novo tier **T2d "desktop"** (ou um `display: true` no tier T2), com manifest
   `desktop { egress, apps, max_secs }`, escalado só quando o navegador sozinho não resolve.
2. Formato de snapshot do `zoen-sandboxd` = formato da E2B (cadeia de diffs + índice por
   bloco) **mais cifragem por bloco**; substitui "dm-thin" no §2.
3. `userfaultfd` + pré-carga gravada no build sobe de prioridade no P1.
4. Teste de carga do P4 inclui 100 criações simultâneas (a lição do issue #3012).
5. Nada de jailer a menos, nada de placa de rede, nada de porta pública: as três diferenças
   em que estamos mais fechados que a E2B continuam.
