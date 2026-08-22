//! Jetonların ve oda sahipliğinin Postgres deposu.
//!
//! `api_tokens` tablosunu PANEL dolduruyor, sunucu yalnızca okuyor. Adonis'in
//! kendi jeton tablosunu okumak da mümkündü ama sunucuyu çerçevenin özel
//! özetleme biçimine bağlardı: bir sürüm yükseltmesi kimlik doğrulamayı
//! sessizce kırardı.

use futures::future::BoxFuture;
use postillion_sync::SyncError;
use sqlx::{PgPool, Row as _};

use crate::auth::TokenStore;
use crate::ownership::{OwnerStore, Scope};

pub const SCHEMA: &str = r#"
create table if not exists api_tokens (
  id           bigserial   primary key,
  user_id      bigint      not null,
  name         text        not null,
  -- Yalnızca ÖZET. Ham jeton hiçbir yerde saklanmıyor; kullanıcıya bir kez
  -- gösterilip unutuluyor.
  token_hash   text        not null unique,
  created_at   timestamptz not null default now(),
  last_used_at timestamptz
);

create index if not exists api_tokens_user on api_tokens (user_id);

create table if not exists room_owners (
  scope   text   not null,
  room    text   not null,
  user_id bigint not null,
  claimed_at timestamptz not null default now(),
  primary key (scope, room)
);
"#;

#[derive(Clone)]
pub struct PgIdentity {
    pool: PgPool,
}

impl PgIdentity {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db_err(e: sqlx::Error) -> SyncError {
    SyncError::Http(e.to_string())
}

impl TokenStore for PgIdentity {
    fn lookup(&self, token_hash: &str) -> BoxFuture<'static, Result<Option<i64>, SyncError>> {
        let pool = self.pool.clone();
        let hash = token_hash.to_string();
        Box::pin(async move {
            // `last_used_at` aynı sorguda güncelleniyor: panelde "bu jeton
            // kullanılıyor mu" sorusunun cevabı, iptal kararının dayanağı.
            let row = sqlx::query(
                "update api_tokens set last_used_at = now()
                  where token_hash = $1
              returning user_id::bigint",
            )
            .bind(&hash)
            .fetch_optional(&pool)
            .await
            .map_err(db_err)?;

            row.map(|r| r.try_get::<i64, _>("user_id").map_err(db_err))
                .transpose()
        })
    }
}

impl OwnerStore for PgIdentity {
    fn claim(
        &self,
        scope: Scope,
        room: &str,
        user_id: i64,
    ) -> BoxFuture<'static, Result<i64, SyncError>> {
        let pool = self.pool.clone();
        let scope = scope.as_str();
        let room = room.to_string();
        Box::pin(async move {
            // Tek ifade, çünkü "bak sonra yaz" ATOMİK DEĞİL: iki cihaz aynı
            // anda katıldığında ikisi de sahipsiz görür ve izolasyon hiç
            // kurulmamış olur. `do update` kendine yazıyor — çakışmada mevcut
            // sahibi geri döndürmenin yolu bu.
            let row = sqlx::query(
                "insert into room_owners (scope, room, user_id)
                 values ($1, $2, $3)
                 on conflict (scope, room)
                   do update set scope = excluded.scope
                 returning user_id::bigint",
            )
            .bind(scope)
            .bind(&room)
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .map_err(db_err)?;

            row.try_get::<i64, _>("user_id").map_err(db_err)
        })
    }
}
