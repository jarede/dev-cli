# CHECKPOINT — Release v0.2.0: portal embutido no dev-server

Handoff para agente externo (OpenCode / deepseek-v4-flash-free).
Plano completo: `docs/superpowers/plans/2026-07-30-release-portal-embutido.md`
Spec: `docs/superpowers/specs/2026-07-30-release-portal-embutido-design.md`

## Escopo deste handoff

Executar as **Tasks 1 a 4** do plano, NA ORDEM, seguindo cada task passo a
passo (os passos têm código pronto — copie fielmente, incluindo os
comentários didáticos). **NÃO executar a Task 5** (tag/release — reservada
ao operador). **NÃO fazer `git push` nem criar tags.** Commits locais sim —
um por task, com a mensagem indicada no passo de commit.

## Status das tasks (ATUALIZAR AQUI a cada task concluída)

| Task | Descrição | Status | Commit |
|---|---|---|---|
| 1 | build.rs + portal.rs (include_dir + fallback) | pendente | — |
| 2 | precedência no main.rs + teste caminho_db | pendente | — |
| 3 | release.yml embala portal + dev-server | pendente | — |
| 4 | README + CLAUDE.md | pendente | — |
| 5 | tag v0.2.0 + verificação | **fora do escopo — não fazer** | — |

## Estado no início

- Último commit: `af0dbb3` (plano). Working tree limpa. Branch `main`,
  sincronizada com origin até `241eb6b` (docs/specs/plano ainda não pushados
  — e devem continuar assim; sem push).
- `web/dist` EXISTE (build recente) — os testes `#[cfg(portal_embutido)]`
  da Task 1 devem rodar localmente.

## Como trabalhar

- Antes de cada commit: `cargo fmt --all` e os gates da task (`cargo test
  --workspace`, `cargo clippy --workspace` — zero warnings; clippy com
  warning = task incompleta).
- Marcar os checkboxes `- [ ]` do plano conforme concluir os passos, e
  atualizar a tabela acima (status + hash) no MESMO commit da task.
- Se um passo falhar de um jeito que o plano não previu: PARAR, registrar o
  erro aqui embaixo numa seção "## Bloqueios", e não improvisar solução
  fora do plano.
- Convenções do repo em `CLAUDE.md` (pt-br, comentários didáticos, sem
  unwrap fora de teste, Conventional Commits pt-br).

## Como retomar (se a sessão cair)

Reler esta tabela; a primeira task "pendente" é a próxima. Conferir
`git log --oneline -5` contra a coluna Commit antes de refazer qualquer
coisa.
