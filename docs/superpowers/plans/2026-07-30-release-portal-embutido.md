# Release v0.2.0: portal embutido no dev-server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dev-server` instalado por brew/scoop/tar.gz sobe backend + frontend com um comando e zero config; tag v0.2.0 fecha a versão com a pipeline existente.

**Architecture:** Spec em `docs/superpowers/specs/2026-07-30-release-portal-embutido-design.md`. O `web/dist` é embutido no binário via `include_dir` (compile-time), com `build.rs` que detecta o dist e liga um `cfg`; sem dist, o build local continua verde e serve uma página explicativa. `portal_dir` configurado mantém precedência (fluxo dev). A pipeline de release compila o portal antes do cargo e embala os DOIS binários nos 4 alvos.

**Tech Stack:** Rust (axum, include_dir, build.rs com rustc-cfg), GitHub Actions (setup-node + matriz existente), Homebrew/Scoop.

## Global Constraints

- pt-br em código/comentários; comentários didáticos (conceito/"porquê") em todo código novo.
- Sem `unwrap()` fora de `#[cfg(test)]`; clippy-clean (`-D warnings` no CI); let chains; `cargo fmt --all` antes de commitar.
- O `build.rs` NÃO roda npm nem nenhum comando externo — só olha o filesystem.
- `deploy/instalar.sh` e `rust.yml` (CI normal) não mudam.
- Commits: Conventional Commits pt-br.
- Descoberta pré-plano (ajuste sobre a spec §2): `caminho_db` JÁ usa `dirs::data_local_dir()` (portátil incl. Windows) e o dir-pai do banco já é criado em `main.rs:116` — a tarefa vira teste de confirmação, sem mudança de comportamento.

---

### Task 1: `build.rs` + módulo `portal.rs` no crate servidor

**Files:**
- Create: `crates/servidor/build.rs`
- Create: `crates/servidor/src/portal.rs`
- Modify: `crates/servidor/src/main.rs` (declarar `mod portal;`, usar na precedência — ver Task 2)
- Modify: `crates/servidor/Cargo.toml` (dep `include_dir`), `Cargo.toml` raiz (workspace dep)
- Test: `crates/servidor/src/portal.rs` (bloco `#[cfg(test)]`)

**Interfaces:**
- Produces: `portal::rotas_portal() -> axum::Router` — um router SÓ com o fallback do portal (embutido ou página mínima), para o `main.rs` usar via `merge`/`fallback_service` e para os testes montarem isolado. `portal::descricao() -> &'static str` — string para o log de subida ("embutido" | "ausente (build sem web/dist)").

- [ ] **Step 1: Dependência**

`Cargo.toml` raiz, em `[workspace.dependencies]`:

```toml
include_dir = "0.7"
```

`crates/servidor/Cargo.toml`, em `[dependencies]`:

```toml
include_dir.workspace = true
```

- [ ] **Step 2: `crates/servidor/build.rs`**

```rust
// Build script: decide EM COMPILE-TIME se o portal (web/dist) existe para
// ser embutido no binário. `cargo::rustc-cfg` liga um flag de compilação
// condicional (`#[cfg(portal_embutido)]`) que o código-fonte consulta —
// assim `cargo build` numa máquina sem o build do frontend continua verde,
// só que servindo uma página explicativa no lugar do portal.
// `rustc-check-cfg` declara o cfg customizado para o rustc não emitir o
// warning `unexpected_cfgs` (todo cfg fora da lista padrão precisa disso).
// docs: https://doc.rust-lang.org/cargo/reference/build-scripts.html
fn main() {
    println!("cargo::rustc-check-cfg=cfg(portal_embutido)");
    // CARGO_MANIFEST_DIR = crates/servidor; o dist fica dois níveis acima.
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if dist.join("index.html").exists() {
        println!("cargo::rustc-cfg=portal_embutido");
    }
    // Recompila quando o build do frontend mudar (ou aparecer/sumir).
    println!("cargo::rerun-if-changed=../../web/dist");
}
```

- [ ] **Step 3: Testes que falham (`portal.rs`, bloco de teste)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get(caminho: &str) -> (StatusCode, String, String) {
        let resposta = rotas_portal()
            .oneshot(Request::builder().uri(caminho).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resposta.status();
        let content_type = resposta
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").to_string())
            .unwrap_or_default();
        let corpo = axum::body::to_bytes(resposta.into_body(), usize::MAX).await.unwrap();
        (status, content_type, String::from_utf8_lossy(&corpo).into_owned())
    }

    // Com o dist embutido (o caso do binário de release e do dev local
    // depois de `npm run build`): raiz e rotas SPA devolvem o index.html.
    #[cfg(portal_embutido)]
    #[tokio::test]
    async fn raiz_e_rota_spa_devolvem_index_html() {
        let (status, ct, corpo) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
        assert!(corpo.contains("<div id=\"root\">"));

        // Rota que só existe no react-router: fallback SPA para o index.
        let (status, ct, _) = get("/historico").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
    }

    #[cfg(portal_embutido)]
    #[tokio::test]
    async fn asset_inexistente_com_extensao_devolve_404() {
        let (status, _, _) = get("/assets/nao-existe.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // Sem o dist no compile: página mínima explicativa em qualquer rota.
    #[cfg(not(portal_embutido))]
    #[tokio::test]
    async fn sem_dist_serve_pagina_explicativa() {
        let (status, ct, corpo) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
        assert!(corpo.contains("portal não compilado"));
    }
}
```

- [ ] **Step 4: Rodar para ver falhar**

Run: `cargo test -p servidor portal -v`
Expected: FAIL de compilação — `portal.rs` não existe.

- [ ] **Step 5: Implementar `portal.rs`**

```rust
// Portal embutido: os arquivos de `web/dist` entram no binário em
// compile-time via `include_dir!` — depois de instalado, `dev-server`
// serve API e frontend sozinho, sem nenhum arquivo externo.
//
// O `#[cfg(portal_embutido)]` vem do build.rs: com `web/dist` presente no
// compile, o bloco embutido é usado; sem ele (ex.: `cargo build` sem ter
// rodado `npm run build`), cai na página explicativa — o build nunca quebra
// por falta do frontend.
// docs: https://docs.rs/include_dir/latest/include_dir/

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

/// Router com um único fallback: o portal (embutido ou explicativo).
/// Isolado num router próprio para o main.rs "pendurar" só quando não há
/// `portal_dir` configurado, e para os testes montarem sem o resto da API.
pub fn rotas_portal() -> Router {
    Router::new().fallback(get(servir))
}

/// Texto para o log de subida do main.rs.
pub fn descricao() -> &'static str {
    if cfg!(portal_embutido) {
        "embutido no binário"
    } else {
        "ausente (build sem web/dist — rode `cd web && npm run build` e recompile, ou configure servidor.portal_dir)"
    }
}

#[cfg(portal_embutido)]
static DIST: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");

/// Content-Type pela extensão. Cobrimos só o que o build do Vite gera —
/// um `match` explícito é mais didático (e auditável) que uma dependência
/// de adivinhação de MIME para meia dúzia de casos.
/// docs: https://developer.mozilla.org/docs/Web/HTTP/Basics_of_HTTP/MIME_types
#[cfg(portal_embutido)]
fn content_type(caminho: &str) -> &'static str {
    match caminho.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(portal_embutido)]
async fn servir(uri: Uri) -> Response {
    let caminho = uri.path().trim_start_matches('/');

    // Arquivo real do dist (ex.: /assets/index-abc.js)?
    if let Some(arquivo) = DIST.get_file(caminho) {
        return ([(header::CONTENT_TYPE, content_type(caminho))], arquivo.contents())
            .into_response();
    }

    // Caminho com extensão que NÃO existe no dist = 404 de verdade (um
    // asset quebrado deve falhar alto, não devolver HTML disfarçado).
    // Sem extensão = rota do react-router (/historico, /testes...) — a SPA
    // resolve no cliente, então devolvemos o index.html (mesmo fallback do
    // ServeDir::not_found_service usado no modo portal_dir).
    let ultimo_segmento = caminho.rsplit('/').next().unwrap_or("");
    if ultimo_segmento.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    match DIST.get_file("index.html") {
        Some(index) => {
            ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], index.contents())
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Sem dist no compile: qualquer rota devolve a página explicativa —
/// honesta sobre o estado do binário, com o passo-a-passo de correção.
#[cfg(not(portal_embutido))]
async fn servir(_uri: Uri) -> Response {
    Html(
        "<!doctype html><meta charset=\"utf-8\"><title>dev-cli · portal</title>\
         <body style=\"font-family:serif;max-width:40rem;margin:4rem auto;line-height:1.6\">\
         <h1>portal não compilado neste binário</h1>\
         <p>Este build do <code>dev-server</code> foi feito sem o frontend. \
         Rode <code>cd web && npm run build</code> e recompile, ou configure \
         <code>servidor.portal_dir</code> apontando para um build do portal. \
         A API continua no ar em <code>/api/*</code>.</p>",
    )
    .into_response()
}
```

Em `main.rs`, adicionar `mod portal;` junto dos outros `mod`.

- [ ] **Step 6: Rodar até passar**

Run: `cargo test -p servidor portal -v`
Expected: PASS (localmente `web/dist` existe, então rodam os testes `#[cfg(portal_embutido)]`). Sanidade extra do fallback: `mv web/dist /tmp/dist-bk && cargo test -p servidor portal -v && mv /tmp/dist-bk web/dist` — deve rodar o teste da página explicativa e passar.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add Cargo.toml Cargo.lock crates/servidor/Cargo.toml crates/servidor/build.rs crates/servidor/src/portal.rs crates/servidor/src/main.rs
git commit -m "feat(servidor): embute o build do portal no binário via include_dir"
```

---

### Task 2: Precedência no `main.rs` + teste do default portátil do banco

**Files:**
- Modify: `crates/servidor/src/main.rs:186-205` (o bloco `if !config.servidor.portal_dir.is_empty()`)
- Test: `crates/nucleo/src/config.rs` (bloco `#[cfg(test)]` existente)

**Interfaces:**
- Consumes: `portal::rotas_portal()`, `portal::descricao()` (Task 1).

- [ ] **Step 1: Precedência de servir o portal**

Substituir o bloco atual (que só serve com `portal_dir` preenchido) por:

```rust
    if !config.servidor.portal_dir.is_empty() {
        // Override explícito (TOML/env/flag): serve do diretório indicado —
        // fluxo de desenvolvimento ou deploy com portal separado. Mantém o
        // fallback SPA do react-router via not_found_service (ver comentário
        // original sobre /historico e recarga de página).
        let index_html = PathBuf::from(&config.servidor.portal_dir).join("index.html");
        rotas = rotas.fallback_service(
            tower_http::services::ServeDir::new(&config.servidor.portal_dir)
                .not_found_service(tower_http::services::ServeFile::new(index_html)),
        );
        println!("portal: dir {}", config.servidor.portal_dir);
    } else {
        // Sem override: o portal embutido no binário (ou a página
        // explicativa, num build sem web/dist) — é o que faz `dev-server`
        // subir frontend + backend com um único comando pós-install.
        rotas = rotas.merge(portal::rotas_portal());
        println!("portal: {}", portal::descricao());
    }
```

(Preservar o comentário didático original sobre SPA/`not_found_service` no primeiro ramo — mover, não apagar.)

- [ ] **Step 2: Teste do default portátil do banco (nucleo)**

No `#[cfg(test)]` de `crates/nucleo/src/config.rs`:

```rust
#[test]
fn caminho_db_default_e_portavel_e_absoluto() {
    // Sem `db` configurado, o caminho vem de dirs::data_local_dir() —
    // Linux: ~/.local/share, macOS: ~/Library/Application Support,
    // Windows: %LOCALAPPDATA% — sempre terminando em dev-cli/logs.db.
    let config = Config::default();
    let caminho = config.caminho_db();
    assert!(caminho.ends_with("dev-cli/logs.db") || caminho.ends_with("dev-cli\\logs.db"));
    // Num ambiente com home resolvível (CI e dev), o caminho é absoluto —
    // o "." de fallback só aparece em ambientes sem diretório de dados.
    assert!(caminho.is_absolute() || caminho.starts_with("."));
}
```

- [ ] **Step 3: Cobertura da precedência**

O ramo `portal_dir` configurado já tem teste em `crates/servidor/src/api.rs`
(`portal_estatico_e_servido_como_fallback`, com tempdir + `ServeDir`) e o
ramo embutido ganhou os testes da Task 1 — os dois lados do `if/else` estão
cobertos. Conferir que o teste existente segue passando após a mudança do
`main.rs` (ele monta o fallback por conta própria, não deve quebrar); se o
nome/comentário dele ficar defasado em relação ao novo log "portal: dir …",
ajustar só o comentário.

- [ ] **Step 4: Rodar tudo**

Run: `cargo test --workspace && cargo clippy --workspace`
Expected: verde, sem warnings. Sanidade manual: `cargo run -p servidor -- --db /tmp/dev-v020.db`, abrir `http://127.0.0.1:8787` → portal carrega (embutido); `curl -s localhost:8787/api/saude` responde; log mostra "portal: embutido no binário".

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add crates/servidor/src/main.rs crates/nucleo/src/config.rs
git commit -m "feat(servidor): portal embutido como fallback default e teste do caminho portátil do banco"
```

---

### Task 3: Pipeline de release embala portal + dev-server

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: o cfg `portal_embutido` liga sozinho porque o npm build roda ANTES do cargo (Task 1 build.rs).

- [ ] **Step 1: Build do frontend antes do cargo (job `build`)**

Depois do checkout e ANTES do toolchain Rust, inserir:

```yaml
      # O portal precisa existir em web/dist ANTES do cargo: o build.rs do
      # crate servidor detecta o dist e embute os assets no binário.
      - name: Instala Node e compila o portal
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json
      - name: Build do portal (web/dist)
        shell: bash
        working-directory: web
        run: |
          set -euo pipefail
          npm ci
          npm run build
```

- [ ] **Step 2: Compilar e empacotar os dois binários**

Trocar o passo de compilação por:

```yaml
      - name: Compila em modo release
        shell: bash
        run: cargo build --release --locked --target "$TARGET" -p dev-cli -p servidor
```

Empacotamento Unix — trocar o miolo por:

```bash
          nome="dev-cli-${TAG}-${TARGET}"
          cp "target/${TARGET}/release/dev-cli" dev-cli
          cp "target/${TARGET}/release/dev-server" dev-server
          tar -czf "dist/${nome}.tar.gz" dev-cli dev-server README.md
          rm dev-cli dev-server
```

Empacotamento Windows — trocar o miolo por:

```powershell
          $nome = "dev-cli-$env:TAG-$env:TARGET"
          Copy-Item "target/$env:TARGET/release/dev-cli.exe" -Destination "dev-cli.exe"
          Copy-Item "target/$env:TARGET/release/dev-server.exe" -Destination "dev-server.exe"
          Compress-Archive -Path "dev-cli.exe","dev-server.exe","README.md" -DestinationPath "dist/$nome.zip"
          Remove-Item "dev-cli.exe","dev-server.exe"
```

- [ ] **Step 3: Fórmula e manifest instalam os dois binários**

Na fórmula gerada (`Regenera Formula/dev-cli.rb`), trocar o `def install`:

```ruby
            def install
              bin.install "dev-cli"
              bin.install "dev-server"
            end
```

No manifest Scoop (`Regenera bucket/dev-cli.json`), trocar a linha do bin:

```json
                      "bin": ["dev-cli.exe", "dev-server.exe"]
```

- [ ] **Step 4: Validar sintaxe e commit**

Run: `node -e "console.log('yaml ok')" && python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml')); print('yaml ok')"` (se o python com pyyaml não existir, conferir com o linter do editor).
Expected: YAML válido.

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): compila o portal antes do cargo e embala dev-server nos artefatos"
```

---

### Task 4: Documentação (README + CLAUDE.md)

**Files:**
- Modify: `README.md` (seção de instalação/uso)
- Modify: `CLAUDE.md` (seção Comandos)

- [ ] **Step 1: README**

Na seção de instalação (após os comandos brew/scoop), adicionar:

```markdown
### Subir o portal + API com um comando

Depois de instalado, basta:

    dev-server

Backend e frontend sobem juntos em <http://127.0.0.1:8787> — o portal web
vem embutido no binário. Sem config nenhuma na primeira execução: o banco
SQLite é criado sozinho no diretório de dados do usuário
(`~/.local/share/dev-cli/logs.db` no Linux, equivalente no macOS/Windows), e
tudo é ajustável depois por `/etc/dev-cli/config.toml` ou variáveis
`DEV_CLI_*`. Sem docker rodando, a API e o portal sobem mesmo assim — a
coleta só começa quando o docker aparecer.
```

- [ ] **Step 2: CLAUDE.md**

Na seção Comandos, adicionar uma linha após o comando do servidor:

```markdown
# release: `npm run build` + `cargo build --release` embute web/dist no dev-server (build.rs de crates/servidor); sem dist, o binário serve uma página explicativa
```

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: instrução do dev-server com portal embutido e defaults de primeira execução"
```

---

### Task 5: Fechamento v0.2.0 (executar por último, com o usuário ciente)

**Files:** nenhum (tag + verificação).

- [ ] **Step 1: Gates completos**

Run:
```bash
cargo fmt --all -- --check && cargo test --workspace && cargo clippy --workspace -- -D warnings
cd web && npm test && npm run build && cd ..
```
Expected: tudo verde.

- [ ] **Step 2: Push + tag**

```bash
git push
git tag v0.2.0
git push origin v0.2.0
```

- [ ] **Step 3: Acompanhar a pipeline e verificar a release**

Run: `gh run watch --exit-status` (workflow "Release" disparado pela tag) e depois:

```bash
gh release view v0.2.0
# baixa o artefato do host atual e confere o one-command:
gh release download v0.2.0 --pattern "*aarch64-apple-darwin*" --dir /tmp/v020
cd /tmp/v020 && tar -xzf dev-cli-v0.2.0-aarch64-apple-darwin.tar.gz
./dev-server --db /tmp/v020.db &
sleep 2
curl -s localhost:8787/api/saude
curl -s localhost:8787/ | head -c 200   # deve ser o HTML do portal
kill %1
```
Expected: `/api/saude` responde; `/` devolve o HTML do portal (título "dev-cli · portal"). Conferir também que o commit automático da fórmula/bucket apareceu na `main` (`git pull` depois).
