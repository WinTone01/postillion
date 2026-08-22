import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import db from '@adonisjs/lucid/services/db'
import User from '#models/user'
import { chats, devices } from '#services/registry'

/**
 * Cihaz ve sohbet listesi.
 *
 * Tablolar `postillion-server` tarafından kuruluyor; testler onları kendileri
 * oluşturuyor (panelin göçlerinde yok — iki yerde tanımlamak ikisinin
 * ayrılması demekti).
 */
test.group('Çalışma alanı', (group) => {
  group.setup(async () => {
    await db.rawQuery(`
      create table if not exists room_owners (
        scope text not null, room text not null, user_id bigint not null,
        claimed_at timestamptz not null default now(),
        primary key (scope, room)
      )`)
    await db.rawQuery(`
      create table if not exists registry_rows (
        org text not null, kind text not null, id text not null,
        seq bigint not null default 0, deleted boolean not null default false,
        del_hlc text, fields jsonb not null default '{}',
        clocks jsonb not null default '{}',
        primary key (org, kind, id)
      )`)
  })
  group.each.setup(() => testUtils.db().withGlobalTransaction())

  async function userWithOrg(org: string) {
    const user = await User.create({
      email: `w${Date.now()}${Math.random()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    await db.table('room_owners').insert({ scope: 'registry', room: org, user_id: user.id })
    return user
  }

  async function row(org: string, kind: string, id: string, fields: object) {
    await db.table('registry_rows').insert({ org, kind, id, fields: JSON.stringify(fields) })
  }

  const noPresence = async () => []

  test('cihazlar listeleniyor', async ({ assert }) => {
    const user = await userWithOrg('org-1')
    await row('org-1', 'devices', 'dev-a', { name: 'Dizüstü', platform: 'linux' })

    const list = await devices(user.id, noPresence)
    assert.lengthOf(list, 1)
    assert.equal(list[0].name, 'Dizüstü')
    assert.isFalse(list[0].online)
  })

  /// Canlılık kayıt satırından DEĞİL, sunucunun presence'ından geliyor:
  /// `lastSeenAt` yalnızca açılış/kapanışta yazılıyor ve açık duran bir
  /// cihaz orada saatler öncesinde görünüyor.
  test('çevrimiçi durumu presence ile geliyor', async ({ assert }) => {
    const user = await userWithOrg('org-2')
    await row('org-2', 'devices', 'dev-a', {
      name: 'Dizüstü',
      platform: 'linux',
      // Çok eski bir damga — buna bakılsaydı cihaz kapalı görünürdü.
      lastSeenAt: 1,
    })

    const list = await devices(user.id, async () => ['dev-a'])
    assert.isTrue(list[0].online, 'presence çevrimiçi diyorsa çevrimiçi olmalı')
  })

  test('başka kullanıcının cihazları görünmüyor', async ({ assert }) => {
    const ayse = await userWithOrg('org-ayse')
    const bora = await userWithOrg('org-bora')
    await row('org-ayse', 'devices', 'dev-ayse', { name: 'Ayşe' })

    assert.lengthOf(await devices(bora.id, noPresence), 0, 'izolasyon panelde de kurulmalı')
    assert.lengthOf(await devices(ayse.id, noPresence), 1)
  })

  test('sohbetler listeleniyor ve arşivlenenler gizleniyor', async ({ assert }) => {
    const user = await userWithOrg('org-3')
    await row('org-3', 'chats', 'c1', { title: 'Aktif', deviceId: 'dev-a', lastMessageAt: 200 })
    await row('org-3', 'chats', 'c2', { title: 'Arşiv', archived: true })

    const list = await chats(user.id, noPresence)
    assert.lengthOf(list, 1)
    assert.equal(list[0].title, 'Aktif')
  })

  test('başlıksız sohbet boş satır bırakmıyor', async ({ assert }) => {
    const user = await userWithOrg('org-4')
    await row('org-4', 'chats', 'c1', { deviceId: 'dev-a' })

    const list = await chats(user.id, noPresence)
    assert.equal(list[0].title, 'Başlıksız')
  })

  test('sohbetin cihazı açıksa işaretleniyor', async ({ assert }) => {
    const user = await userWithOrg('org-5')
    await row('org-5', 'chats', 'c1', { title: 'Test', deviceId: 'dev-a' })

    const kapali = await chats(user.id, noPresence)
    assert.isFalse(kapali[0].deviceOnline)

    const acik = await chats(user.id, async () => ['dev-a'])
    assert.isTrue(acik[0].deviceOnline, 'yazma bu şarta bağlı')
  })

  test('odası olmayan kullanıcı boş liste alıyor', async ({ assert }) => {
    const user = await User.create({
      email: `bos${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    assert.lengthOf(await devices(user.id, noPresence), 0)
    assert.lengthOf(await chats(user.id, noPresence), 0)
  })
})
