// CASCA DE IO (com miolo puro testável): executa comandos `docker` no host
// local — o modo padrão, já que os binários rodam NA VM que tem o docker —
// ou através de SSH (modo de desenvolvimento, para consultar uma VM remota
// sem instalar nada nela).
//
// A separação chave: `montar_comando` é PURO (decide programa + argumentos,
// testável sem docker/ssh instalados); `executar` é a casca que de fato
// dispara o processo e captura a saída.

use std::process::Command;

/// Estratégia de execução dos comandos docker.
// Um enum com dados ("Ssh" carrega o host) é a forma idiomática em Rust de
// modelar "uma escolha entre alternativas que carregam informação própria" —
// o `match` obriga a tratar todas.
// docs: https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html
#[derive(Debug, Clone, PartialEq)]
pub enum Executor {
    /// Executa `docker ...` diretamente (requer usuário no grupo docker).
    Local,
    /// Executa `ssh <host> "docker ..."` (host no formato "user@host").
    Ssh(String),
}

impl Executor {
    /// NÚCLEO PURO: monta (programa, argumentos) sem executar nada.
    /// No modo SSH os argumentos docker viram UMA string (o shell remoto
    /// re-divide), por isso o `join(" ")`.
    pub fn montar_comando(&self, args_docker: &[&str]) -> (String, Vec<String>) {
        match self {
            Executor::Local => (
                "docker".to_string(),
                args_docker.iter().map(|s| s.to_string()).collect(),
            ),
            Executor::Ssh(host) => (
                "ssh".to_string(),
                vec![host.clone(), format!("docker {}", args_docker.join(" "))],
            ),
        }
    }

    /// CASCA DE IO: executa e devolve stdout+stderr combinados.
    /// Por que combinar? `docker logs` manda para o stderr o que o processo
    /// do container escreveu em stderr — e loggers como o Loguru escrevem
    /// justamente lá. Se o comando FALHOU (exit != 0), o stderr vira mensagem
    /// de erro em vez de dado.
    // docs: https://doc.rust-lang.org/std/process/struct.Command.html
    pub fn executar(&self, args_docker: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let (programa, args) = self.montar_comando(args_docker);
        let saida = Command::new(&programa)
            .args(&args)
            .output()
            .map_err(|erro| format!("falha ao executar {programa}: {erro}"))?;

        if !saida.status.success() {
            return Err(format!(
                "`{programa} {}` terminou com erro: {}",
                args.join(" "),
                String::from_utf8_lossy(&saida.stderr)
            )
            .into());
        }

        // `from_utf8_lossy` troca bytes inválidos por U+FFFD em vez de
        // falhar — logs de container nem sempre são UTF-8 perfeito.
        // docs: https://doc.rust-lang.org/std/string/struct.String.html#method.from_utf8_lossy
        let mut texto = String::from_utf8_lossy(&saida.stdout).to_string();
        texto.push_str(&String::from_utf8_lossy(&saida.stderr));
        Ok(texto)
    }
}

/// Metadados de um container obtidos via `docker ps`.
#[derive(Debug, Clone)]
pub struct ContainerDocker {
    pub nome: String,
    /// Status textual: "Up 2 days", "Exited (0) 3 days ago", etc.
    pub status: String,
    /// Timestamp de criação: "2026-07-04 12:00:00 +0000 UTC".
    pub criado_em: String,
}

/// Lista os containers rodando (nome|status|criado_em, um por linha).
pub fn listar_containers(
    executor: &Executor,
) -> Result<Vec<ContainerDocker>, Box<dyn std::error::Error>> {
    let saida =
        executor.executar(&["ps", "--format", "'{{.Names}}|{{.Status}}|{{.CreatedAt}}'"])?;
    Ok(parsear_ps(&saida))
}

/// NÚCLEO PURO: converte a saída do `docker ps --format` em structs.
/// Aceita as linhas com ou sem as aspas simples que o format acima gera.
fn parsear_ps(saida: &str) -> Vec<ContainerDocker> {
    saida
        .lines()
        .map(|linha| linha.trim().trim_matches('\''))
        .filter(|linha| !linha.is_empty())
        // `filter_map` + `?` dentro do closure: linhas sem os 3 campos
        // separados por `|` são simplesmente descartadas.
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.filter_map
        .filter_map(|linha| {
            let mut partes = linha.splitn(3, '|');
            Some(ContainerDocker {
                nome: partes.next()?.to_string(),
                status: partes.next()?.to_string(),
                criado_em: partes.next()?.to_string(),
            })
        })
        .collect()
}

/// Busca as últimas `tail` linhas do log (0 = todas).
pub fn obter_logs(
    executor: &Executor,
    nome: &str,
    tail: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let tail_str = tail.to_string();
    if tail > 0 {
        executor.executar(&["logs", "--tail", &tail_str, nome])
    } else {
        executor.executar(&["logs", nome])
    }
}

/// Busca os logs desde um timestamp Unix (coleta incremental).
pub fn obter_logs_desde(
    executor: &Executor,
    nome: &str,
    desde: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let desde_str = desde.to_string();
    executor.executar(&["logs", "--since", &desde_str, nome])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_comando_local() {
        let (prog, args) = Executor::Local.montar_comando(&["logs", "--tail", "10", "meu-app"]);
        assert_eq!(prog, "docker");
        assert_eq!(args, vec!["logs", "--tail", "10", "meu-app"]);
    }

    #[test]
    fn monta_comando_ssh_junta_args_docker() {
        let exec = Executor::Ssh("dev@qa.exemplo.com".to_string());
        let (prog, args) = exec.montar_comando(&["ps", "-a"]);
        assert_eq!(prog, "ssh");
        assert_eq!(args, vec!["dev@qa.exemplo.com", "docker ps -a"]);
    }

    #[test]
    fn parseia_saida_do_ps() {
        let saida = "'web-1|Up 2 days|2026-07-04 12:00:00 +0000 UTC'\n'api-1|Up 5 hours|2026-07-06 08:00:00 +0000 UTC'\n";
        let lista = parsear_ps(saida);
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0].nome, "web-1");
        assert_eq!(lista[0].status, "Up 2 days");
        assert_eq!(lista[1].criado_em, "2026-07-06 08:00:00 +0000 UTC");
    }

    #[test]
    fn parseia_ps_ignora_linhas_vazias_e_malformadas() {
        let lista = parsear_ps("\n\nsem-pipe\n'a|b|c'\n");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].nome, "a");
    }
}
