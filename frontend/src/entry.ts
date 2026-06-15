/**
 * Point d'entrée du bundle MODULE keestore (la page /keestore), chargé à
 * l'exécution. Buildé séparément via `vite.module.config.ts` : les specifiers
 * partagés (`@kubuno/sdk`, `@ui`, react…) sont externes et résolus au runtime
 * par l'import map du host ; kdbxweb + hash-wasm restent bundlés localement
 * (coffre KeePass chiffré côté client). Le host importe ce fichier puis appelle
 * `register()` ; `sdkVersion` permet de rejeter une incompatibilité de contrat.
 */
import { lazy } from 'react'
import {
  RouteRegistry,
  WaffleAppRegistry,
  FaviconRegistry,
  useSidebarStore,
  SDK_VERSION,
} from '@kubuno/sdk'
import './index.css'
import './i18n'
import KeeStoreLogo from './KeeStoreLogo'

export const sdkVersion = SDK_VERSION

export function register() {
  FaviconRegistry.register('keestore', '/keestore-logo.svg')

  WaffleAppRegistry.register('keestore', 'Keestore', [
    { id: 'keestore', label: 'Keestore', Icon: KeeStoreLogo, path: '/keestore' },
  ])

  useSidebarStore.getState().register({
    moduleId:    'keestore',
    routePrefix: '/keestore',
  })

  // Routes
  const KeeStorePage = lazy(() => import('./KeeStorePage'))

  RouteRegistry.register('keestore', KeeStorePage)
}
