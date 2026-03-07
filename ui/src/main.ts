import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import Aura from '@primeuix/themes/aura'
import 'primeicons/primeicons.css'

import App from './App.vue'
import router from './router'

const app = createApp(App)

// PrimeVue is configured here for use in subsequent phases.
// Phase 1 uses basic HTML; Phase 4+ will adopt DataTable, Button, Dialog, etc.
app.use(PrimeVue, {
  theme: {
    preset: Aura,
  },
})

app.use(router)
app.mount('#app')
