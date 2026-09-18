// Grupo de subcomandos `panorama`: coleta de inventário de infraestrutura
// (containers, vhosts, repositórios do GitLab e saúde dos hosts) gravada
// como snapshot JSON em disco.
//
// A casca aqui é mínima: carrega a config, chama o orquestrador do nucleo
// (`nucleo::panorama::orquestrador::coletar`), grava o snapshot com a
// gravação atômica do nucleo e aplica a retenção. O cálculo em si vive no
// nucleo; este módulo só liga as pontas e mostra o resultado.

use std::path::PathBuf;

use clap::Args;
use clap::Subcommand;

use nucleo::config::Config;
use nucleo::panorama::gravacao::{aplicar_retencao, gravar_atomico, nome_arquivo};
use nucleo::panorama::snapshot::Snapshot;

use crate::panorama::coletar::ColetarArgs;

mod coletar;

/// Comandos de infraestrutura (panorama).
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct PanoramaArgs {
    #[command(subcommand)]
    comando: PanoramaCommands,
}

impl PanoramaArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match &self.comando {
            PanoramaCommands::Coletar(args) => args.execute(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum PanoramaCommands {
    /// Coleta o inventário de todos os hosts e grava o snapshot JSON.
    Coletar(ColetarArgs),
}

/// Resolve a config (precedência flags > env > arquivo > defaults), valida a
/// parte `panorama` e devolve a config pronta para o orquestrador usar.
fn carregar_config(
    caminho: Option<&std::path::Path>,
) -> Result<Config, Box<dyn std::error::Error>> {
    let config = Config::carregar(caminho)?;
    // Host com `acesso = "ssh"` sem `destino` é erro de config ENGANOSO de
    // coletar: melhor abortar antes de começar do que falhar por host.
    if let Err(mensagem) = config.panorama.validar() {
        return Err(mensagem.into());
    }
    Ok(config)
}

/// Grava o snapshot no diretório da config (nome por hora) e aplica a
/// retenção. Devolve (caminho gravado, arquivos removidos).
fn gravar_com_retencao(
    snapshot: &Snapshot,
    diretorio: &std::path::Path,
    retencao_dias: u64,
) -> Result<(PathBuf, Vec<String>), Box<dyn std::error::Error>> {
    // Sem `coletado_em` válido para nomear o arquivo, não há como gravar —
    // o nome usa o prefixo ISO (granularidade de hora) como identidade.
    let nome = nome_arquivo(&snapshot.coletado_em)
        .ok_or("coletado_em fora do formato esperado (YYYY-MM-DDTHH...)")?;
    let json = serde_json::to_string_pretty(snapshot)?;
    let caminho = gravar_atomico(diretorio, &nome, &json)?;
    // Os erros do snapshot não param aqui: o arquivo a ser gravado é o do
    // contrato e a retenção usa o nome. A faxina vem depois da gravação.
    let removidos = aplicar_retencao(diretorio, retencao_dias, &snapshot.coletado_em)?;
    Ok((caminho, removidos))
}

/// Imprime um resumo legível do snapshot recém-gravado (e os avisos de
/// falha parcial, que são parte do CONTRATO — nunca esconder).
fn resumo(coletado: &Snapshot, caminho: &std::path::Path, removidos: &[String]) -> String {
    let mut saida = String::new();
    saida.push_str(&format!(
        "Snapshot {} gravado em {}\n",
        coletado.versao,
        caminho.display()
    ));
    saida.push_str(&format!(
        "{} hosts, {} containers, {} vhosts, {} repositórios\n",
        coletado.hosts.len(),
        coletado.containers.len(),
        coletado.vhosts.len(),
        coletado.repositorios.len()
    ));
    if !removidos.is_empty() {
        saida.push_str(&format!(
            "Retenção: removidos {} arquivo(s) antigo(s)\n",
            removidos.len()
        ));
    }
    for erro in &coletado.erros {
        saida.push_str(&format!("aviso {}: {}\n", erro.coletor, erro.mensagem));
    }
    saida
}
