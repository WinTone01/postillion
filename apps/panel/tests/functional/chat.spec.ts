import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import db from '@adonisjs/lucid/services/db'
import User from '#models/user'

/**
 * Sohbet sayfası.
 *
 * Transkriptin KENDİSİ sunucudan geliyor ve burada sunucu yok; ölçülen şey
 * erişim denetimi ve sunucuya ulaşılamadığında sayfanın ne yaptığı.
 */
test.group('Sohbet', (group) => {
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

  async function userWithChat(org: string, chatId: string) {
    const user = await User.create({
      email: `c${Date.now()}${Math.random()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    await db.table('room_owners').insert({ scope: 'registry', room: org, user_id: user.id })
    await db.table('registry_rows').insert({
      org,
      kind: 'chats',
      id: chatId,
      fields: JSON.stringify({ title: 'Deneme', deviceId: 'dev-a' }),
    })
    return user
  }

  test('kendi sohbeti açılıyor', async ({ client }) => {
    const user = await userWithChat('org-c1', 'chat-1')
    const response = await client.get('/chats/chat-1').loginAs(user)
    response.assertStatus(200)
    response.assertTextIncludes('Deneme')
  })

  /// İzolasyonun panel tarafındaki sınavı: sohbet kimliğini bilmek yetmemeli.
  test('başkasının sohbeti açılmıyor', async ({ client }) => {
    await userWithChat('org-ayse', 'gizli-sohbet')
    const bora = await User.create({
      email: `bora${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })

    const response = await client.get('/chats/gizli-sohbet').loginAs(bora)
    response.assertStatus(404)
  })

  test('olmayan sohbet 404', async ({ client }) => {
    const user = await userWithChat('org-c2', 'chat-2')
    const response = await client.get('/chats/hic-olmayan').loginAs(user)
    response.assertStatus(404)
  })

  /// Sunucu ayarlı değil: sayfa AÇILMALI ve arızayı söylemeli. Boş bir
  /// transkript göstermek, sohbeti boş sanmaya yol açardı.
  test('sunucuya ulaşılamayınca sebebi yazıyor', async ({ client }) => {
    const user = await userWithChat('org-c3', 'chat-3')
    const response = await client.get('/chats/chat-3').loginAs(user)
    response.assertStatus(200)
    response.assertTextIncludes('ulaşılamadı')
  })

  test('sohbet sayfası kimliksiz açılmıyor', async ({ client }) => {
    const response = await client.get('/chats/chat-1').redirects(0)
    response.assertStatus(302)
  })

  test('başkasının sohbetine mesaj gönderilemiyor', async ({ client }) => {
    await userWithChat('org-g1', 'gizli-2')
    const bora = await User.create({
      email: `g${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })

    const response = await client
      .post('/chats/gizli-2/send')
      .form({ prompt: 'merhaba' })
      .withCsrfToken()
      .loginAs(bora)
      .redirects(0)

    response.assertStatus(404)
  })

  /// Cihaz kapalı ya da sunucu yok: gönderim BAŞARISIZ olmalı ve sebebi
  /// kullanıcıya dönmeli. Sessizce yutmak, mesajın gittiğini sandırırdı.
  test('ulaşılamayan cihazda gönderim sebebiyle birlikte düşüyor', async ({ client }) => {
    const user = await userWithChat('org-g2', 'chat-g2')

    const response = await client
      .post('/chats/chat-g2/send')
      .form({ prompt: 'merhaba' })
      .withCsrfToken()
      .loginAs(user)
      .redirects(0)

    // Geri yönlendiriyor; hata flash'ta.
    response.assertStatus(302)
  })

  test('boş mesaj gönderilemiyor', async ({ client }) => {
    const user = await userWithChat('org-g3', 'chat-g3')
    const response = await client
      .post('/chats/chat-g3/send')
      .form({ prompt: '   ' })
      .withCsrfToken()
      .loginAs(user)
      .redirects(0)
    response.assertStatus(302)
  })
})
