//! Pano görüntü okuma yolu. `#[ignore]`: panoda görüntü olmasını gerektiriyor.
//!
//!   wl-copy --type image/png < resim.png
//!   cargo test --test clipboard_probe -- --ignored --nocapture

/// Uygulamanın kullandığı komutun aynısı: pano → PNG → base64.
#[test]
#[ignore]
fn panodaki_goruntu_png_olarak_okunuyor() {
    let shot = postillion_lib::testing::clipboard_image().expect("panoda görüntü olmalı");

    assert_eq!(shot.media_type, "image/png");

    // Base64'ü çözüp gerçekten PNG olduğunu doğrula — imza sekiz bayt.
    let decoded = decode_base64(&shot.data);
    assert!(
        decoded.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "PNG imzası yok: {:?}",
        &decoded[..8.min(decoded.len())]
    );

    eprintln!("OK: {} bayt base64, {} bayt PNG", shot.data.len(), decoded.len());
}

fn decode_base64(input: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;

    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let Some(value) = ALPHABET.iter().position(|c| *c == byte) else {
            continue;
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    out
}
