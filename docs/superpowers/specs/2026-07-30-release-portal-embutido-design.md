# Release v0.2.0: portal embutido no dev-server — design

Data: 2026-07-30 · Status: aprovado em conversa (Abordagem A)

## Objetivo

Fechar a versão **v0.2.0** de modo que, instalado por qualquer via (brew,
scoop, tar.gz, `cargo build`), **um único comando — `dev-server` — suba
backend e frontend juntos**, com defaults que funcionam em qualquer
computador já na primeira execução (sem config, sem env, sem arquivos
extras).

## Decisões já tomadas

- **Abordagem A**: o build de release compila o portal e **embute o
  `web/dist` no binário do `dev-server`** via crate `include_dir`
  (compile-time). Descartadas: dist solto no tar.gz (frágil no brew/scoop) e
  download na primeira execução (dependência de rede).
- Versão: **v0.2.0** (o workspace já declara 0.2.0; só falta a tag).
- `deploy/instalar.sh` (RHEL/systemd) permanece como está — o caminho
  "qualquer computador" é o binário de gerenciador.

## 1. Portal embutido (`crates/servidor`)

- Dependência nova: `include_dir` (workspace). A macro `include_dir!`
  embute `web/dist` no binário em compile-time; acesso zero-cost em runtime.
- **Fallback de compile**: `cargo build` local sem `web/dist` presente NÃO
  pode quebrar nem rodar npm escondido. Um `build.rs` no crate `servidor`
  verifica se `../../web/dist/index.html` existe e exporta um `cfg` (ex.:
  `cargo::rustc-cfg=portal_embutido`); sem o dist, o binário embute só uma
  página mínima estática (HTML inline no código, estética neutra) dizendo:
  "portal não compilado neste binário — rode `cd web && npm run build` e
  recompile, ou configure `servidor.portal_dir`". O `build.rs` também
  imprime `cargo::rerun-if-changed=../../web/dist` para recompilar quando o
  dist mudar.
- **Precedência de servir o portal** (rota fallback do axum):
  1. `portal_dir` configurado (TOML ou `DEV_CLI_SERVIDOR_PORTAL_DIR`) →
     `ServeDir` como hoje (fluxo de desenvolvimento/override).
  2. Senão, com portal embutido → assets do `include_dir!` (content-type por
     extensão; `index.html` para `/` e para caminhos sem extensão — SPA com
     react-router precisa do fallback de rota para `index.html`).
  3. Senão (build local sem dist) → página mínima explicativa.
- As rotas `/api/*` não mudam. O log de subida informa a origem do portal
  ("portal: embutido (vX.Y.Z)" / "portal: dir <caminho>" / "portal:
  ausente").

## 2. Defaults portáveis de primeira execução

- **Banco**: `Config::caminho_db()` hoje expande `~/.local/share/dev-cli/`
  via `HOME`. Ajustar para resolver de forma portável: `HOME` (Unix) com
  fallback para `USERPROFILE` (Windows); no Windows o default vira
  `%USERPROFILE%\.local\share\dev-cli\logs.db` (mesma árvore relativa —
  simples e previsível; sem introduzir crate `dirs` só para isso). O
  diretório é criado com `create_dir_all` na primeira subida (confirmar que
  já acontece; se não, criar).
- **Bind**: `127.0.0.1:8787` (atual, já portátil). **Coleta**: docker local
  (atual). **Sem NENHUMA env obrigatória**: toda `DEV_CLI_*` continua
  override opcional, documentada no `deploy/config.exemplo.toml`.
- Primeira execução sem docker instalado: o coletor já tolera falha de
  coleta (loga e tenta de novo); o portal e a API sobem mesmo assim —
  confirmar esse comportamento num teste manual e registrar no README.

## 3. Release e gerenciadores (`.github/workflows/release.yml`)

- Job `build` passa a: (1) `actions/setup-node` (Node 22 LTS) + `cd web &&
  npm ci && npm run build` ANTES do cargo; (2) compilar TAMBÉM o
  `dev-server` (`cargo build --release --locked --target $TARGET -p dev-cli
  -p servidor`); (3) empacotar `dev-cli` + `dev-server` (+ `.exe` no
  Windows) no mesmo tar.gz/zip.
- Fórmula Homebrew: `bin.install "dev-cli"` + `bin.install "dev-server"`; o
  bloco `test do` continua no `dev-cli version`.
- Manifest Scoop: `"bin": ["dev-cli.exe", "dev-server.exe"]`.
- CI normal (`rust.yml`): inalterado (o fallback de compile garante build
  verde sem npm).

## 4. Documentação

- README: seção "Instalação" ganha o one-liner pós-install: `dev-server` →
  abre `http://127.0.0.1:8787`. Nota sobre defaults (banco em
  `~/.local/share/dev-cli/`, override por `DEV_CLI_*`).
- `CLAUDE.md`: uma linha na seção Comandos sobre o portal embutido (build de
  release embute `web/dist`; dev continua com Vite/proxy).

## 5. Fechamento da versão (executar APÓS o merge desta spec/implementação)

1. Gates completos (`cargo fmt/test/clippy`, `npm test/build`).
2. `git tag v0.2.0 && git push --tags` — a pipeline existente cria a
   Release, os artefatos dos 4 alvos e atualiza fórmula/bucket.
3. Verificar a Release publicada: baixar o tar.gz de um alvo e conferir que
   `dev-server` sobe o portal sozinho (`curl localhost:8787/` devolve o
   HTML do portal, `/api/saude` responde).

## Testes

- Rust: teste de rota do axum servindo o portal embutido (quando o cfg está
  ativo, `GET /` devolve 200 com `text/html`; `GET /rota-spa-qualquer`
  idem — fallback SPA; `GET /assets/inexistente.js` → 404) e teste de que
  `portal_dir` configurado tem precedência sobre o embutido (fixture com
  tempdir). Sem dist no compile, os testes do fallback mínimo (`GET /` →
  200 com a página explicativa).
- `caminho_db`: teste unitário do fallback `HOME`/`USERPROFILE`.
- Web: nada muda (o dist é consumido como está).

## Fora de escopo

- Unificar `dev-cli` e `dev-server` num binário só.
- Auto-update, download de portal em runtime, empacotar o instalador
  systemd nos gerenciadores.
- Migração do exemplo `config.exemplo.toml` (segue válido).
