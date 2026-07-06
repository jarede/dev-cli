// CASCA DE IO: subcomando `logs containers`.
// Lê logs de containers locais via `container logs`, com suporte a
// `--follow` (modo tempo real com painel redesenhavel).

use std::collections::BTreeMap;
use std::io::BufRead;
use std::io::Write;

use clap::Args;

use crate::logs::core::contar_niveis_container;
use crate::logs::render::renderizar_container;

#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ContainersArgs {
    /// Container específico; se omitido, varre todos os detectados por `container list`.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Acompanha os logs em tempo real (como `tail -f`), redesenhando o painel a cada linha nova.
    #[arg(short = 'f', long, help_heading = crate::help::OPCOES)]
    follow: bool,
}

impl ContainersArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let nomes = if let Some(container) = &self.container {
            vec![container.clone()]
        } else {
            listar_containers()?
        };

        if self.follow {
            seguir_containers(&nomes)?;
            return Ok(String::new());
        }

        let mut saida = String::new();
        for nome in nomes {
            let conteudo = obter_logs(&nome)?;
            let niveis = contar_niveis_container(&conteudo);
            saida.push_str(&renderizar_container(&nome, None, &niveis));
        }

        Ok(saida.trim_end().to_string())
    }
}

/// CASCA DE IO: pergunta ao binário `container` quais containers existem.
fn listar_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("container")
        .args(["list", "-q"])
        .output()
        .map_err(|erro| format!("falha ao executar 'container list': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container list' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .map(str::to_string)
        .collect())
}

/// CASCA DE IO: busca o log completo de um container via `container logs`.
fn obter_logs(nome: &str) -> Result<String, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("container")
        .args(["logs", nome])
        .output()
        .map_err(|erro| format!("falha ao executar 'container logs {nome}': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container logs {nome}' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

/// CASCA DE IO: modo `-f`. Abre um processo `container logs -f <nome>` por
/// container; cada um é lido numa thread própria. As threads só enviam linhas
/// por um canal `mpsc`; quem acumula contagens e redesenha o painel é a thread
/// principal.
fn seguir_containers(nomes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();

    for nome in nomes {
        let mut child = std::process::Command::new("container")
            .args(["logs", "-f", nome])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|erro| format!("falha ao executar 'container logs -f {nome}': {erro}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("não foi possível capturar a saída do container")?;

        let tx = tx.clone();
        let nome_da_thread = nome.clone();
        std::thread::spawn(move || {
            let leitor = std::io::BufReader::new(stdout);
            for linha in leitor.lines().map_while(Result::ok) {
                let _ = tx.send((nome_da_thread.clone(), linha));
            }
            let _ = child.wait();
        });
    }
    drop(tx);

    let mut totais: BTreeMap<String, BTreeMap<String, usize>> = nomes
        .iter()
        .map(|nome| (nome.clone(), BTreeMap::new()))
        .collect();

    for (nome, linha) in rx {
        let niveis_da_linha = contar_niveis_container(&linha);
        let acumulado = totais.entry(nome).or_default();
        for (nivel, quantidade) in niveis_da_linha {
            *acumulado.entry(nivel).or_insert(0) += quantidade;
        }

        print!("\x1b[2J\x1b[H");
        for nome in nomes {
            if let Some(niveis) = totais.get(nome) {
                print!("{}", renderizar_container(nome, None, niveis));
            }
        }
        std::io::stdout().flush()?;
    }

    Ok(())
}
