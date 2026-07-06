// CASCA DE IO: subcomando `logs stats`.
// Lê arquivos supervisord.log do disco, delega o cálculo ao núcleo puro
// e delega a formatação ao módulo de render.

use std::path::PathBuf;

use clap::Args;

use crate::logs::core::contar;
use crate::logs::render::renderizar;

#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct StatsArgs {
    /// Container específico; se omitido, varre todos em --path.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Caminho do diretório com os logs dos containers.
    #[arg(long, default_value = "dados/logs", help_heading = crate::help::OPCOES)]
    path: PathBuf,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let alvos = self.descobrir_alvos()?;
        let mut saida = String::new();
        for (nome, caminho) in alvos {
            let conteudo = std::fs::read_to_string(&caminho)
                .map_err(|erro| format!("não foi possível ler '{}': {erro}", caminho.display()))?;
            let contagens = contar(&conteudo);
            saida.push_str(&renderizar(&nome, &contagens));
        }
        Ok(saida.trim_end().to_string())
    }

    fn descobrir_alvos(&self) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
        if let Some(container) = &self.container {
            let caminho = self.path.join(container).join("supervisord.log");
            if !caminho.exists() {
                return Err(format!(
                    "arquivo de log não encontrado para o container '{container}': '{}'",
                    caminho.display()
                )
                .into());
            }
            return Ok(vec![(container.clone(), caminho)]);
        }

        let entradas = std::fs::read_dir(&self.path).map_err(|erro| {
            format!(
                "não foi possível ler o diretório '{}': {erro}",
                self.path.display()
            )
        })?;

        let mut nomes = Vec::new();
        for entrada in entradas {
            let entrada = entrada?;
            if entrada.file_type()?.is_dir()
                && let Some(nome) = entrada.file_name().to_str()
            {
                nomes.push(nome.to_string());
            }
        }
        nomes.sort();

        Ok(nomes
            .into_iter()
            .map(|nome| {
                let caminho = self.path.join(&nome).join("supervisord.log");
                (nome, caminho)
            })
            .collect())
    }
}
