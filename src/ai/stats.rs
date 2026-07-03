// Subcomando `ai stats`: sem argumento, mostra um dashboard combinado de
// todos os provedores (OpenCode + Claude Code); com um provedor explícito
// (`ai stats opencode` / `ai stats claude`), encaminha só para aquele —
// mesmo comportamento de hoje, sem mudança.
use chrono::Local;
use clap::Args;
use clap::Subcommand;

use crate::ai::claude;
use crate::ai::claude::ClaudeArgs;
use crate::ai::opencode;
use crate::ai::opencode::OpencodeArgs;
use crate::ai::render;

/// Estatísticas de uso de IA: todos os provedores combinados (padrão) ou
/// um provedor específico (`opencode`/`claude`).
#[derive(Args, Debug)]
#[command(
    help_template = crate::help::ARGUMENTOS_SUBCOMANDOS,
    next_help_heading = crate::help::OPCOES
)]
pub struct StatsArgs {
    #[command(subcommand)]
    comando: Option<StatsCommands>,

    /// Período: mês (YYYY-MM) ou dia (YYYY-MM-DD). Se omitido, usa o mês
    /// atual. Só usado quando nenhum provedor é passado (combinado).
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    periodo: Option<String>,

    /// Mostra todo o histórico (ignora o filtro de período).
    #[arg(long, conflicts_with = "periodo", help_heading = crate::help::OPCOES)]
    historico: bool,

    /// Largura do heatmap em semanas (4-104, padrão 52).
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104), help_heading = crate::help::OPCOES)]
    weeks: u32,

    /// Quantos dias mostrar no ranking dos mais intensos (padrão 5).
    #[arg(long, short, default_value_t = 5, help_heading = crate::help::OPCOES)]
    top: usize,

    /// Desativa cores ANSI (útil para pipes/arquivos).
    #[arg(long, help_heading = crate::help::OPCOES)]
    no_color: bool,

    /// Em vez do dashboard, imprime JSON com os dados brutos.
    #[arg(long, help_heading = crate::help::OPCOES)]
    json: bool,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum. Cada variante carrega um tipo de
        // args diferente (`OpencodeArgs` vs `ClaudeArgs`). Sem subcomando
        // (`None`), roda o dashboard combinado.
        match &self.comando {
            Some(StatsCommands::Opencode(args)) => args.execute(),
            Some(StatsCommands::Claude(args)) => args.execute(),
            None => self.execute_combinado(),
        }
    }

    // ── execute_combinado ────────────────────────────────────────────
    // Carrega os dois provedores e mescla num só dashboard. Um provedor
    // sem dados no período (banco do OpenCode ausente, ou nenhuma sessão
    // Claude naquele mês) é pulado silenciosamente — o comando só falha
    // se os dois estiverem vazios, igual ao "nenhuma sessão encontrada"
    // que cada subcomando individual já retorna hoje.
    fn execute_combinado(&self) -> Result<String, Box<dyn std::error::Error>> {
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        let dados_claude = claude::carregar_dados(&periodo);
        let claude_vazio =
            dados_claude.sessoes.is_empty() && dados_claude.tokens_por_dia.is_empty();
        let resultado_opencode = opencode::carregar_dados(&periodo);

        // Cada provedor vira `None` (e entra em `pulados`) se não tiver
        // dados no período ou se a carga falhou (banco ausente, etc.).
        let mut pulados: Vec<&str> = Vec::new();
        let claude_opt = if claude_vazio {
            pulados.push("Claude Code");
            None
        } else {
            Some(dados_claude)
        };
        let opencode_opt = match resultado_opencode {
            Ok(dados) if !(dados.sessoes.is_empty() && dados.tokens_por_dia.is_empty()) => {
                Some(dados)
            }
            _ => {
                pulados.push("OpenCode");
                None
            }
        };

        let dados = match (claude_opt, opencode_opt) {
            (None, None) => return Ok(format!("Nenhuma sessão encontrada para {periodo}")),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => render::mesclar_dados(a, b),
        };

        let por_dia = render::agregar_por_dia(&dados.sessoes);
        let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
        let nota_pulados = if pulados.is_empty() {
            String::new()
        } else {
            format!(" (sem dados do {})", pulados.join(" e do "))
        };
        let subtitulo = if self.historico {
            format!(
                "{:.1}h totais em {} sessões{}",
                total_horas,
                dados.sessoes.len(),
                nota_pulados
            )
        } else {
            format!("{periodo}{nota_pulados}")
        };

        if self.json {
            #[derive(serde::Serialize)]
            struct LinhaDia {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                provedores_pulados: Vec<String>,
                total_horas: f64,
                dias: Vec<LinhaDia>,
                custo_usd_total: f64,
                modelos: Vec<render::ModeloUso>,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
            }
            let saida_json = Saida {
                historico: self.historico,
                mes: periodo.clone(),
                provedores_pulados: pulados.iter().map(|p| p.to_string()).collect(),
                total_horas,
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
                custo_usd_total: dados.custo_total,
                modelos: dados.modelos,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        Ok(render::renderizar_dashboard(
            "IA atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            Some(self.top),
        ))
    }
}

// Enum dos provedores de IA para o subcomando `ai stats`. Cada variante
// carrega seu próprio `*Args` (e portanto seus campos e defaults).
/// Provedores de IA disponíveis para estatísticas.
#[derive(Subcommand, Debug)]
enum StatsCommands {
    /// Estatísticas do OpenCode (tokens, custo, heatmap).
    Opencode(OpencodeArgs),
    /// Estatísticas do Claude Code (horas, custo, heatmap).
    Claude(ClaudeArgs),
}
