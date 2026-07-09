// CASCA DE IO: subcomando `logs stats`.
// Lê arquivos supervisord.log do disco, delega o cálculo ao núcleo puro
// e delega a formatação ao módulo de render.

use std::path::PathBuf;

// `Args`: macro de derive do clap que gera, a partir dos campos anotados,
// o parser de linha de comando (flags, posicionais, valores default).
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
use clap::Args;

use crate::logs::render::renderizar;
use nucleo::core::contar;

/// Estatísticas de logs de um container específico, ou de todos.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct StatsArgs {
    /// Container específico; se omitido, varre todos em --path.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Caminho do diretório com os logs dos containers.
    // `#[arg(long, default_value = "dados/logs", ...)]`: `long` expõe o campo
    // como `--path <valor>`; `default_value` faz o clap preencher `path`
    // automaticamente quando a flag não é passada, então o campo não precisa
    // ser `Option<PathBuf>`.
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
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
            // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.map_err
            // docs: https://doc.rust-lang.org/std/fs/fn.read_to_string.html
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

    /// Resolve a lista de arquivos a processar: um único container ou todos.
    fn descobrir_alvos(&self) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
        // `if let Some(...)` desestrutura o Option: só entra se o usuário passou container.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html
        if let Some(container) = &self.container {
            // `join` monta o caminho `<path>/<container>/supervisord.log`.
            // docs: https://doc.rust-lang.org/std/path/struct.Path.html#method.join
            let caminho = self.path.join(container).join("supervisord.log");
            if !caminho.exists() {
                // `.into()` converte a String para o `Box<dyn Error>` do retorno.
                // docs: https://doc.rust-lang.org/std/convert/trait.Into.html
                return Err(format!(
                    "arquivo de log não encontrado para o container '{container}': '{}'",
                    caminho.display()
                )
                .into());
            }
            // `vec![...]` cria um Vec com um único elemento.
            // docs: https://doc.rust-lang.org/std/macro.vec.html
            return Ok(vec![(container.clone(), caminho)]);
        }

        // Sem container: listamos as entradas do diretório `--path`.
        // docs: https://doc.rust-lang.org/std/fs/fn.read_dir.html
        let entradas = std::fs::read_dir(&self.path).map_err(|erro| {
            format!(
                "não foi possível ler o diretório '{}': {erro}",
                self.path.display()
            )
        })?;

        let mut nomes = Vec::new();
        // `read_dir` produz um iterador de `Result<DirEntry>`: cada leitura pode falhar.
        // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html
        for entrada in entradas {
            let entrada = entrada?;
            // Só nos interessam subdiretórios (um por container). A "let chain"
            // com `&& let` encadeia duas condições num único `if`: só empurra o
            // nome se for diretório E o nome for UTF-8 válido (`to_str()` devolve
            // `Option<&str>`, `None` se não for).
            // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_type
            // docs: https://doc.rust-lang.org/std/fs/struct.FileType.html#method.is_dir
            // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_name
            // docs: https://doc.rust-lang.org/std/ffi/struct.OsStr.html#method.to_str
            if entrada.file_type()?.is_dir()
                && let Some(nome) = entrada.file_name().to_str()
            {
                nomes.push(nome.to_string());
            }
        }
        // A ordem do `read_dir` não é garantida; ordenamos para saída estável.
        // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort
        nomes.sort();

        // Transforma cada nome em uma tupla (nome, caminho do log).
        // `into_iter` consome o Vec; `map` adapta cada item; `collect` remonta um Vec.
        // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html#tymethod.into_iter
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
        Ok(nomes
            .into_iter()
            .map(|nome| {
                let caminho = self.path.join(&nome).join("supervisord.log");
                (nome, caminho)
            })
            .collect())
    }
}
