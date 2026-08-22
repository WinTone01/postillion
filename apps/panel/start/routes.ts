/*
|--------------------------------------------------------------------------
| Routes file
|--------------------------------------------------------------------------
*/

import router from '@adonisjs/core/services/router'
import { middleware } from './kernel.js'

const AuthController = () => import('#controllers/auth_controller')

// Giriş yapmış kullanıcı kayıt/giriş sayfasını görmemeli.
router
  .group(() => {
    router.get('/register', [AuthController, 'showRegister'])
    router.post('/register', [AuthController, 'register'])
    router.get('/login', [AuthController, 'showLogin'])
    router.post('/login', [AuthController, 'login'])
  })
  .use(middleware.guest())

router.post('/logout', [AuthController, 'logout']).use(middleware.auth())

// Panelin kendisi kimlik ister.
router.on('/').render('pages/home').use(middleware.auth())
