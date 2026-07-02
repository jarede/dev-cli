// Módulo do subcomando `logs stats`.
//
// A ideia central de arquitetura aqui é separar duas responsabilidades:
//   1. NÚCLEO PURO  -> `fn contar`: recebe texto, devolve contagens. Não toca
//      em disco nem imprime nada. Por ser puro, é 100% testável com strings
//      inline (ver o módulo `tests` no final).
//   2. CASCA DE IO  -> `StatsArgs::execute` e `descobrir_alvos`: descobrem os
//      arquivos, leem do disco e formatam a saída colorida.
// Manter o cálculo separado do efeito é um padrão que facilita testar e raciocinar.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;
use clap::Subcommand;
// Trait de extensão do `owo-colors`: ao importá-la, todo tipo que implementa
// `Display` ganha métodos como `.red()`, `.bold()`, `.dimmed()`.
use owo_colors::OwoColorize;

// Níveis que o supervisord escreve na 3ª coluna de cada linha própria.
// `[&str; 6]` é um array de tamanho fixo conhecido em tempo de compilação.
const NIVEIS_CONHECIDOS: [&str; 6] = ["INFO", "DEBG", "WARN", "CRIT", "ERRO", "TRAC"];
// Palavras que procuramos no TEXTO da linha (logs da app embutidos no stdout).
const PALAVRAS_CHAVE: [&str; 4] = ["error", "warn", "info", "debug"];

/// Comandos de log.
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct LogsArgs {
    // `logs` é um grupo: ele apenas encaminha para um subcomando aninhado.
    #[command(subcommand)]
    comando: LogsCommands,
}

impl LogsArgs {
    // `&self` = empréstimo imutável; só lemos os campos, não os consumimos.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum; hoje só há uma variante, mas o
        // `match` deixa o compilador exigir que novas variantes sejam tratadas.
        match &self.comando {
            LogsCommands::Stats(args) => args.execute(),
        }
    }
}

/// Subcomandos de `logs`.
#[derive(Subcommand, Debug)]
enum LogsCommands {
    /// Estatísticas de logs de containers.
    Stats(StatsArgs),
}

/// Estatísticas de logs de um container específico, ou de todos.
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
        // Descobre quais arquivos ler. O `?` propaga o erro para cima se falhar.
        let alvos = self.descobrir_alvos()?;
        // Acumulador da saída. `mut` porque vamos anexar texto a cada iteração.
        let mut saida = String::new();
        // `alvos` é um Vec de tuplas (nome, caminho); o `for` consome cada uma.
        for (nome, caminho) in alvos {
            // Lê o arquivo inteiro para uma String. `map_err` troca o erro cru
            // de IO por uma mensagem clara com o caminho antes do `?` propagar.
            let conteudo = std::fs::read_to_string(&caminho)
                .map_err(|erro| format!("não foi possível ler '{}': {erro}", caminho.display()))?;
            // NÚCLEO PURO: transforma texto em contagens.
            let contagens = contar(&conteudo);
            // Formata o bloco colorido deste container e anexa ao acumulador.
            saida.push_str(&renderizar(&nome, &contagens));
        }
        // `trim_end` remove a quebra de linha final; devolvemos a String pronta
        // (quem imprime é o `main.rs`).
        Ok(saida.trim_end().to_string())
    }

    // Resolve a lista de arquivos a processar: um único container ou todos.
    fn descobrir_alvos(&self) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
        // `if let Some(...)` desestrutura o Option: só entra se o usuário passou container.
        if let Some(container) = &self.container {
            // `join` monta o caminho `<path>/<container>/supervisord.log`.
            let caminho = self.path.join(container).join("supervisord.log");
            if !caminho.exists() {
                // `.into()` converte a String para o `Box<dyn Error>` do retorno.
                return Err(format!(
                    "arquivo de log não encontrado para o container '{container}': '{}'",
                    caminho.display()
                )
                .into());
            }
            // `vec![...]` cria um Vec com um único elemento.
            return Ok(vec![(container.clone(), caminho)]);
        }

        // Sem container: listamos as entradas do diretório `--path`.
        let entradas = std::fs::read_dir(&self.path).map_err(|erro| {
            format!(
                "não foi possível ler o diretório '{}': {erro}",
                self.path.display()
            )
        })?;

        let mut nomes = Vec::new();
        // `read_dir` produz um iterador de `Result<DirEntry>`: cada leitura pode falhar.
        for entrada in entradas {
            let entrada = entrada?;
            // Só nos interessam subdiretórios (um por container). A "let chain"
            // com `&& let` encadeia duas condições num único `if`: só empurra o
            // nome se for diretório E o nome for UTF-8 válido (`to_str()` devolve
            // `Option<&str>`, `None` se não for).
            if entrada.file_type()?.is_dir()
                && let Some(nome) = entrada.file_name().to_str()
            {
                nomes.push(nome.to_string());
            }
        }
        // A ordem do `read_dir` não é garantida; ordenamos para saída estável.
        nomes.sort();

        // Transforma cada nome em uma tupla (nome, caminho do log).
        // `into_iter` consome o Vec; `map` adapta cada item; `collect` remonta um Vec.
        Ok(nomes
            .into_iter()
            .map(|nome| {
                let caminho = self.path.join(&nome).join("supervisord.log");
                (nome, caminho)
            })
            .collect())
    }
}

// Resultado do núcleo puro: dois mapas de "rótulo -> quantidade".
// `BTreeMap` mantém as chaves ordenadas, o que dá saída determinística.
// `#[derive(Default)]` dá um construtor `Contagens::default()` com mapas vazios;
// `PartialEq` permite comparar com `assert_eq!` nos testes.
#[derive(Default, Debug, PartialEq)]
struct Contagens {
    niveis: BTreeMap<String, usize>,   // coluna de nível do supervisord
    palavras: BTreeMap<String, usize>, // palavras-chave encontradas no texto
}

// NÚCLEO PURO: uma passada por todas as linhas, sem nenhum efeito colateral.
fn contar(conteudo: &str) -> Contagens {
    let mut contagens = Contagens::default();

    // `lines()` itera linha a linha sem alocar cópias (empresta fatias do texto).
    for linha in conteudo.lines() {
        // Coluna supervisord: `split_whitespace()` quebra por espaços e `nth(2)`
        // pega o 3º token (índice 2). Retorna `Option`: `None` se não existir.
        // A "let chain" (`&& ...`) só entra no bloco se o token existir E for um
        // nível conhecido (`contains` sobre o array).
        if let Some(token) = linha.split_whitespace().nth(2)
            && NIVEIS_CONHECIDOS.contains(&token)
        {
            // API `entry`: pega o contador da chave (inserindo 0 se ausente)
            // e o `*... += 1` incrementa o valor no lugar.
            *contagens.niveis.entry(token.to_string()).or_insert(0) += 1;
        }

        // Busca de palavras-chave case-insensitive: comparamos tudo em minúsculo.
        let linha_minuscula = linha.to_lowercase();
        for palavra in PALAVRAS_CHAVE {
            // Conta 1 por LINHA que contém a palavra (não por ocorrência).
            if linha_minuscula.contains(palavra) {
                *contagens.palavras.entry(palavra.to_string()).or_insert(0) += 1;
            }
        }
    }

    contagens
}

// Escolhe a cor conforme a severidade e devolve o campo já formatado.
fn colorir_nivel(nivel: &str, contagem: usize) -> String {
    // O número em negrito; `{}` usa o `Display` que o `.bold()` produz.
    let texto = format!("{nivel} {}", contagem.bold());
    // Comparamos em maiúsculo para casar tanto "ERRO"/"error" quanto "INFO"/"info".
    match nivel.to_uppercase().as_str() {
        "ERROR" | "ERRO" | "CRIT" => texto.red().to_string(),
        "WARN" | "WARNING" => texto.yellow().to_string(),
        "INFO" => texto.green().to_string(),
        "DEBUG" | "DEBG" => texto.dimmed().to_string(),
        "TRAC" | "TRACE" => texto.cyan().to_string(),
        // `_` é o caso-curinga: qualquer outro rótulo fica sem cor.
        _ => texto,
    }
}

// Monta o bloco de texto (colorido) de um container.
fn renderizar(nome: &str, contagens: &Contagens) -> String {
    let mut saida = format!("📦 {}\n", nome.bold());

    // Só imprime a linha "supervisord" se houver ao menos uma contagem > 0.
    // `any` para na primeira que satisfaz a condição.
    if contagens.niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   supervisord   ");
        // Coleta cada nível não-zero já colorido numa lista de Strings.
        // O padrão `|&(_, &contagem)|` desestrutura a referência da tupla:
        // `_` ignora a chave e `&contagem` copia o número emprestado.
        let campos: Vec<String> = contagens
            .niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        // `join` intercala os campos com espaços, sem separador sobrando na ponta.
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    // Mesma lógica para as palavras-chave encontradas no texto.
    if contagens.palavras.values().any(|&valor| valor > 0) {
        saida.push_str("   texto         ");
        let campos: Vec<String> = contagens
            .palavras
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(palavra, &contagem)| colorir_nivel(palavra, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    saida
}

// `#[cfg(test)]`: este módulo só é compilado ao rodar `cargo test`.
#[cfg(test)]
mod tests {
    // Traz para o escopo tudo do módulo pai (incluindo `contar` e `Contagens`).
    use super::*;

    #[test]
    fn conta_niveis_supervisord_e_ignora_continuacoes() {
        // `"\` no fim da linha junta as linhas seguintes sem espaço extra.
        let conteudo = "\
2026-05-08 14:29:39,438 INFO mensagem qualquer
2026-05-08 14:29:39,438 DEBG mensagem com Error no meio
2026-05-08 14:29:39,438 WARN aviso
2026-05-08 14:29:39,438 CRIT falha grave
2026-05-08 14:29:39,438 ERRO erro explícito
2026-05-08 14:29:39,438 TRAC rastreio detalhado
linha de continuação sem timestamp nem nível";

        let contagens = contar(conteudo);

        // Cada nível apareceu exatamente uma vez.
        assert_eq!(contagens.niveis.get("INFO"), Some(&1));
        assert_eq!(contagens.niveis.get("DEBG"), Some(&1));
        assert_eq!(contagens.niveis.get("WARN"), Some(&1));
        assert_eq!(contagens.niveis.get("CRIT"), Some(&1));
        assert_eq!(contagens.niveis.get("ERRO"), Some(&1));
        assert_eq!(contagens.niveis.get("TRAC"), Some(&1));
        // A linha de continuação não adicionou nenhuma chave nova.
        assert_eq!(contagens.niveis.len(), 6);
    }

    #[test]
    fn linha_de_continuacao_nao_conta_nivel() {
        let conteudo = "isso não tem timestamp nem nível válido na posição 3";
        let contagens = contar(conteudo);
        assert!(contagens.niveis.is_empty());
    }

    #[test]
    fn palavra_chave_error_e_case_insensitive() {
        let conteudo = "\
2026-05-08 14:29:39,438 DEBG contém Error no texto
2026-05-08 14:29:39,438 DEBG contém ERROR no texto
2026-05-08 14:29:39,438 DEBG contém error no texto";

        let contagens = contar(conteudo);

        // Error / ERROR / error contam todos para a mesma chave "error".
        assert_eq!(contagens.palavras.get("error"), Some(&3));
    }

    #[test]
    fn conta_uma_vez_por_linha_mesmo_com_multiplas_ocorrencias() {
        let conteudo = "2026-05-08 14:29:39,438 INFO error error error na mesma linha";
        let contagens = contar(conteudo);
        // Três ocorrências na mesma linha = 1 (contamos por linha).
        assert_eq!(contagens.palavras.get("error"), Some(&1));
    }
}
