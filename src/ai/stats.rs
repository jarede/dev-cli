// Subcomando `ai stats`: sem argumento, mostra um dashboard combinado de
// todos os provedores (OpenCode + Claude Code); com um provedor explícito
// (`ai stats opencode` / `ai stats claude`), encaminha só para aquele —
// mesmo comportamento de hoje, sem mudança.
// `chrono::Local` dá acesso ao relógio/fuso horário local da máquina, usado
// para descobrir "qual é o mês atual" quando o usuário não passa `--periodo`.
use chrono::Local;
// `Args`/`Subcommand`: macros de derive do clap — mesma ideia de `src/logs.rs`:
// geram o parser de linha de comando a partir dos campos/variantes anotados.
use clap::Args;
use clap::Subcommand;

// Módulos irmãos dentro de `crate::ai`: cada provedor tem sua própria lógica
// de carga de dados (`carregar_dados`) e seu próprio `*Args` para quando o
// usuário chama o subcomando específico (`ai stats claude`, `ai stats opencode`).
use crate::ai::claude;
use crate::ai::claude::ClaudeArgs;
use crate::ai::opencode;
use crate::ai::opencode::OpencodeArgs;
// `render`: funções e tipos compartilhados de formatação/mesclagem do
// dashboard (heatmap, tabela de modelos, etc.), reaproveitadas tanto aqui
// quanto nos subcomandos individuais de cada provedor.
use crate::ai::render;

/// Estatísticas de uso de IA: todos os provedores combinados (padrão) ou
/// um provedor específico (`opencode`/`claude`).
#[derive(Args, Debug)]
#[command(
    help_template = crate::help::ARGUMENTOS_SUBCOMANDOS,
    next_help_heading = crate::help::OPCOES
)]
pub struct StatsArgs {
    // `Option<StatsCommands>` (em vez de `StatsCommands` direto, como em
    // `LogsArgs`): aqui o subcomando é OPCIONAL — o clap permite rodar
    // `ai stats` sozinho (cai em `None`, dashboard combinado) ou com um
    // provedor explícito (`Some(...)`).
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
        // `--historico` ignora qualquer período: string vazia é o sinal que
        // `carregar_dados` de cada provedor interpreta como "sem filtro,
        // traga tudo". Sem `--historico`, usamos o período informado
        // (`self.periodo`) ou, na ausência dele, o mês atual formatado como
        // "YYYY-MM" (`unwrap_or_else` só roda o `Local::now()` se `periodo`
        // for `None` — evita calcular a data à toa quando não é necessário).
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        // Carrega os dois provedores incondicionalmente; decidir se algum
        // deles fica de fora acontece depois, olhando o conteúdo carregado.
        let dados_claude = claude::carregar_dados(&periodo);
        // Claude não tem um `Result` de carga (não lê banco externo que possa
        // faltar) — "vazio" aqui é o único jeito de saber que não há dados
        // para o período.
        let claude_vazio =
            dados_claude.sessoes.is_empty() && dados_claude.tokens_por_dia.is_empty();
        // OpenCode lê de um banco SQLite próprio, que pode não existir; por
        // isso a carga devolve `Result`, e tratamos tanto erro quanto "vazio"
        // do mesmo jeito: provedor pulado.
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
        // `match` com guarda (`if !(...)`): só entra no primeiro braço se o
        // resultado for `Ok` E os dados não estiverem vazios; qualquer outra
        // combinação (erro OU `Ok` vazio) cai no `_` e marca como pulado.
        let opencode_opt = match resultado_opencode {
            Ok(dados) if !(dados.sessoes.is_empty() && dados.tokens_por_dia.is_empty()) => {
                Some(dados)
            }
            _ => {
                pulados.push("OpenCode");
                None
            }
        };

        // `match` sobre uma tupla de dois `Option`: cobre as quatro
        // combinações possíveis de presença/ausência de cada provedor.
        // Quando os dois faltam, `return` sai cedo da função com uma
        // mensagem (não há dashboard para montar). Quando só um está
        // presente, ele "é" o resultado combinado. Só quando os dois têm
        // dados é que `mesclar_dados` de fato combina as duas fontes.
        let dados = match (claude_opt, opencode_opt) {
            (None, None) => return Ok(format!("Nenhuma sessão encontrada para {periodo}")),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => render::mesclar_dados(a, b),
        };

        // Agrega as sessões (de qualquer provedor, já mescladas) por dia do
        // calendário, obtendo um mapa dia -> (horas, quantidade de sessões).
        let por_dia = render::agregar_por_dia(&dados.sessoes);
        // `.values()` itera só os valores do mapa (ignora as chaves/dias);
        // `.map(|(h, _)| h)` extrai a componente "horas" de cada tupla,
        // descartando a contagem de sessões; `.sum()` soma tudo em um único
        // `f64`. O tipo de retorno é inferido pela anotação `: f64` na
        // variável.
        let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
        // Texto extra avisando quais provedores ficaram de fora, só quando
        // há algum: `pulados.join(" e do ")` junta "Claude Code" e/ou
        // "OpenCode" com o conectivo, formando algo como "OpenCode e do
        // Claude Code" quando os dois faltam (situação rara, já tratada
        // acima pelo `return` antecipado).
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
            // As três structs abaixo são LOCAIS a este bloco `if` — só
            // existem para dar forma ao JSON de saída, então não fazia
            // sentido declará-las no nível do módulo. `#[derive(serde::Serialize)]`
            // gera automaticamente o código que converte cada struct para
            // JSON, campo a campo, usando o nome do campo Rust como chave.
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
            // Formato final do JSON impresso quando `--json` é passado;
            // espelha os mesmos dados do dashboard textual, só que
            // estruturados para consumo por outro programa.
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
                // `pulados` é `Vec<&str>` (fatias emprestadas de literais
                // `"Claude Code"`/`"OpenCode"`); o JSON final precisa ser
                // dono dos seus dados (`Vec<String>`), então `.to_string()`
                // em cada item faz a cópia dono.
                provedores_pulados: pulados.iter().map(|p| p.to_string()).collect(),
                total_horas,
                // `por_dia` é um mapa dia -> (horas, sessoes); `.iter()`
                // percorre pares (chave, valor) por referência, e o `map`
                // desestrutura a tupla `(horas, sessoes)` direto no
                // parâmetro do closure para montar cada `LinhaDia`.
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        // `*horas`/`*sessoes`: desreferencia a referência
                        // emprestada do mapa para copiar o valor (ambos são
                        // tipos `Copy`: `f64` e `u32`), já que a struct
                        // `LinhaDia` precisa ser dona dos seus campos.
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
                custo_usd_total: dados.custo_total,
                // `dados.modelos`/`dados.sem_preco`: movidos para dentro da
                // struct `Saida` (não são clonados) — `dados` não é mais
                // usado depois deste ponto no bloco `if self.json`.
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
            // `serde_json::to_string_pretty`: serializa a struct para uma
            // String JSON indentada (legível por humanos); `?` propaga o
            // erro se, por algum motivo, a serialização falhar.
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        // Caminho padrão (sem `--json`): monta o dashboard textual/colorido
        // compartilhado com os subcomandos de cada provedor. `!self.no_color`
        // inverte a flag: por padrão colore (`no_color = false` -> `true`
        // aqui), e só desliga quando o usuário pede `--no-color` explicitamente.
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
