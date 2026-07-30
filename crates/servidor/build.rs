// Build script: decide EM COMPILE-TIME se o portal (web/dist) existe para
// ser embutido no binário. `cargo::rustc-cfg` liga um flag de compilação
// condicional (`#[cfg(portal_embutido)]`) que o código-fonte consulta —
// assim `cargo build` numa máquina sem o build do frontend continua verde,
// só que servindo uma página explicativa no lugar do portal.
// `rustc-check-cfg` declara o cfg customizado para o rustc não emitir o
// warning `unexpected_cfgs` (todo cfg fora da lista padrão precisa disso).
// docs: https://doc.rust-lang.org/cargo/reference/build-scripts.html
fn main() {
    println!("cargo::rustc-check-cfg=cfg(portal_embutido)");
    // CARGO_MANIFEST_DIR = crates/servidor; o dist fica dois níveis acima.
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if dist.join("index.html").exists() {
        println!("cargo::rustc-cfg=portal_embutido");
    }
    // Recompila quando o build do frontend mudar (ou aparecer/sumir).
    println!("cargo::rerun-if-changed=../../web/dist");
}
