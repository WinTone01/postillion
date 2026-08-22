import { defineConfig } from 'vite'
import adonisjs from '@adonisjs/vite/client'

export default defineConfig({
  build: {
    // `public/assets` DEĞİL: tanıtım sayfasının görselleri orada duruyor ve
    // Vite derlemede o dizini temizleyip kendi çıktısını yazıyordu — yerelde
    // görünmeyen, yalnızca imajda ortaya çıkan bir kayıp.
    outDir: 'public/build',
  },
  plugins: [
    adonisjs({
      /**
       * Entrypoints of your application. Each entrypoint will
       * result in a separate bundle.
       */
      entrypoints: ['resources/css/app.css', 'resources/js/app.js'],

      /**
       * Paths to watch and reload the browser on file change
       */
      reload: ['resources/views/**/*.edge'],
    }),
  ],
})
