/**
 * Canlı transkript.
 *
 * Sayfa açıkken sohbeti düzenli yokluyor. WebSocket daha zarif olurdu ama
 * panelin sunucuyla arasında zaten bir vekil var (tarayıcı sunucu jetonunu
 * ASLA görmemeli) ve yoklama o vekilden geçiyor.
 *
 * Yoklama ucuz: panel elindeki baş sırayı gönderiyor, değişmemişse sunucu
 * belgeyi hiç kurmuyor ve `unchanged` dönüyor.
 */

const root = document.querySelector('[data-chat-id]')

if (root) {
  const chatId = root.dataset.chatId
  const list = root.querySelector('.transcript')
  let headSeq = Number(root.dataset.headSeq || 0)
  /** Art arda hata sayısı — geri çekilme buna göre. */
  let failures = 0

  const render = (messages) => {
    if (!list) {
      // Transkript hiç çizilmemişse (boş sohbet) sayfayı yenilemek en
      // basiti: ilk mesajla birlikte düzen de değişiyor.
      window.location.reload()
      return
    }
    list.replaceChildren(
      ...messages.map((message) => {
        const li = document.createElement('li')
        li.className = `msg ${message.role}`

        const who = document.createElement('span')
        who.className = 'who'
        who.textContent = message.role === 'user' ? 'Siz' : 'Ajan'
        li.append(who)

        for (const part of message.parts ?? []) {
          if (part.kind === 'text') {
            const p = document.createElement('p')
            p.className = 'text'
            // `textContent`: sohbet içeriği kullanıcı metni ve HTML olarak
            // yorumlanmamalı.
            p.textContent = part.text ?? ''
            li.append(p)
          } else if (part.kind === 'error') {
            const p = document.createElement('p')
            p.className = 'part-error'
            p.textContent = part.message ?? ''
            li.append(p)
          } else if (part.kind === 'tool') {
            const p = document.createElement('p')
            p.className = 'tool'
            p.textContent = part.call?.command ?? part.call?.path ?? 'araç'
            if (!part.resolved) {
              const pending = document.createElement('span')
              pending.className = 'pending'
              pending.textContent = 'çalışıyor'
              p.append(' ', pending)
            }
            li.append(p)
          }
        }
        return li
      })
    )
  }

  const poll = async () => {
    try {
      const response = await fetch(`/chats/${encodeURIComponent(chatId)}/messages?since=${headSeq}`, {
        headers: { accept: 'application/json' },
      })
      if (!response.ok) {
        throw new Error(String(response.status))
      }
      const body = await response.json()
      failures = 0
      headSeq = body.headSeq ?? headSeq
      if (body.messages) {
        render(body.messages)
      }
    } catch {
      // Sessizce geri çekiliyoruz: geçici bir arıza için ekrana hata basmak,
      // okunabilir duran bir transkripti gürültüye boğardı.
      failures += 1
    }
    // Üst üste hatada aralık açılıyor, 30 saniyede duruyor.
    const delay = Math.min(3000 * 2 ** Math.min(failures, 4), 30_000)
    window.setTimeout(poll, delay)
  }

  window.setTimeout(poll, 3000)
}
