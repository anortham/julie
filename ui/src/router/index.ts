import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from '@/views/Dashboard.vue'
import Projects from '@/views/Projects.vue'
import Search from '@/views/Search.vue'
import Memories from '@/views/Memories.vue'
import Agents from '@/views/Agents.vue'
import Standup from '@/views/Standup.vue'

const router = createRouter({
  history: createWebHistory('/ui/'),
  routes: [
    {
      path: '/',
      name: 'dashboard',
      component: Dashboard,
    },
    {
      path: '/projects',
      name: 'projects',
      component: Projects,
    },
    {
      path: '/search',
      name: 'search',
      component: Search,
    },
    {
      path: '/memories',
      name: 'memories',
      component: Memories,
    },
    {
      path: '/agents',
      name: 'agents',
      component: Agents,
    },
    {
      path: '/standup',
      name: 'standup',
      component: Standup,
    },
  ],
})

export default router
