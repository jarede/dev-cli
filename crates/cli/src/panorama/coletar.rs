// CASCA DE IO: subcomando `panorama coletar` — roda o orquestrador do nucleo
// (sistema + docker + proxy + GitLab, por host), grava o snapshot JSON com
// gravação atômica e aplica a retenção configurada.

use std::path::PathBuf;

use clap::Args;

use nucleo::panorama::orquestrador;

/// Coleta o inventário de todos os hosts e grava o snapshot JSON.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ColetarArgs {
    /// Caminho do arquivo de configuração TOML.
    /// (default: ~/.config/dev-cli/config.toml, se existir)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Diretório de saída dos snapshots (sobrepõe config/env).
    /// Útil para testar sem tocar no diretório de produção.
    #[arg(long)]
    saida: Option<PathBuf>,
}

impl ColetarArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // 1. Config com precedência flags > env > arquivo > defaults.
        let mut config = super::carregar_config(self.config.as_deref())?;
        if let Some(saida) = &self.saida {
            config.panorama.diretorio_snapshots = saida.display().to_string();
        }
        let diretorio = PathBuf::from(&config.panorama.diretorio_snapshots);

        // 2. Coleta (orquestrador do nucleo). `dados: None` significa que
        // NENHUM coletor funcionou — é falha; parcial com avisos é sucesso.
        // docs: https://doc.rust-lang.org/std/result/enum.Result.html
        let resultado = orquestrador::coletar(&config);
        let snapshot = match resultado.dados {
            Some(snapshot) => snapshot,
            // Nada foi coagido: devolve o primeiro erro para o usuário saber
            // o motivo (em vez de um vago "falhou").
            None => {
                let mensagem = resultado
                    .erros
                    .first()
                    .map(|e| e.mensagem.as_str())
                    .unwrap_or("nenhum coletor produziu dados");
                return Err(mensagem.into());
            }
        };

        // 3. Grava (atômico) e aplica retenção.
        let (caminho, removidos) =
            super::gravar_com_retencao(&snapshot, &diretorio, config.panorama.retencao_dias)?;

        // 4. Resumo amigável (e os avisos, parte do contrato).
        Ok(super::resumo(&snapshot, &caminho, &removidos))
    }
}
