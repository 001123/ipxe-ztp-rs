import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

// The backend reads its port from `PORT` (see `config/development.yaml`:
// `get_env(name="PORT", default="5150")`), so this proxy has to read the same
// variable. Hardcoding the default silently sends every `/api` call to 5150 the
// moment anyone overrides it.
//
// `loadEnv` is Vite's own reader; the empty prefix takes shell variables too.
// It is used here for the dev server only — and to inject the test login
// credentials into the client bundle (see `testLoginEnv` below). Nothing else
// from .env reaches the client.
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '..', '')
  const backendPort = env.PORT || '5150'
  // `HOST=0.0.0.0 pnpm dev` exposes the dev server on the LAN; the default
  // stays localhost-only.
  const devHost = env.HOST || 'localhost'

  // Pre-fill the login form in dev: when the dev server runs with IPXE_ZTP_IS_TEST=true
  // in .env, the Login page reads these constants and starts with the
  // credentials already typed in. Gated on `mode === "development"` so
  // `vite build` never bakes the password into a production bundle. Set
  // IPXE_ZTP_TEST_PASSWORD to '' (or IPXE_ZTP_IS_TEST to anything else) to opt out.
  const testLoginEnabled =
    mode === 'development' && (env.IPXE_ZTP_IS_TEST || '').toLowerCase() === 'true'
  const testLoginEnv = {
    'import.meta.env.VITE_IPXE_ZTP_IS_TEST': JSON.stringify(env.IPXE_ZTP_IS_TEST || ''),
    'import.meta.env.VITE_IPXE_ZTP_TEST_EMAIL': JSON.stringify(
      testLoginEnabled ? (env.IPXE_ZTP_TEST_EMAIL || '') : '',
    ),
    'import.meta.env.VITE_IPXE_ZTP_TEST_PASSWORD': JSON.stringify(
      testLoginEnabled ? (env.IPXE_ZTP_TEST_PASSWORD || '') : '',
    ),
  }

  return {
    plugins: [react(), tailwindcss()],
    define: testLoginEnv,
    resolve: {
      alias: { '@': path.resolve(import.meta.dirname, './src') },
    },
    build: { outDir: 'dist' },
    server: {
      host: devHost,
      port: 5173,
      proxy: { '/api': `http://localhost:${backendPort}` },
    },
  }
})
