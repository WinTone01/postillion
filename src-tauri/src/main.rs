// Windows'ta release derlemede konsol penceresi açılmasın.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK'nın DMABUF renderer'ı Wayland + NVIDIA kombinasyonunda
    // "Error 71 (Protocol error)" ile pencereyi açar açmaz düşüyor.
    // GTK başlamadan önce ayarlanmalı, yoksa etkisi olmaz.
    //
    // Zaten set edilmişse dokunmuyoruz — kullanıcı bilerek değiştirmiş olabilir.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    postillion_lib::run()
}
