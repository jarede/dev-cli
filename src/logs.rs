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
use std::io::BufRead;
use std::io::Write;
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
        // `match` sobre a referência do enum; o `match` exaustivo obriga a
        // tratar cada variante nova assim que ela for adicionada.
        match &self.comando {
            LogsCommands::Stats(args) => args.execute(),
            LogsCommands::Containers(args) => args.execute(),
        }
    }
}

/// Subcomandos de `logs`.
#[derive(Subcommand, Debug)]
enum LogsCommands {
    /// Estatísticas de logs de containers (arquivos supervisord).
    Stats(StatsArgs),
    /// Estatísticas de logs dos containers detectados via `container list`.
    Containers(ContainersArgs),
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

// Níveis que a saída de `container logs` costuma usar (formatos ao estilo
// das crates `tracing`/`log`, ou de logging do Python). Comparamos sempre em
// maiúsculo, então "warn" e "WARN" caem na mesma chave.
const NIVEIS_CONTAINER: [&str; 8] = [
    "TRACE", "DEBUG", "INFO", "WARN", "WARNING", "ERROR", "CRITICAL", "FATAL",
];

/// Estatísticas de logs de todos os containers detectados via `container list`.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ContainersArgs {
    /// Container específico; se omitido, varre todos os detectados por `container list`.
    // `Option<String>`: o clap deixa esse argumento posicional opcional porque
    // o tipo é `Option`; sem valor na linha de comando, vira `None`.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Acompanha os logs em tempo real (como `tail -f`), redesenhando o painel a cada linha nova.
    // `bool` com `#[arg(short, long)]`: o clap trata automaticamente como uma
    // flag (`-f`/`--follow`) que não recebe valor — presente = `true`.
    #[arg(short = 'f', long, help_heading = crate::help::OPCOES)]
    follow: bool,
}

impl ContainersArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `if let ... else`: um único container pedido vira um Vec de 1;
        // sem container, perguntamos ao binário `container` quem está rodando.
        let nomes = if let Some(container) = &self.container {
            vec![container.clone()]
        } else {
            listar_containers()?
        };

        if self.follow {
            // Modo ao vivo: fica bloqueado imprimindo atualizações direto no
            // terminal. Só retorna quando todos os containers pararem de logar
            // (ou nunca, se o usuário encerrar com Ctrl+C antes disso).
            seguir_containers(&nomes)?;
            return Ok(String::new());
        }

        let mut saida = String::new();
        for nome in nomes {
            let conteudo = obter_logs(&nome)?;
            // NÚCLEO PURO: mesmo princípio do `contar` acima, mas adaptado ao
            // formato colorido (códigos ANSI) que `container logs` emite.
            let niveis = contar_niveis_container(&conteudo);
            saida.push_str(&renderizar_container(&nome, &niveis));
        }
        Ok(saida.trim_end().to_string())
    }
}

// CASCA DE IO: modo `-f`. Abre um processo `container logs -f <nome>` por
// container; cada um é lido numa thread própria (senão o `for` bloqueante de
// uma leitura travaria a leitura das outras). As threads só enviam linhas por
// um canal `mpsc`; quem acumula contagens e redesenha o painel é a thread
// principal, evitando qualquer mutex compartilhado entre elas.
fn seguir_containers(nomes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // `mpsc` = multi-producer, single-consumer: uma thread por container
    // (produtoras) alimentam um único receptor aqui na thread principal.
    let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();

    for nome in nomes {
        let mut child = std::process::Command::new("container")
            .args(["logs", "-f", nome])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|erro| format!("falha ao executar 'container logs -f {nome}': {erro}"))?;

        // `take()` retira o stdout de dentro do `Child` (deixando `None` no
        // lugar) para podermos movê-lo para a thread de leitura junto com o
        // resto do `child` (que precisamos manter vivo até o processo acabar).
        let stdout = child
            .stdout
            .take()
            .ok_or("não foi possível capturar a saída do container")?;

        let tx = tx.clone();
        let nome_da_thread = nome.clone();
        std::thread::spawn(move || {
            let leitor = std::io::BufReader::new(stdout);
            // `lines()` bloqueia entre chamadas até a próxima linha chegar —
            // é isso que dá o efeito de "tempo real"; `map_while(Result::ok)`
            // ignora erros de leitura e para no primeiro (ex.: pipe fechado).
            for linha in leitor.lines().map_while(Result::ok) {
                // Erro de `send` só ocorre se a thread principal já saiu;
                // ignoramos porque não há mais ninguém para avisar.
                let _ = tx.send((nome_da_thread.clone(), linha));
            }
            // Espera o processo `container logs -f` encerrar de fato (evita
            // deixá-lo como zumbi quando o container para).
            let _ = child.wait();
        });
    }
    // Descarta nosso clone original do transmissor: o canal só fecha (e o
    // `for` abaixo termina) quando TODAS as threads soltarem o `tx` delas.
    drop(tx);

    // Contagem acumulada por container, viva só na thread principal (não
    // precisa de `Mutex`: nenhuma outra thread toca nela, só recebem por canal).
    // `collect()` monta o `BTreeMap` a partir de um iterador de tuplas
    // `(chave, valor)` — cada container começa com um mapa de níveis vazio.
    let mut totais: BTreeMap<String, BTreeMap<String, usize>> =
        nomes.iter().map(|nome| (nome.clone(), BTreeMap::new())).collect();

    // `Receiver` (o `rx`) implementa `Iterator`: o `for` bloqueia esperando a
    // próxima mensagem e só termina quando todos os `tx` (um por thread) forem
    // descartados, ou seja, quando todas as threads produtoras acabarem.
    for (nome, linha) in rx {
        let niveis_da_linha = contar_niveis_container(&linha);
        let acumulado = totais.entry(nome).or_default();
        for (nivel, quantidade) in niveis_da_linha {
            *acumulado.entry(nivel).or_insert(0) += quantidade;
        }

        // Redesenha o painel inteiro a cada linha nova: limpa a tela e move o
        // cursor para o topo (códigos ANSI), como um dashboard ao vivo.
        print!("\x1b[2J\x1b[H");
        for nome in nomes {
            if let Some(niveis) = totais.get(nome) {
                print!("{}", renderizar_container(nome, niveis));
            }
        }
        // `print!` só escreve no buffer da stdout; sem `flush` o painel não
        // apareceria até o buffer encher.
        std::io::stdout().flush()?;
    }

    Ok(())
}

// CASCA DE IO: pergunta ao binário `container` quais containers existem.
// `-q` faz o comando devolver só os IDs/nomes, um por linha.
fn listar_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // `Command::new(...).args([...]).output()` executa o processo, espera ele
    // terminar e captura stdout/stderr/status de uma vez (diferente de
    // `spawn()`, que devolve o `Child` já em execução para controlarmos nós
    // mesmos — usado em `seguir_containers` para o modo `-f`).
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

    // `stdout`/`stderr` vêm como `Vec<u8>` (bytes crus); `from_utf8_lossy`
    // converte para `str` trocando bytes inválidos por `�` em vez de falhar.
    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        // `str::trim`/`str::to_string` usados como valor (sem `|x| x.trim()`):
        // toda função de um argumento pode virar closure implícita em `map`.
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .map(str::to_string)
        .collect())
}

// CASCA DE IO: busca o log completo de um container via `container logs`.
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

// Remove sequências de escape ANSI (ex.: "\x1b[32m") de uma linha, deixando
// só o texto visível. `container logs` colore a saída, o que atrapalharia a
// busca pelo token do nível se não fosse removido antes.
fn remover_ansi(linha: &str) -> String {
    let mut limpa = String::with_capacity(linha.len());
    let mut chars = linha.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Descarta tudo até o 'm' que fecha o código de escape (formato
            // "\x1b[<códigos>m"); `by_ref()` evita mover o iterador inteiro.
            for ch in chars.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
        } else {
            limpa.push(c);
        }
    }
    limpa
}

// NÚCLEO PURO: conta ocorrências de cada nível na 2ª coluna de cada linha
// (depois da data), já sem códigos ANSI. Também é chamada linha a linha pelo
// modo `-f` (uma linha por vez), então precisa funcionar tanto para um texto
// gigante quanto para uma única linha recém-chegada.
fn contar_niveis_container(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Índice 1 (2º token) = nível: o formato de `container logs` é
        // "<data> <nível> <origem>: <mensagem>", sem hora junto da data.
        if let Some(token) = limpa.split_whitespace().nth(1) {
            // `to_uppercase()` normaliza antes de comparar/guardar, para que
            // "info" e "INFO" caiam sempre na mesma chave do mapa.
            let token_maiusculo = token.to_uppercase();
            if NIVEIS_CONTAINER.contains(&token_maiusculo.as_str()) {
                *niveis.entry(token_maiusculo).or_insert(0) += 1;
            }
        }
    }
    niveis
}

// Monta o bloco de texto (colorido) com os níveis de um container.
// Recebe um `BTreeMap` "cru" (em vez do `struct Contagens` usado por
// `renderizar`) porque aqui só existe uma dimensão de contagem (níveis) — não
// há a segunda categoria "palavras-chave no texto" que o modo `stats` tem.
fn renderizar_container(nome: &str, niveis: &BTreeMap<String, usize>) -> String {
    let mut saida = format!("📦 {}\n", nome.bold());

    // `any`: verdadeiro se existir ao menos uma entrada com contagem > 0;
    // pára na primeira que satisfizer, sem percorrer o mapa inteiro à toa.
    if niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   ");
        let campos: Vec<String> = niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    } else {
        saida.push_str("   (nenhum nível reconhecido nos logs)\n");
    }

    saida
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
        "ERROR" | "ERRO" | "CRIT" | "CRITICAL" | "FATAL" => texto.red().to_string(),
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

    #[test]
    fn remove_ansi_preserva_apenas_texto_visivel() {
        // Sequência real emitida por `container logs`: "\x1b[32m INFO\x1b[0m".
        let linha = "\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: msg";
        assert_eq!(remover_ansi(linha), "2026-07-03  INFO dev_web: msg");
    }

    #[test]
    fn conta_niveis_container_ignora_codigos_ansi() {
        let conteudo = "\
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: server starting
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: falha ao conectar
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: lento
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[35mCRITICAL\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: crash iminente";

        let niveis = contar_niveis_container(conteudo);

        assert_eq!(niveis.get("INFO"), Some(&1));
        assert_eq!(niveis.get("ERROR"), Some(&1));
        assert_eq!(niveis.get("WARN"), Some(&1));
        assert_eq!(niveis.get("CRITICAL"), Some(&1));
        assert_eq!(niveis.len(), 4);
    }

    #[test]
    fn conta_niveis_container_ignora_linhas_sem_nivel_conhecido() {
        let conteudo = "mensagem qualquer sem nivel reconhecivel no segundo token";
        let niveis = contar_niveis_container(conteudo);
        assert!(niveis.is_empty());
    }
}
