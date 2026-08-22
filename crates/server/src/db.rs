//! Postgres deposu ve şema.
//!
//! SQLite değil Postgres: çok kullanıcılı bir serviste SQLite'ın tek yazıcısı
//! herkesi sıraya sokardı.
//!
//! Şema açılışta uygulanıyor ve tamamı idempotent — sunucuyu yeniden
//! başlatmak ayrı bir göç adımı gerektirmemeli.

use futures::future::BoxFuture;
use postillion_sync::room::{ChatStore, Row, RoomState};
use postillion_sync::SyncError;
use sqlx::{PgPool, Row as _};

/// Tablolar ve indeksler.
///
/// `seq` SOHBET BAŞINA 1'den başlayıp birer birer artıyor ve boşluk
/// bırakmıyor. Global bir kimlik sütunu kullanmak daha ucuzdu ama İSTEMCİYİ
/// KIRIYOR: imleç yalnızca satır bitişikse ilerliyor (`seq <= cursor + 1`),
/// dolayısıyla 1'den sonra 42 gelen bir oda kalıcı bir boşluk gibi görünüyor
/// ve istemci her turda tüm geçmişi yeniden çekip aynı satırları tekrar
/// uyguluyor. Uçtan uca test bunu satırın iki kez ulaşması olarak yakaladı.
///
/// Sayaç ayrı bir tabloda: `max(seq) + 1` okuması eşzamanlı iki yazımda aynı
/// değeri verirdi.
const SCHEMA: &str = r#"
create table if not exists chat_rooms (
  chat_id  text   primary key,
  head_seq bigint not null
);

create table if not exists chat_rows (
  chat_id    text        not null,
  seq        bigint      not null,
  device     text        not null,
  batch_id   text        not null,
  payload    bytea       not null,
  created_at timestamptz not null default now(),
  primary key (chat_id, seq)
);

-- Aynı gönderim iki kez yazılamaz: istemci bağlantı koptuğunda tekrarlıyor.
create unique index if not exists chat_rows_batch on chat_rows (chat_id, batch_id);

create table if not exists chat_checkpoints (
  chat_id    text        primary key,
  seq        bigint      not null,
  payload    bytea       not null,
  updated_at timestamptz not null default now()
);
"#;

/// Şema kurulumunu sıraya sokan danışma kilidinin anahtarı.
///
/// Rastgele ama SABİT: aynı veritabanına bağlanan her süreç aynı anahtarı
/// kullanmak zorunda, yoksa kilit iş görmez.
const SCHEMA_LOCK: i64 = 0x504f5354_494c4c4f_u64 as i64;

pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPool::connect(url).await?;

    // `create table if not exists` eşzamanlılığa karşı GÜVENLİ DEĞİL: iki
    // süreç aynı anda çalıştırdığında ikisi de tablonun yokluğunu görüyor ve
    // biri "already exists" ile düşüyor. Sunucunun iki kopyasını aynı anda
    // başlatmak bunu tetiklerdi.
    //
    // Kilit tek bir bağlantı üzerinden alınıyor: danışma kilitleri oturuma
    // bağlı, havuzdan farklı bir bağlantı gelirse serbest bırakma tutmaz.
    let mut conn = pool.acquire().await?;
    sqlx::query("select pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&mut *conn)
        .await?;

    let applied = sqlx::raw_sql(&format!("{SCHEMA}\n{}", crate::registry_db::SCHEMA))
        .execute(&mut *conn)
        .await;

    // Şema başarısız olsa bile kilit bırakılmalı, yoksa sonraki her açılış
    // sonsuza kadar bekler.
    sqlx::query("select pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&mut *conn)
        .await?;
    applied?;

    Ok(pool)
}

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// `bigint` işaretli geliyor, protokol `u64` istiyor.
///
/// Negatif bir değer buraya ancak veri bozulmuşsa gelir; panik yerine sıfır
/// vermek istemciyi baştan yakalamaya düşürüyor, çökertmiyor.
fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn db_err(e: sqlx::Error) -> SyncError {
    SyncError::Http(e.to_string())
}

impl ChatStore for PgStore {
    fn state(&self, chat_id: &str) -> BoxFuture<'static, Result<RoomState, SyncError>> {
        let pool = self.pool.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            // Tek sorgu: üç ayrı sorgu katılımı üç gidiş dönüş yapardı.
            let row = sqlx::query(
                r#"
                -- Her sütun açıkça bigint'e çevriliyor: `octet_length` int4
                -- döndürüyor ve sürücü tipi çözmeye çalıştığında düşüyor.
                select
                  coalesce((select head_seq from chat_rooms where chat_id = $1), 0)::bigint
                    as head_seq,
                  coalesce((select seq from chat_checkpoints where chat_id = $1), 0)::bigint
                    as checkpoint_seq,
                  coalesce((select octet_length(payload) from chat_checkpoints
                             where chat_id = $1), 0)::bigint                as checkpoint_size,
                  (select count(*) from chat_rows where chat_id = $1)::bigint
                    as row_count,
                  coalesce((select sum(octet_length(payload)) from chat_rows
                             where chat_id = $1), 0)::bigint                as row_bytes
                "#,
            )
            .bind(&chat_id)
            .fetch_one(&pool)
            .await
            .map_err(db_err)?;

            let checkpoint_seq: i64 = row.try_get("checkpoint_seq").map_err(db_err)?;
            Ok(RoomState {
                head_seq: to_u64(row.try_get::<i64, _>("head_seq").map_err(db_err)?),
                // Anlık görüntünün kapsadığı nokta aynı zamanda tabandır:
                // altındaki satırlar artık gerekmiyor.
                seq_floor: to_u64(checkpoint_seq),
                checkpoint_seq: to_u64(checkpoint_seq),
                checkpoint_size: to_u64(row.try_get("checkpoint_size").map_err(db_err)?),
                row_count: to_u64(row.try_get("row_count").map_err(db_err)?),
                row_bytes: to_u64(row.try_get("row_bytes").map_err(db_err)?),
            })
        })
    }

    fn rows_after(
        &self,
        chat_id: &str,
        after: u64,
        exclude_device: Option<String>,
    ) -> BoxFuture<'static, Result<Vec<Row>, SyncError>> {
        let pool = self.pool.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            let rows = sqlx::query(
                r#"
                select seq, device, batch_id, payload
                  from chat_rows
                 where chat_id = $1
                   and seq > $2
                   and ($3::text is null or device <> $3)
                 order by seq
                "#,
            )
            .bind(&chat_id)
            .bind(after as i64)
            .bind(exclude_device)
            .fetch_all(&pool)
            .await
            .map_err(db_err)?;

            rows.into_iter()
                .map(|row| {
                    Ok(Row {
                        seq: to_u64(row.try_get("seq").map_err(db_err)?),
                        device: row.try_get("device").map_err(db_err)?,
                        batch_id: row.try_get("batch_id").map_err(db_err)?,
                        payload: row.try_get("payload").map_err(db_err)?,
                    })
                })
                .collect()
        })
    }

    fn append(
        &self,
        chat_id: &str,
        device: String,
        batch_id: String,
        payload: Vec<u8>,
    ) -> BoxFuture<'static, Result<(u64, bool), SyncError>> {
        let pool = self.pool.clone();
        let chat_id = chat_id.to_string();
        Box::pin(async move {
            // Tek ifade, çünkü sayacı artırmak ile satırı yazmak AYRILAMAZ:
            // araya giren bir hata sayacı yakmış olurdu ve yanan bir sıra
            // istemcinin gördüğü kalıcı bir boşluk demek.
            //
            // `bumped` yalnızca `existing` boşken üretiyor: tekrarlanan bir
            // gönderim sayacı hiç kıpırdatmamalı.
            let row = sqlx::query(
                r#"
                with existing as (
                  select seq from chat_rows where chat_id = $1 and batch_id = $3
                ), bumped as (
                  insert into chat_rooms (chat_id, head_seq)
                  select $1, 1 where not exists (select 1 from existing)
                  on conflict (chat_id)
                    do update set head_seq = chat_rooms.head_seq + 1
                  returning head_seq
                ), inserted as (
                  insert into chat_rows (chat_id, seq, device, batch_id, payload)
                  select $1, head_seq, $2, $3, $4 from bumped
                  returning seq
                )
                select seq, false as dup from inserted
                union all
                -- Tekrar: AYNI sıra geri veriliyor. Yeni bir sıra istemcinin
                -- imlecini almadığı satırların ötesine taşırdı.
                select seq, true as dup from existing
                "#,
            )
            .bind(&chat_id)
            .bind(&device)
            .bind(&batch_id)
            .bind(&payload)
            .fetch_one(&pool)
            .await
            .map_err(db_err)?;

            Ok((
                to_u64(row.try_get("seq").map_err(db_err)?),
                row.try_get("dup").map_err(db_err)?,
            ))
        })
    }
}
