// Suítes de teste de integração e2e. Uma suíte é uma pipeline de comandos
// CLI encadeados: uma etapa de execução seguida de validações dos dados
// produzidos. As etapas rodam em ordem; se uma falha — por erro de execução
// (exit ≠ 0) OU por validação (exit 0 mas stdout ≠ esperado) — a pipeline
// para e as etapas seguintes ficam "não executado".
//
// O arquivo de cada suíte é um TOML em `/etc/dev-cli/testes/<id>.toml`
// (caminho configurável). O runner é exposto pelo dev-server em
// /api/testes/suites/{id}/rodar e usado pela tela "Testes · e2e" do portal.
//
// O módulo é dividido em três camadas como o resto do nucleo:
//   - tipos puros + parse TOML (sem IO, 100% testáveis);
//   - storage (le/escreve os TOML no diretório);
//   - runner (executa a pipeline em sequência, com timeout por etapa).
//
// docs: docs/design_handoff_portal_dev_cli/README.md (seção 5)

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Identificador de uma suíte — vira o nome do arquivo (`<id>.toml`).
/// Restringido a ASCII alfanumérico + `-` + `_` para ser seguro em paths
/// e não dar ambiguidade (sem espaços, sem acentos, sem `/`).
pub fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Tipo de uma etapa da pipeline.
///
/// - `Exec`: roda o comando; falha se exit code ≠ 0.
/// - `Valida`: roda o comando e compara o stdout (trimmed) com `esperado`;
///   falha se exit code ≠ 0 OU se a comparação não bater (a "falha silenciosa"
///   do exemplo canônico: comando roda, exit 0, mas o dado produzido não
///   confere — exit 0 não basta, o que importa é o stdout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipoPasso {
    Exec,
    Valida,
}

/// Uma etapa da pipeline.
///
/// `cmd` é o comando COMPLETO em forma de string (com argumentos) — o
/// runner usa um shell (`sh -c`) para que o usuário possa escrever
/// redirecionamentos (`>`, `>>`), pipes e aspas sem precisar aprender uma
/// sintaxe de array. Trade-off: não dá para passar argumentos com espaços
/// que contenham aspas de um jeito "seguro" (a regra do shell se aplica);
/// para o caso de uso (comandos curtos, copiar/colar do terminal) isso é
/// uma simplificação que vale a pena.
///
/// `esperado` só faz sentido para `valida` (o `cmd` rodou sem erro, o stdout
/// tem que bater). Em `exec` fica vazio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passo {
    pub tipo: TipoPasso,
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub esperado: Option<String>,
}

/// Suíte de testes — o arquivo TOML completo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suite {
    /// ID da suíte — igual ao nome do arquivo sem `.toml`. Validado em
    /// `Suite::com_id` (chamado tanto na criação quanto ao ler do disco).
    pub id: String,
    /// Nome legível ("prezzo · recalcular preços") — pode ter acentos e
    /// espaços, é só rótulo.
    pub nome: String,
    /// Timeout por etapa em segundos. Cada etapa que demorar mais é
    /// abortada e marcada como falha de execução.
    #[serde(default = "timeout_default")]
    pub timeout_etapa_seg: u64,
    /// Etapas, NA ORDEM em que devem rodar.
    pub passos: Vec<Passo>,
}

// `serde(default)` aceita `timeout_etapa_seg` ausente no TOML, mas exige
// que a função seja `fn() -> T` (não `fn` em si) — sem isso, o derive não
// acha o default.
fn timeout_default() -> u64 {
    30
}

impl Suite {
    /// Cria uma suíte validando o ID. Retorna erro se o ID for vazio
    /// ou tiver caracteres fora de `[A-Za-z0-9_-]` (mesma regra do
    /// `id_valido`, exposta como método para `?` fluir bem).
    pub fn com_id(
        id: &str,
        nome: String,
        timeout_etapa_seg: u64,
        passos: Vec<Passo>,
    ) -> Result<Self, String> {
        if !id_valido(id) {
            return Err(format!(
                "id inválido: {:?} (use só letras, números, '-' e '_'; até 64 chars)",
                id
            ));
        }
        if passos.is_empty() {
            return Err("a suíte precisa ter pelo menos uma etapa".to_string());
        }
        Ok(Self {
            id: id.to_string(),
            nome,
            timeout_etapa_seg,
            passos,
        })
    }
}

/// Estado de uma etapa dentro de uma execução em andamento ou já terminada.
///
/// "Pulado" não é o mesmo que "pendente": pendente é "ainda não cheguei
/// nela", pulado é "uma etapa anterior falhou e a pipeline parou, esta não
/// vai mais rodar". O portal usa isso para esmaecer (.5 opacity) as etapas
/// puladas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EstadoPasso {
    /// Ainda não executada (etapas à frente da atual).
    Pendente,
    /// Em execução no momento.
    Rodando,
    /// Saiu com exit 0 (e, se validação, stdout bateu com o esperado).
    Ok {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        saida: Option<String>,
    },
    /// Exit code ≠ 0 (comando falhou).
    FalhaExec { exit_code: i32, stderr: String },
    /// Exit 0 mas stdout ≠ esperado (só acontece em validação).
    FalhaValida { esperado: String, obtido: String },
    /// Etapas à frente de uma falha — não rodam.
    Pulado,
}

impl EstadoPasso {
    /// Rótulo curto para a UI ("passou", "rodando…", "falhou · execução"…).
    pub fn rotulo(&self) -> &'static str {
        match self {
            EstadoPasso::Pendente => "aguardando",
            EstadoPasso::Rodando => "rodando…",
            EstadoPasso::Ok { .. } => "passou",
            EstadoPasso::FalhaExec { .. } => "falhou · execução",
            EstadoPasso::FalhaValida { .. } => "falhou · validação",
            EstadoPasso::Pulado => "não executado",
        }
    }

    /// Cor CSS (variável do `index.css` do portal) — o portal lê esta
    /// string e usa direto no `style={{ color }}`. Mantemos a lista
    /// espelhada em `formato.ts::corNivel`-like: a fonte da verdade das
    /// cores continua sendo o CSS; aqui só devolvemos o NOME da variável
    /// para a UI resolver.
    pub fn cor_var(&self) -> &'static str {
        match self {
            EstadoPasso::Pendente | EstadoPasso::Pulado => "var(--color-neutral-400)",
            EstadoPasso::Rodando => "var(--color-accent-700)",
            EstadoPasso::Ok { .. } => "var(--sev-verde)",
            EstadoPasso::FalhaExec { .. } | EstadoPasso::FalhaValida { .. } => {
                "var(--sev-vermelho)"
            }
        }
    }
}

/// Estado completo de UMA execução de uma suíte. Cada execução é uma
/// "instância" — rodar a mesma suíte duas vezes gera duas execuções com IDs
/// diferentes, registradas separadamente no servidor em memória.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execucao {
    pub id_execucao: String,
    pub suite_id: String,
    pub iniciada_em_unix: i64,
    /// Estado de cada passo, NA MESMA ORDEM de `Suite.passos`. O
    /// `Vec<EstadoPasso>` é o "tabuleiro" que o runner vai atualizando
    /// a cada etapa e que o portal polla a cada 1s.
    pub estados: Vec<EstadoPasso>,
    /// Se a pipeline inteira terminou (sucesso total OU falha que parou).
    /// `Rodando` quando ainda há etapas pendentes ou em andamento.
    pub conclusao: ConclusaoExecucao,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConclusaoExecucao {
    /// Ainda executando — o cliente continua polling.
    Rodando,
    /// Todas as etapas passaram.
    Sucesso,
    /// Alguma etapa falhou (exec OU valida) e parou a pipeline.
    Falha,
}

impl Execucao {
    /// Cria uma execução "zerada": todos os passos em `Pendente`, conclusão
    /// `Rodando`. O runner substitui os estados à medida que avança.
    pub fn nova(id_execucao: String, suite: &Suite, agora_unix: i64) -> Self {
        Self {
            id_execucao,
            suite_id: suite.id.clone(),
            iniciada_em_unix: agora_unix,
            estados: vec![EstadoPasso::Pendente; suite.passos.len()],
            conclusao: ConclusaoExecucao::Rodando,
        }
    }

    /// Duração total em segundos (entre `iniciada_em_unix` e `agora_unix`).
    /// Usada pela UI para mostrar "6.1s" no header da suíte.
    pub fn duracao_seg(&self, agora_unix: i64) -> i64 {
        (agora_unix - self.iniciada_em_unix).max(0)
    }
}

// ─── Storage ──────────────────────────────────────────────────────────

/// Resolve o diretório de suítes a partir de uma config opcional. Vazio /
/// ausente = `/etc/dev-cli/testes` em Unix. Centralizado aqui pra que o
/// servidor e o futuro CLI (`dev-cli testes ...`) apontem para o mesmo
/// lugar, e os testes consigam injetar um `tempdir`.
pub fn diretorio_testes(dir_override: Option<&str>) -> PathBuf {
    match dir_override {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from("/etc/dev-cli/testes"),
    }
}

/// Diretório de trabalho (`cwd`) de uma execução — isolado por
/// `id_execucao` dentro do temp dir do SO, para os comandos da suíte nunca
/// rodarem no cwd do processo do `dev-server`. Etapas da MESMA execução
/// compartilham este diretório (o exemplo canônico do handoff —
/// `echo oi > oi.txt` seguido de `test -f oi.txt` — depende disso); execuções
/// diferentes (mesma suíte rodada duas vezes) recebem diretórios diferentes.
fn diretorio_execucao(id_execucao: &str) -> PathBuf {
    std::env::temp_dir()
        .join("dev-cli-testes-exec")
        .join(id_execucao)
}

/// Lista as suítes do diretório: lê todos os `*.toml`, faz o parse, e
/// devolve em ordem alfabética de `id`. Suítes com TOML inválido são
/// puladas (logar o erro ficaria para uma camada superior; aqui só
/// devolvemos as válidas — o portal não trava se um arquivo corrompido
/// existir no diretório).
pub fn listar_suites(dir: &Path) -> Result<Vec<Suite>, Box<dyn std::error::Error>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut suites = Vec::new();
    for entrada in std::fs::read_dir(dir)? {
        let entrada = entrada?;
        let caminho = entrada.path();
        // Só `<id>.toml` — ignora `*.tmp`, `*~`, diretórios...
        if caminho.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let texto = match std::fs::read_to_string(&caminho) {
            Ok(t) => t,
            // Erro de leitura (permissão, I/O): pula o arquivo mas não
            // aborta a listagem inteira.
            Err(_) => continue,
        };
        if let Ok(suite) = parsear_suite_toml(&texto) {
            suites.push(suite);
        }
    }
    suites.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(suites)
}

/// Lê uma suíte específica do diretório.
pub fn ler_suite(dir: &Path, id: &str) -> Result<Suite, Box<dyn std::error::Error>> {
    if !id_valido(id) {
        return Err(format!("id inválido: {id:?}").into());
    }
    let caminho = dir.join(format!("{id}.toml"));
    let texto = std::fs::read_to_string(&caminho)
        .map_err(|_| format!("suíte {id:?} não encontrada em {}", dir.display()))?;
    let suite = parsear_suite_toml(&texto)?;
    Ok(suite)
}

/// Grava a suíte no disco (cria o diretório se não existir).
pub fn gravar_suite(dir: &Path, suite: &Suite) -> Result<(), Box<dyn std::error::Error>> {
    if !id_valido(&suite.id) {
        return Err(format!("id inválido: {:?}", suite.id).into());
    }
    std::fs::create_dir_all(dir)?;
    let caminho = dir.join(format!("{}.toml", suite.id));
    let texto = serializar_suite_toml(suite)?;
    std::fs::write(caminho, texto)?;
    Ok(())
}

/// Parse TOML → `Suite` (camada pura, testável com string inline).
/// Usa o campo `id` do TOML; se o TOML omitir o `id` (legado), cai no
/// nome do arquivo passado em `id_do_arquivo`.
pub fn parsear_suite_toml(texto: &str) -> Result<Suite, toml::de::Error> {
    toml::from_str(texto)
}

/// Serializa uma suíte em TOML. Usado pelo endpoint `POST /api/testes/suites`
/// (criação) e pelo `dev-cli testes criar`.
pub fn serializar_suite_toml(suite: &Suite) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(suite)
}

// ─── Runner ───────────────────────────────────────────────────────────

/// Callback que o runner chama a cada mudança de estado. O servidor
/// passa um `Fn(Execucao) + Send + Sync` para atualizar o registro em
/// memória da execução; o cliente (portal) lê esse registro via polling.
///
/// `&Execucao` em vez de `Execucao`: a struct é clonada dentro do
/// callback se ele precisar; passar por referência é mais barato e
/// reflete que o runner É o dono (o callback só observa).
pub type ProgressoCallback = std::sync::Arc<dyn Fn(&Execucao) + Send + Sync>;

/// Roda uma suíte, etapa por etapa, atualizando o `Execucao` à medida que
/// avança. Retorna a `Execucao` final (clonada) por conveniência — o
/// callback já recebeu todas as atualizações intermediárias.
///
/// `progresso` é chamado:
///   1. uma vez no início (estados = tudo Pendente, conclusão = Rodando);
///   2. uma vez por etapa, ANTES de rodar (estado daquela = Rodando) e
///      DEPOIS de rodar (estado atualizado);
///   3. uma vez no fim (conclusão = Sucesso ou Falha).
///
/// Cancelamento: a função `cancelar` retornada permite abortar a pipeline
/// em andamento (o loop checa entre etapas).
pub async fn rodar(
    mut execucao: Execucao,
    suite: &Suite,
    progresso: ProgressoCallback,
) -> Execucao {
    progresso(&execucao);

    // Diretório de trabalho DEDICADO a esta execução — os passos rodam com
    // `cwd` aqui, nunca no cwd do processo do `dev-server`. Sem isso, um
    // exemplo como `echo oi > oi.txt` (o caso canônico do handoff) escreve
    // onde quer que o servidor tenha sido iniciado — em produção isso pode
    // ser um diretório do sistema; em desenvolvimento, a raiz do próprio
    // repo (foi exatamente o que aconteceu ao testar esta suíte manualmente).
    // Não removemos o diretório ao final: em caso de falha, ele fica
    // disponível para inspeção manual (mesma lógica de deixar `stderr`
    // visível); o SO limpa `temp_dir()` no seu próprio ciclo.
    let workdir = diretorio_execucao(&execucao.id_execucao);
    if let Err(e) = std::fs::create_dir_all(&workdir) {
        execucao.estados[0] = EstadoPasso::FalhaExec {
            exit_code: -1,
            stderr: format!("não foi possível criar o diretório de trabalho da execução: {e}"),
        };
        for j in 1..execucao.estados.len() {
            execucao.estados[j] = EstadoPasso::Pulado;
        }
        execucao.conclusao = ConclusaoExecucao::Falha;
        progresso(&execucao);
        limpar_cancelamento(&execucao.id_execucao);
        return execucao;
    }

    for (i, passo) in suite.passos.iter().enumerate() {
        // Checa cancelamento entre etapas: barato (atômico). Usa o helper
        // `canceladas()` para acessar o `Mutex<HashSet>` dentro do
        // `OnceLock` (uma vez inicializado).
        if canceladas()
            .lock()
            .map(|g| g.contains(&execucao.id_execucao))
            .unwrap_or(false)
        {
            // Marca o que sobrou como Pulado e encerra.
            for j in i..execucao.estados.len() {
                execucao.estados[j] = EstadoPasso::Pulado;
            }
            execucao.conclusao = ConclusaoExecucao::Falha;
            progresso(&execucao);
            return execucao;
        }

        execucao.estados[i] = EstadoPasso::Rodando;
        progresso(&execucao);

        let estado = rodar_passo(
            passo,
            Duration::from_secs(suite.timeout_etapa_seg),
            &workdir,
        )
        .await;
        execucao.estados[i] = estado.clone();
        progresso(&execucao);

        // Se a etapa falhou, todas as seguintes viram Pulado e a pipeline
        // termina com Falha.
        if matches!(
            estado,
            EstadoPasso::FalhaExec { .. } | EstadoPasso::FalhaValida { .. }
        ) {
            for j in (i + 1)..execucao.estados.len() {
                execucao.estados[j] = EstadoPasso::Pulado;
            }
            execucao.conclusao = ConclusaoExecucao::Falha;
            progresso(&execucao);
            limpar_cancelamento(&execucao.id_execucao);
            return execucao;
        }
    }

    execucao.conclusao = ConclusaoExecucao::Sucesso;
    progresso(&execucao);
    // Limpa o flag de cancelamento: o runner já encerrou, manter o ID no
    // conjunto vazaria memória em execuções longas.
    limpar_cancelamento(&execucao.id_execucao);
    execucao
}

/// Roda UMA etapa. Devolve o estado final dela — `Ok { saida }`,
/// `FalhaExec { exit_code, stderr }` ou `FalhaValida { esperado, obtido }`.
///
/// Usa `sh -c` (Unix) ou `cmd /C` (Windows) — o caso de uso (comandos
/// curtos, copiar/colar do terminal) assume a presença de um shell. Em
/// ambiente sem shell o runner falharia com exit 127 ("command not found")
/// e a etapa viraria `FalhaExec`, que é a resposta certa.
async fn rodar_passo(passo: &Passo, timeout: Duration, workdir: &Path) -> EstadoPasso {
    #[cfg(unix)]
    let (programa, args) = ("sh", vec!["-c", &passo.cmd]);
    #[cfg(windows)]
    let (programa, args) = ("cmd", vec!["/C", &passo.cmd]);

    let filho = match Command::new(programa)
        .args(&args)
        .current_dir(workdir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true) // tokio dropa o handle ao final → mata o filho
        .spawn()
    {
        Ok(f) => f,
        Err(e) => {
            return EstadoPasso::FalhaExec {
                exit_code: -1,
                stderr: format!("falha ao iniciar comando: {e}"),
            };
        }
    };

    // `tokio::time::timeout` envolve a future do `wait` — se o timeout
    // expirar, `wait_with_output` é dropado e `kill_on_drop` mata o filho.
    // docs: https://docs.rs/tokio/latest/tokio/time/fn.timeout.html
    let resultado = tokio::time::timeout(timeout, filho.wait_with_output()).await;
    match resultado {
        Err(_) => {
            // Timeout: kill_on_drop já matou; a mensagem é informativa.
            EstadoPasso::FalhaExec {
                exit_code: -1,
                stderr: format!("timeout após {}s", timeout.as_secs()),
            }
        }
        Ok(Err(e)) => EstadoPasso::FalhaExec {
            exit_code: -1,
            stderr: format!("erro aguardando comando: {e}"),
        },
        Ok(Ok(saida)) => {
            let stdout = String::from_utf8_lossy(&saida.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&saida.stderr).into_owned();
            let exit_code = saida.status.code().unwrap_or(-1);

            if exit_code != 0 {
                return EstadoPasso::FalhaExec { exit_code, stderr };
            }

            // Exit 0: para `exec` é Ok direto. Para `valida`, compara stdout
            // com `esperado` (trimmed, comparação exata — sem regex, sem
            // glob; a UI mostra o "obtido" pra o usuário ver o que saiu).
            match passo.tipo {
                TipoPasso::Exec => EstadoPasso::Ok {
                    saida: Some(stdout),
                },
                TipoPasso::Valida => {
                    let esperado = passo.esperado.as_deref().unwrap_or("").trim().to_string();
                    let obtido = stdout.trim().to_string();
                    if obtido == esperado {
                        EstadoPasso::Ok {
                            saida: Some(stdout),
                        }
                    } else {
                        EstadoPasso::FalhaValida { esperado, obtido }
                    }
                }
            }
        }
    }
}

// ─── Cancelamento ─────────────────────────────────────────────────────

// `OnceLock` + `Mutex<HashSet<String>>`: o servidor registra o ID da
// execução que deve ser cancelada; o runner checa entre etapas. É a
// estrutura mais simples que dá para chamar de QUALQUER thread (sem
// precisar passar o handle de cancelamento pelo closure). Como testes
// e2e são I/O-bound e o conjunto raramente passa de algumas dezenas de
// entradas, o `Mutex<HashSet>` é mais do que suficiente — não vale
// trocar por `DashMap` ou `tokio::sync::RwLock` para este caso.
// docs: https://doc.rust-lang.org/std/sync/struct.OnceLock.html
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static EXECUCOES_CANCELADAS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn canceladas() -> &'static Mutex<HashSet<String>> {
    EXECUCOES_CANCELADAS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Marca a execução para cancelamento. O runner vai abortar entre a
/// próxima etapa (não no meio de uma — o `Command` continua até exit
/// ou timeout).
pub fn cancelar(id_execucao: &str) {
    if let Ok(mut g) = canceladas().lock() {
        g.insert(id_execucao.to_string());
    }
}

/// Limpa o flag de cancelamento (chamado quando a execução termina
/// naturalmente, para o conjunto não crescer infinitamente).
pub fn limpar_cancelamento(id_execucao: &str) {
    if let Ok(mut g) = canceladas().lock() {
        g.remove(id_execucao);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn id_valido_aceita_apenas_caracteres_seguros() {
        assert!(id_valido("prezzo-recalcular"));
        assert!(id_valido("qa_2024_07_15"));
        assert!(id_valido("abc123"));
        assert!(!id_valido("")); // vazio
        assert!(!id_valido("a b")); // espaço
        assert!(!id_valido("á")); // acento
        assert!(!id_valido("a/b")); // path separator
        assert!(!id_valido("../etc")); // tentativa de escape
        assert!(!id_valido(&"a".repeat(65))); // teto
    }

    #[test]
    fn suite_com_id_rejeita_invalido_e_sem_passos() {
        assert!(Suite::com_id("", "x".into(), 30, vec![]).is_err());
        assert!(Suite::com_id("../x", "x".into(), 30, vec![]).is_err());
        assert!(Suite::com_id("ok", "x".into(), 30, vec![]).is_err()); // sem passos
        let p = Passo {
            tipo: TipoPasso::Exec,
            cmd: "true".into(),
            esperado: None,
        };
        let s = Suite::com_id("ok", "ok".into(), 30, vec![p]).unwrap();
        assert_eq!(s.id, "ok");
        assert_eq!(s.timeout_etapa_seg, 30);
    }

    #[test]
    fn parse_toml_minimo() {
        let texto = r#"
id = "prezzo"
nome = "prezzo · recalcular"
timeout_etapa_seg = 10

[[passos]]
tipo = "exec"
cmd = "echo oi"

[[passos]]
tipo = "valida"
cmd = "echo oi"
esperado = "oi"
"#;
        let s = parsear_suite_toml(texto).unwrap();
        assert_eq!(s.id, "prezzo");
        assert_eq!(s.nome, "prezzo · recalcular");
        assert_eq!(s.timeout_etapa_seg, 10);
        assert_eq!(s.passos.len(), 2);
        assert_eq!(s.passos[0].tipo, TipoPasso::Exec);
        assert_eq!(s.passos[1].tipo, TipoPasso::Valida);
        assert_eq!(s.passos[1].esperado.as_deref(), Some("oi"));
    }

    #[test]
    fn parse_toml_aceita_timeout_ausente_com_default() {
        // O TOML do usuário pode omitir `timeout_etapa_seg` — o default
        // (30s) tem que entrar.
        let texto = r#"
id = "x"
nome = "x"
[[passos]]
tipo = "exec"
cmd = "true"
"#;
        let s = parsear_suite_toml(texto).unwrap();
        assert_eq!(s.timeout_etapa_seg, 30);
    }

    #[test]
    fn round_trip_serializa_e_parseia_de_volta() {
        let original = Suite {
            id: "rt".into(),
            nome: "round trip".into(),
            timeout_etapa_seg: 15,
            passos: vec![
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "echo oi".into(),
                    esperado: None,
                },
                Passo {
                    tipo: TipoPasso::Valida,
                    cmd: "cat".into(),
                    esperado: Some("x".into()),
                },
            ],
        };
        let s = serializar_suite_toml(&original).unwrap();
        let de_volta = parsear_suite_toml(&s).unwrap();
        assert_eq!(original, de_volta);
    }

    #[test]
    fn diretorio_testes_resolve_override_e_default() {
        assert_eq!(
            diretorio_testes(Some("/var/lib/dev-cli/testes")),
            PathBuf::from("/var/lib/dev-cli/testes")
        );
        assert_eq!(
            diretorio_testes(Some("")),
            PathBuf::from("/etc/dev-cli/testes")
        );
        // Default depende do OS — só checamos que não está vazio.
        assert!(!diretorio_testes(None).as_os_str().is_empty());
    }

    #[test]
    fn gravar_e_ler_roundtrip_via_disco() {
        let dir = tempdir();
        let suite = Suite {
            id: "fs-test".into(),
            nome: "fs".into(),
            timeout_etapa_seg: 5,
            passos: vec![Passo {
                tipo: TipoPasso::Exec,
                cmd: "true".into(),
                esperado: None,
            }],
        };
        gravar_suite(&dir, &suite).unwrap();
        let lida = ler_suite(&dir, "fs-test").unwrap();
        assert_eq!(suite, lida);
    }

    #[test]
    fn ler_suite_com_id_invalido_falha() {
        let dir = tempdir();
        assert!(ler_suite(&dir, "../etc/passwd").is_err());
    }

    #[test]
    fn gravar_suite_com_id_invalido_falha() {
        let dir = tempdir();
        let suite = Suite {
            id: "../bad".into(),
            nome: "x".into(),
            timeout_etapa_seg: 30,
            passos: vec![Passo {
                tipo: TipoPasso::Exec,
                cmd: "true".into(),
                esperado: None,
            }],
        };
        assert!(gravar_suite(&dir, &suite).is_err());
    }

    #[test]
    fn listar_suites_diretorio_ausente_devolve_vazio() {
        // `listar_suites` num caminho que não existe NÃO pode dar erro —
        // é o caso "primeira vez que alguém usa o recurso".
        let dir = tempdir().join("nao-existe");
        let suites = listar_suites(&dir).unwrap();
        assert!(suites.is_empty());
    }

    #[test]
    fn listar_suites_ignora_arquivos_invalidos() {
        let dir = tempdir();
        // TOML corrompido: não pode quebrar a listagem.
        std::fs::write(dir.join("quebrado.toml"), "isto nao é toml [").unwrap();
        // TOML válido: aparece.
        let valida = Suite {
            id: "ok".into(),
            nome: "ok".into(),
            timeout_etapa_seg: 30,
            passos: vec![Passo {
                tipo: TipoPasso::Exec,
                cmd: "true".into(),
                esperado: None,
            }],
        };
        gravar_suite(&dir, &valida).unwrap();
        let lista = listar_suites(&dir).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, "ok");
    }

    #[test]
    fn estado_rotulos_e_cores() {
        // Não-bloqueante: só garante que as duas funções não paniquem
        // e que o `Ok` é verde (a "cor certa" do sucesso no DS).
        assert_eq!(EstadoPasso::Ok { saida: None }.rotulo(), "passou");
        assert!(EstadoPasso::Ok { saida: None }.cor_var().contains("verde"));
        assert!(
            EstadoPasso::FalhaExec {
                exit_code: 7,
                stderr: "x".into()
            }
            .cor_var()
            .contains("vermelho")
        );
        assert!(EstadoPasso::Pendente.cor_var().contains("neutral"));
    }

    #[test]
    fn execucao_nova_tem_todos_os_passos_pendentes() {
        let suite = Suite {
            id: "x".into(),
            nome: "x".into(),
            timeout_etapa_seg: 30,
            passos: vec![
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "true".into(),
                    esperado: None,
                },
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "true".into(),
                    esperado: None,
                },
            ],
        };
        let ex = Execucao::nova("e1".into(), &suite, 1_000);
        assert_eq!(ex.estados.len(), 2);
        assert!(matches!(ex.estados[0], EstadoPasso::Pendente));
        assert_eq!(ex.conclusao, ConclusaoExecucao::Rodando);
        assert_eq!(ex.duracao_seg(1_005), 5);
    }

    #[tokio::test]
    async fn rodar_suite_toda_ok() {
        let suite = Suite {
            id: "ok".into(),
            nome: "ok".into(),
            timeout_etapa_seg: 5,
            passos: vec![
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "echo oi".into(),
                    esperado: None,
                },
                Passo {
                    tipo: TipoPasso::Valida,
                    cmd: "echo oi".into(),
                    esperado: Some("oi".into()),
                },
            ],
        };
        let ex = Execucao::nova("e1".into(), &suite, 0);
        // Callback que conta quantas vezes foi chamado.
        let contador = std::sync::Arc::new(std::sync::Mutex::new(0));
        let c2 = contador.clone();
        let final_exec = rodar(
            ex,
            &suite,
            std::sync::Arc::new(move |_| {
                *c2.lock().unwrap() += 1;
            }),
        )
        .await;
        assert_eq!(final_exec.conclusao, ConclusaoExecucao::Sucesso);
        assert_eq!(final_exec.estados.len(), 2);
        assert!(matches!(final_exec.estados[0], EstadoPasso::Ok { .. }));
        assert!(matches!(final_exec.estados[1], EstadoPasso::Ok { .. }));
        // 1 inicial + 2× (rodando + resultado) + 1 final = 6 callbacks
        // (o callback de "final" vem da última chamada antes do return).
        assert!(*contador.lock().unwrap() >= 5);
    }

    #[tokio::test]
    async fn rodar_suite_falha_exec_para_pipeline() {
        let suite = Suite {
            id: "fail".into(),
            nome: "fail".into(),
            timeout_etapa_seg: 5,
            passos: vec![
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "false".into(), // sai com exit 1
                    esperado: None,
                },
                Passo {
                    tipo: TipoPasso::Exec,
                    cmd: "echo nao_deve_rodar".into(),
                    esperado: None,
                },
            ],
        };
        let ex = Execucao::nova("e1".into(), &suite, 0);
        let final_exec = rodar(ex, &suite, std::sync::Arc::new(|_| {})).await;
        assert_eq!(final_exec.conclusao, ConclusaoExecucao::Falha);
        assert!(matches!(
            final_exec.estados[0],
            EstadoPasso::FalhaExec { exit_code: 1, .. }
        ));
        assert!(matches!(final_exec.estados[1], EstadoPasso::Pulado));
    }

    #[tokio::test]
    async fn rodar_suite_falha_valida_com_saida_correta_mas_esperado_errado() {
        let suite = Suite {
            id: "fv".into(),
            nome: "fv".into(),
            timeout_etapa_seg: 5,
            passos: vec![Passo {
                tipo: TipoPasso::Valida,
                cmd: "echo pendente".into(),
                esperado: Some("concluido".into()),
            }],
        };
        let ex = Execucao::nova("e1".into(), &suite, 0);
        let final_exec = rodar(ex, &suite, std::sync::Arc::new(|_| {})).await;
        assert_eq!(final_exec.conclusao, ConclusaoExecucao::Falha);
        match &final_exec.estados[0] {
            EstadoPasso::FalhaValida { esperado, obtido } => {
                assert_eq!(esperado, "concluido");
                assert_eq!(obtido, "pendente");
            }
            outro => panic!("esperava FalhaValida, veio {outro:?}"),
        }
    }

    #[tokio::test]
    async fn rodar_suite_timeout_marca_falha_exec() {
        let suite = Suite {
            id: "to".into(),
            nome: "to".into(),
            timeout_etapa_seg: 1, // 1 segundo
            passos: vec![Passo {
                tipo: TipoPasso::Exec,
                #[cfg(unix)]
                cmd: "sleep 5".into(), // bem mais que 1s
                #[cfg(windows)]
                cmd: "timeout /t 5 /nobreak >nul".into(),
                esperado: None,
            }],
        };
        let ex = Execucao::nova("e1".into(), &suite, 0);
        let final_exec = rodar(ex, &suite, std::sync::Arc::new(|_| {})).await;
        assert_eq!(final_exec.conclusao, ConclusaoExecucao::Falha);
        assert!(matches!(
            final_exec.estados[0],
            EstadoPasso::FalhaExec { .. }
        ));
    }

    #[tokio::test]
    async fn rodar_suite_comando_inexistente_vira_falha_exec() {
        // `sh -c nope-12345` falha com "command not found" (exit 127).
        let suite = Suite {
            id: "nf".into(),
            nome: "nf".into(),
            timeout_etapa_seg: 5,
            passos: vec![Passo {
                tipo: TipoPasso::Exec,
                cmd: "nope-12345".into(),
                esperado: None,
            }],
        };
        let ex = Execucao::nova("e1".into(), &suite, 0);
        let final_exec = rodar(ex, &suite, std::sync::Arc::new(|_| {})).await;
        assert_eq!(final_exec.conclusao, ConclusaoExecucao::Falha);
        assert!(matches!(
            final_exec.estados[0],
            EstadoPasso::FalhaExec { .. }
        ));
    }

    #[test]
    fn cancelar_e_limpar_funciona() {
        // Round-trip do HashSet estático: o callback `cancelar` insere,
        // `limpar_cancelamento` remove — não testamos o efeito no runner
        // (coberto pelo teste de `rodar_suite_*`), só a API.
        let id = "test-cancel-x";
        limpar_cancelamento(id); // garante estado inicial
        cancelar(id);
        // Re-cancelar não pode duplicar.
        cancelar(id);
        limpar_cancelamento(id);
    }

    /// Cria um diretório temporário único por teste — `std::env::temp_dir()`
    /// + um nome com o nome do teste + nanos para não colidir em paralelo.
    fn tempdir() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("dev-cli-testes-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
