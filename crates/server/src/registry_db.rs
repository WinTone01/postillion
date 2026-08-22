//! Çalışma alanı kaydının Postgres deposu.
//!
//! Sohbet satırlarından farklı olarak burada bir günlük yok: kayıt yalnızca
//! GÜNCEL DURUMU tutuyor, satır başına tek kayıt ve alan başına saat. Bu
//! yüzden tablo sohbet sayısıyla büyüyor, yazım sayısıyla değil.

use futures::future::BoxFuture;
use postillion_doc::{RegistryRow, RowOp};
use postillion_sync::registry_room::{AppliedBatch, RegistryState, RegistryStore};
use postillion_sync::SyncError;
use sqlx::{PgPool, Row as _};

pub const SCHEMA: &str = r#"
create table if not exists registry_meta (
  org      text   primary key,
  seq      bigint not null default 0,
  gc_floor bigint not null default 0
);

create table if not exists registry_rows (
  org     text    not null,
  kind    text    not null,
  id      text    not null,
  seq     bigint  not null,
  deleted boolean not null default false,
  del_hlc text,
  fields  jsonb   not null default '{}',
  clocks  jsonb   not null default '{}',
  primary key (org, kind, id)
);

-- Delta eşitlemenin tek sorgusu: `seq > imleç`.
create index if not exists registry_rows_org_seq on registry_rows (org, seq);
"#;

#[derive(Clone)]
pub struct PgRegistry {
    pool: PgPool,
    /// Kimin çevrimiçi olduğu BELLEKTE — bkz. `crate::presence`.
    presence: crate::presence::Presence,
}

impl PgRegistry {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            presence: crate::presence::Presence::new(),
        }
    }

    /// Panelin okuduğu canlı cihaz listesi.
    pub fn presence(&self) -> &crate::presence::Presence {
        &self.presence
    }
}

fn db_err(e: sqlx::Error) -> SyncError {
    SyncError::Http(e.to_string())
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn row_from(record: &sqlx::postgres::PgRow) -> Result<RegistryRow, SyncError> {
    Ok(RegistryRow {
        kind: record.try_get("kind").map_err(db_err)?,
        id: record.try_get("id").map_err(db_err)?,
        seq: to_u64(record.try_get("seq").map_err(db_err)?),
        deleted: record.try_get("deleted").map_err(db_err)?,
        del_hlc: record.try_get("del_hlc").map_err(db_err)?,
        fields: serde_json::from_value(record.try_get("fields").map_err(db_err)?)
            .map_err(|e| SyncError::Protocol(e.to_string()))?,
        clocks: serde_json::from_value(record.try_get("clocks").map_err(db_err)?)
            .map_err(|e| SyncError::Protocol(e.to_string()))?,
    })
}

impl RegistryStore for PgRegistry {
    fn live_presence(&self, org: &str) -> std::collections::HashMap<String, i64> {
        self.presence.live(org)
    }

    fn note_presence(&self, org: &str, device: &str, at: i64) {
        self.presence.beat(org, device, at);
    }

    fn state(&self, org: &str) -> BoxFuture<'static, Result<RegistryState, SyncError>> {
        let pool = self.pool.clone();
        let org = org.to_string();
        Box::pin(async move {
            let record = sqlx::query(
                "select coalesce(seq, 0)::bigint as seq,
                        coalesce(gc_floor, 0)::bigint as gc_floor
                   from registry_meta where org = $1",
            )
            .bind(&org)
            .fetch_optional(&pool)
            .await
            .map_err(db_err)?;

            // Kaydı olmayan org boş bir oda: sıfır sıra, sıfır ufuk. Hata
            // döndürmek ilk cihazın hiç bağlanamaması demekti.
            Ok(match record {
                Some(r) => RegistryState {
                    seq: to_u64(r.try_get("seq").map_err(db_err)?),
                    gc_floor: to_u64(r.try_get("gc_floor").map_err(db_err)?),
                },
                None => RegistryState::default(),
            })
        })
    }

    fn rows_since(
        &self,
        org: &str,
        since: u64,
    ) -> BoxFuture<'static, Result<Vec<RegistryRow>, SyncError>> {
        let pool = self.pool.clone();
        let org = org.to_string();
        Box::pin(async move {
            let records = sqlx::query(
                "select kind, id, seq, deleted, del_hlc, fields, clocks
                   from registry_rows
                  where org = $1 and seq > $2
                  order by seq, kind, id",
            )
            .bind(&org)
            .bind(since as i64)
            .fetch_all(&pool)
            .await
            .map_err(db_err)?;

            records.iter().map(row_from).collect()
        })
    }

    fn apply_batch(
        &self,
        org: &str,
        ops: Vec<RowOp>,
    ) -> BoxFuture<'static, Result<AppliedBatch, SyncError>> {
        let pool = self.pool.clone();
        let org = org.to_string();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(db_err)?;

            // Meta satırı KİLİTLENİYOR. Sıra bu batch'in kimliği ve iki
            // eşzamanlı batch aynı sırayı alırsa delta eşitleme bozulur:
            // ikinci batch'in satırları birincinin sırasıyla yazılır ve
            // imleci o sıraya gelmiş bir istemci onları hiç görmez.
            let meta = sqlx::query(
                "insert into registry_meta (org) values ($1)
                 on conflict (org) do update set org = excluded.org
                 returning seq::bigint, gc_floor::bigint",
            )
            .bind(&org)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?;

            let current = to_u64(meta.try_get("seq").map_err(db_err)?);
            let next = current + 1;

            let mut touched: Vec<RegistryRow> = Vec::new();
            for op in &ops {
                // Aynı batch içinde aynı satıra birden çok op gelebiliyor;
                // sonraki op öncekinin BİRLEŞMİŞ hâlini görmeli.
                let before = match touched.iter().position(|r| r.kind == op.kind && r.id == op.id) {
                    Some(index) => Some(touched[index].clone()),
                    None => {
                        let record = sqlx::query(
                            "select kind, id, seq, deleted, del_hlc, fields, clocks
                               from registry_rows
                              where org = $1 and kind = $2 and id = $3",
                        )
                        .bind(&org)
                        .bind(&op.kind)
                        .bind(&op.id)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(db_err)?;
                        record.as_ref().map(row_from).transpose()?
                    }
                };

                let (merged, changed) = postillion_doc::apply_op(before.as_ref(), op);
                let Some(mut merged) = merged.filter(|_| changed) else {
                    continue;
                };
                merged.seq = next;
                touched.retain(|r| !(r.kind == merged.kind && r.id == merged.id));
                touched.push(merged);
            }

            let seq = if touched.is_empty() { current } else { next };

            for row in &touched {
                sqlx::query(
                    "insert into registry_rows (org, kind, id, seq, deleted, del_hlc, fields, clocks)
                     values ($1, $2, $3, $4, $5, $6, $7, $8)
                     on conflict (org, kind, id) do update set
                       seq = excluded.seq, deleted = excluded.deleted,
                       del_hlc = excluded.del_hlc, fields = excluded.fields,
                       clocks = excluded.clocks",
                )
                .bind(&org)
                .bind(&row.kind)
                .bind(&row.id)
                .bind(row.seq as i64)
                .bind(row.deleted)
                .bind(&row.del_hlc)
                .bind(serde_json::to_value(&row.fields).unwrap_or_default())
                .bind(serde_json::to_value(&row.clocks).unwrap_or_default())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            }

            if !touched.is_empty() {
                sqlx::query("update registry_meta set seq = $2 where org = $1")
                    .bind(&org)
                    .bind(next as i64)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }

            tx.commit().await.map_err(db_err)?;
            Ok(AppliedBatch {
                rows: touched,
                seq,
            })
        })
    }
}
