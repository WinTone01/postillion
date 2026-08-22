//! Sohbet içeriğini SUNUCUDA materyalize etme.
//!
//! Panelin transkripti, bilgisayar KAPALIYKEN de göstermesi gerekiyor. Bu, işi
//! belgeyi tutan host'a yaptırma seçeneğini eliyor: host çevrimdışıysa
//! sorulacak kimse yok.
//!
//! Alternatif tarayıcıda loro çalıştırmaktı. Sunucuda yapmak iki nedenle daha
//! iyi: birleştirme kodu burada zaten var (`postillion-doc`, istemcinin
//! çalıştırdığının aynısı) ve ilerideki bulut çalıştırma sunucunun sohbeti
//! okuyup yazmasını zaten gerektirecek. Tarayıcıya konsa o iş için ikinci kez
//! yazılması gerekirdi.
//!
//! Bedeli açık: satırlar artık sunucu için opak değil. Uçtan uca şifreleme
//! olmadığı için bu zaten böyleydi — sunucu ayrıştırmıyordu, ayrıştıramıyor
//! değildi.

use postillion_doc::{SessionDoc, SessionMessageEntry};
use postillion_sync::room::Row;

/// Satırları birleştirip mesajları okur.
///
/// Sıra ÖNEMLİ değil — loro bir CRDT ve birleştirme değişmeli. Yine de
/// satırlar sıralı geliyor; bozuk tek bir satır bütün transkripti düşürmemeli
/// diye tek tek içe aktarılıyorlar.
pub fn materialize(rows: &[Row]) -> Result<Vec<SessionMessageEntry>, String> {
    let doc = loro::LoroDoc::new();
    let mut skipped = 0usize;

    for row in rows {
        if doc.import(&row.payload).is_err() {
            // Bir satırın bağımlılıkları henüz gelmemiş olabilir (park) ya da
            // gerçekten bozuk olabilir. İkisinde de doğru davranış devam
            // etmek: elde olan kadarını göstermek, hiçbir şey göstermemekten
            // iyi.
            skipped += 1;
        }
    }

    if skipped > 0 {
        tracing::warn!(skipped, total = rows.len(), "transkript: bazı satırlar içe aktarılamadı");
    }

    SessionDoc::from_doc(doc)
        .read_entries()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postillion_doc::{MessagePart, MessageRole};

    fn entry(id: &str, text: &str) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.into(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                id: format!("{id}-p0"),
                text: text.into(),
            }],
            created_at: 1_700_000_000_000,
            device_id: "dev-a".into(),
            status: None,
            continuation_of: None,
        }
    }

    /// Bir cihazın yazdığı satırlardan sunucu transkripti kurabilmeli.
    ///
    /// Panelin bilgisayar KAPALIYKEN de içeriği göstermesi buna bağlı: host'a
    /// soramadığımız için satırları kendimiz birleştiriyoruz.
    #[test]
    fn satirlardan_transkript_kuruluyor() {
        // Bir istemcinin yaptığını yapıyoruz: doc'a yaz, güncellemeyi dışa
        // aktar, satır yükü olarak taşı.
        let source = SessionDoc::init("chat-1").expect("doc");
        source.push_message(&entry("m1", "merhaba")).expect("yazım");
        let first = source
            .doc()
            .export(loro::ExportMode::Snapshot)
            .expect("dışa aktarım");

        source.push_message(&entry("m2", "ikinci")).expect("yazım");
        let second = source
            .doc()
            .export(loro::ExportMode::Snapshot)
            .expect("dışa aktarım");

        let rows = vec![
            Row { seq: 1, device: "dev-a".into(), batch_id: "b1".into(), payload: first },
            Row { seq: 2, device: "dev-a".into(), batch_id: "b2".into(), payload: second },
        ];

        let entries = materialize(&rows).expect("transkript");
        let texts: Vec<String> = entries
            .iter()
            .flat_map(|e| e.parts.iter())
            .filter_map(|p| match p {
                MessagePart::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(texts, vec!["merhaba", "ikinci"]);
    }

    /// Bozuk ya da bağımlılığı eksik bir satır bütün transkripti düşürmemeli:
    /// elde olan kadarını göstermek, hiçbir şey göstermemekten iyi.
    #[test]
    fn bozuk_satir_transkripti_dusurmuyor() {
        let source = SessionDoc::init("chat-2").expect("doc");
        source.push_message(&entry("m1", "sağlam")).expect("yazım");
        let good = source
            .doc()
            .export(loro::ExportMode::Snapshot)
            .expect("dışa aktarım");

        let rows = vec![
            Row { seq: 1, device: "dev-a".into(), batch_id: "b1".into(), payload: vec![0xff; 32] },
            Row { seq: 2, device: "dev-a".into(), batch_id: "b2".into(), payload: good },
        ];

        let entries = materialize(&rows).expect("transkript");
        assert_eq!(entries.len(), 1, "sağlam satır yine okunmalı");
    }

    #[test]
    fn bos_oda_bos_transkript() {
        assert!(materialize(&[]).expect("transkript").is_empty());
    }
}
