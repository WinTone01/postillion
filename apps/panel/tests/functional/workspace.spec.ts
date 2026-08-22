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

    const { items: list } = await devices(user.id, noPresence)
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

    const { items: list } = await devices(user.id, async () => ['dev-a'])
    assert.isTrue(list[0].online, 'presence çevrimiçi diyorsa çevrimiçi olmalı')
  })

  test('başka kullanıcının cihazları görünmüyor', async ({ assert }) => {
    const ayse = await userWithOrg('org-ayse')
    const bora = await userWithOrg('org-bora')
    await row('org-ayse', 'devices', 'dev-ayse', { name: 'Ayşe' })

    const boraList = await devices(bora.id, noPresence)
    assert.lengthOf(boraList.items, 0, 'izolasyon panelde de kurulmalı')
    const ayseList = await devices(ayse.id, noPresence)
    assert.lengthOf(ayseList.items, 1)
  })

  test('sohbetler listeleniyor ve arşivlenenler gizleniyor', async ({ assert }) => {
    const user = await userWithOrg('org-3')
    await row('org-3', 'chats', 'c1', { title: 'Aktif', deviceId: 'dev-a', lastMessageAt: 200 })
    await row('org-3', 'chats', 'c2', { title: 'Arşiv', archived: true })

    const { items: list } = await chats(user.id, noPresence)
    assert.lengthOf(list, 1)
    assert.equal(list[0].title, 'Aktif')
  })

  test('başlıksız sohbet boş satır bırakmıyor', async ({ assert }) => {
    const user = await userWithOrg('org-4')
    await row('org-4', 'chats', 'c1', { deviceId: 'dev-a' })

    const { items: list } = await chats(user.id, noPresence)
    assert.equal(list[0].title, 'Başlıksız')
  })

  test('sohbetin cihazı açıksa işaretleniyor', async ({ assert }) => {
    const user = await userWithOrg('org-5')
    await row('org-5', 'chats', 'c1', { title: 'Test', deviceId: 'dev-a' })

    const kapaliList = await chats(user.id, noPresence)
    const kapali = kapaliList.items
    assert.isFalse(kapali[0].deviceOnline)

    const acikList = await chats(user.id, async () => ['dev-a'])
    const acik = acikList.items
    assert.isTrue(acik[0].deviceOnline, 'yazma bu şarta bağlı')
  })

  test('odası olmayan kullanıcı boş liste alıyor', async ({ assert }) => {
    const user = await User.create({
      email: `bos${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const bosCihaz = await devices(user.id, noPresence)
    assert.lengthOf(bosCihaz.items, 0)
    const bosSohbet = await chats(user.id, noPresence)
    assert.lengthOf(bosSohbet.items, 0)
  })
})

/**
 * Ulaşılamamak ile çevrimdışı olmak AYRI.
 *
 * Panel bunları birleştiriyordu: presence çağrısı reddedilince (panel
 * kullanıcının değil işletmecinin kimliğiyle soruyordu) her cihaz kesin bir
 * dille "çevrimdışı" görünüyordu — açık duran bir makine için yanlış bilgi.
 */
test.group('Canlılık bilinmiyorsa', (group) => {
  group.each.setup(() => testUtils.db().withGlobalTransaction())

  async function userWithOrg(org: string) {
    const user = await User.create({
      email: `l${Date.now()}${Math.random()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    await db.table('room_owners').insert({ scope: 'registry', room: org, user_id: user.id })
    return user
  }

  const unreachable = async () => null

  test('sunucuya ulaşılamayınca cihaz durumu bilinmiyor sayılıyor', async ({ assert }) => {
    const user = await userWithOrg('org-ulasilamaz')
    await db.table('registry_rows').insert({
      org: 'org-ulasilamaz',
      kind: 'devices',
      id: 'dev-x',
      fields: JSON.stringify({ name: 'Dizüstü', platform: 'linux' }),
    })

    const list = await devices(user.id, unreachable)
    assert.lengthOf(list.items, 1, 'liste veritabanından geliyor, yine görünmeli')
    assert.isFalse(list.livenessKnown, 'çağrı düştüyse canlılık BİLİNMİYOR')
  })

  test('presence cevap verirse canlılık biliniyor', async ({ assert }) => {
    const user = await userWithOrg('org-cevapli')
    const list = await devices(user.id, async () => [])
    assert.isTrue(list.livenessKnown)
  })

  test('odası olmayan kullanıcı için canlılık biliniyor sayılıyor', async ({ assert }) => {
    // Soracak oda yok; bunu "bilinmiyor" saymak boş listeyi gereksizce
    // şüpheli gösterirdi.
    const user = await User.create({
      email: `n${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const list = await devices(user.id, unreachable)
    assert.isTrue(list.livenessKnown)
  })
})
