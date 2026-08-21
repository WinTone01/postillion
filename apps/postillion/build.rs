//! Windows kaynak gömme: uygulama ikonu.
//!
//! Manifest kasıtlı olarak yok — gpui kendi `RT_MANIFEST`ini (id 1, DPI
//! farkındalığı + common controls) gömüyor; ikinci bir manifest aynı kaynak
//! kimliğine düşer ve bağlama hata verir.

fn main() {
    println!("cargo:rerun-if-changed=windows/postillion.rc");
    println!("cargo:rerun-if-changed=windows/postillion.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("windows/postillion.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embedding the windows icon resource");
    }
}
