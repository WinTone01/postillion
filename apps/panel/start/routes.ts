/*
|--------------------------------------------------------------------------
| Routes file
|--------------------------------------------------------------------------
*/

import router from '@adonisjs/core/services/router'
import { middleware } from './kernel.js'

const AuthController = () => import('#controllers/auth_controller')
const TokensController = () => import('#controllers/tokens_controller')
const WorkspaceController = () => import('#controllers/workspace_controller')

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
router
  .group(() => {
    router.get('/', [WorkspaceController, 'index'])
    router.get('/tokens', [TokensController, 'index'])
    router.post('/tokens', [TokensController, 'store'])
    // Tarayıcı formları yalnızca GET/POST gönderiyor; `?_method=DELETE`
    // Adonis'in yöntem sahteciliğiyle gerçek DELETE'e çevriliyor.
    router.delete('/tokens/:id', [TokensController, 'destroy'])
  })
  .use(middleware.auth())
