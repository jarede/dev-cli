// Gravação atômica do snapshot e retenção dos arquivos antigos.
//
// Por que atômico? O consumidor lê o MESMO diretório enquanto o binário
// escreve. Escrever direto no arquivo final abre uma janela em que ele lê
// JSON truncado. O padrão é: escrever num `.tmp`, `sync` do arquivo e
// `rename` para o nome final — o rename é atômico dentro do mesmo filesystem.
//
// A idade de um snapshot sai do NOME do arquivo (granularidade de hora),
// jamais do mtime: mtime muda com cópia e backup.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};

/// Nome do arquivo a partir de um `coletado_em` ISO ("2026-08-07T14:00:00"),
/// com GRANULARIDADE DE HORA: "2026-08-07T14.json". Ordem lexicográfica =
/// ordem cronológica, então "o mais recente" é o último da lista ordenada.
/// Devolve `None` quando o texto não tem o prefixo esperado.
pub fn nome_arquivo(coletado_em: &str) -> Option<String> {
    // Os 13 primeiros caracteres de "YYYY-MM-DDTHH:MM:SS" são "YYYY-MM-DDTHH".
    if coletado_em.len() < 13 {
        return None;
    }
    // `into()` converte o corte em String heap -> compõe o nome completo.
    Some(format!("{}.json", &coletado_em[..13],))
}

/// Grava o conteúdo num arquivo temporário, sincroniza e renomeia para o
/// nome final (passo atômico). Cria o diretório se não existir e garante que
/// o arquivo fique legível pelo grupo (o consumidor roda como outro usuário)
/// mas NÃO gravável por outros usuários.
pub fn gravar_atomico(
    diretorio: &Path,
    nome_final: &str,
    conteudo: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(diretorio)?;

    let caminho_final = diretorio.join(nome_final);
    // O temporário mora no MESMO diretório: só assim o rename final é
    // atômico (a atomicidade exige o mesmo filesystem; gravar em /tmp e
    // renomear para outro diretório não é atômico).
    let caminho_tmp = diretorio.join(format!("{nome_final}.tmp"));

    let resultado = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(&caminho_tmp, conteudo)?;
        // `sync_all`: garante que os bytes chegaram ao disco ANTES do rename
        // — sem isso, um crash entre o nome e o conteúdo poderia deixar um
        // arquivo final com dados pela metade.
        // docs: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all
        let arquivo = fs::File::open(&caminho_tmp)?;
        arquivo.sync_all()?;

        // 0o644: dono escreve/lê, grupo lê, outros só leem — legível pelo
        // consumidor (outro usuário do grupo) e não gravável por terceiros.
        let mut permissao = fs::metadata(&caminho_tmp)?.permissions();
        permissao.set_mode(0o644);
        fs::set_permissions(&caminho_tmp, permissao)?;

        fs::rename(&caminho_tmp, &caminho_final)?;
        Ok(())
    })();

    // Nada de `.tmp` sobrando: qualquer falha no caminho acima remove o
    // temporário antes de devolver o erro.
    if resultado.is_err() {
        let _ = fs::remove_file(&caminho_tmp);
    }
    resultado?;
    Ok(caminho_final)
}

/// Lê o `NaiveDateTime` embutido no nome "YYYY-MM-DDTHH.json". Nomes fora do
/// padrão devolvem `None` — e por isso são IGNORADOS pela retenção, nunca
/// apagados.
fn data_do_nome(nome: &str) -> Option<NaiveDateTime> {
    let sem_extensao = nome.strip_suffix(".json")?;
    // O nome é "YYYY-MM-DDTHH": separa data ("YYYY-MM-DD") e hora ("HH").
    // Um nome como "2026-08-07T10:45:00.json" cai fora — hora não é um inteiro
    // de 1 dígito — e devolve None, então a retenção o ignora, não apaga.
    let (dia, hora) = sem_extensao.split_once('T')?;
    let data = NaiveDate::parse_from_str(dia, "%Y-%m-%d").ok()?;
    let hora: u32 = hora.parse().ok()?;
    data.and_hms_opt(hora, 0, 0)
}

/// Remove do diretório os snapshots mais velhos que `retencao_dias` dias.
///
/// A idade é calculada do NOME (não do mtime) comparando com o instante de
/// referência `agora`. Devolve a lista de arquivos removidos (para quem
/// quiser avisar). Leitura do diretório que falha é erro; remoção que falha
/// é IGNORADA — faxina não derruba uma coleta já gravada.
pub fn aplicar_retencao(
    diretorio: &Path,
    retencao_dias: u64,
    agora: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let agora_dt = NaiveDateTime::parse_from_str(agora, "%Y-%m-%dT%H:%M:%S")
        .map_err(|erro| format!("agora fora do formato ISO esperado: {erro}"))?;
    let limite = chrono::Duration::days(retencao_dias as i64);

    let mut removidos = Vec::new();
    for entrada in fs::read_dir(diretorio)? {
        let entrada = match entrada {
            Ok(e) => e,
            // Alguém criou/apagou arquivo no meio da faxina: segue em frente.
            Err(_) => continue,
        };
        let caminho = entrada.path();
        if !caminho.is_file() {
            continue;
        }
        let nome = entrada.file_name().to_string_lossy().into_owned();
        // Fora do padrão ("README", "notas.txt"): ignorado, NÃO apagado.
        let Some(data) = data_do_nome(&nome) else {
            continue;
        };
        let idade = agora_dt.signed_duration_since(data);
        if idade > limite {
            match fs::remove_file(&caminho) {
                Ok(()) => removidos.push(nome),
                // Falha ao remover é AVISO, nunca erro fatal.
                Err(_) => continue,
            }
        }
    }
    Ok(removidos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_arquivo_deriva_do_coletado_em() {
        assert_eq!(
            nome_arquivo("2026-08-07T14:00:00").as_deref(),
            Some("2026-08-07T14.json")
        );
        // Qualquer minuto/segundo do mesmo hora cai no MESMO arquivo.
        assert_eq!(
            nome_arquivo("2026-08-07T14:59:59").as_deref(),
            Some("2026-08-07T14.json")
        );
        // Texto curto demais: não há hora para nomear.
        assert_eq!(nome_arquivo("2026-08-07"), None);
    }

    #[test]
    fn grava_atomico_cria_o_arquivo_sem_tmp_residual() {
        let diretorio = tempfile::tempdir().expect("tempdir do teste");

        let caminho = gravar_atomico(diretorio.path(), "2026-08-07T14.json", "{\"versao\":1}")
            .expect("gravação atômica");

        // Conteúdo completo e nenhum `.tmp` sobrou no diretório.
        let texto = std::fs::read_to_string(&caminho).unwrap();
        assert_eq!(texto, "{\"versao\":1}");
        let entradas: Vec<_> = std::fs::read_dir(diretorio.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(entradas.iter().all(|nome| !nome.ends_with(".tmp")));
    }

    #[test]
    fn gravar_atomico_cria_o_diretorio_se_ausente() {
        let raiz = tempfile::tempdir().unwrap();
        let subdir = raiz.path().join("um").join("dois");
        let caminho = gravar_atomico(&subdir, "2026-08-07T14.json", "{\"a\":1}").unwrap();
        assert!(caminho.is_file());
    }

    #[test]
    fn retencao_remove_velhos_preserva_novos_e_ignora_fora_do_padrao() {
        let diretorio = tempfile::tempdir().unwrap();
        let agora = "2026-08-07T12:00:00";

        // Um snapshot de ontem (deve sumir) e um de hoje (deve ficar).
        fs::write(diretorio.path().join("2026-08-06T10.json"), "{}").unwrap();
        fs::write(diretorio.path().join("2026-08-07T10.json"), "{}").unwrap();
        // Fora do padrão: ignorado, NUNCA apagado.
        fs::write(diretorio.path().join("notas.txt"), "não sou snapshot").unwrap();
        fs::write(diretorio.path().join("2026-08-07T10:45:00.json"), "sub-est").unwrap();

        let removidos = aplicar_retencao(diretorio.path(), 1, agora).unwrap();
        assert_eq!(removidos, vec!["2026-08-06T10.json".to_string()]);

        let restantes: Vec<_> = std::fs::read_dir(diretorio.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(restantes.contains(&"2026-08-07T10.json".to_string()));
        assert!(restantes.contains(&"notas.txt".to_string()));
        assert!(restantes.contains(&"2026-08-07T10:45:00.json".to_string()));
    }

    #[test]
    fn retencao_zero_remove_tudo_do_padrao() {
        let diretorio = tempfile::tempdir().unwrap();
        fs::write(diretorio.path().join("2026-08-07T04.json"), "{}").unwrap();
        let removidos = aplicar_retencao(diretorio.path(), 0, "2026-08-07T05:00:00").unwrap();
        assert_eq!(removidos, vec!["2026-08-07T04.json".to_string()]);
    }
}
