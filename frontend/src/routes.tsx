import { createBrowserRouter, Navigate } from 'react-router'
import { App } from './App'
import { getToken } from './auth/token'
import { Login } from './auth/Login'
import { RequireAuth } from './auth/RequireAuth'
import { Home } from './pages/Home'
import { Machines } from './pages/Machines'
import { OsVersions } from './pages/OsVersions'
import { Settings } from './pages/Settings'

function IndexRedirect() {
  // Signed-in users land on the machines list; the public home stays for guests.
  if (getToken() !== null) {
    return <Navigate to="/machines" replace />
  }
  return <Home />
}

export const router = createBrowserRouter([
  {
    path: '/',
    element: <App />,
    children: [
      { index: true, element: <IndexRedirect /> },
      { path: 'login', element: <Login /> },
      {
        element: <RequireAuth />,
        children: [
          { path: 'machines', element: <Machines /> },
          { path: 'os-versions', element: <OsVersions /> },
          { path: 'settings', element: <Settings /> },
        ],
      },
    ],
  },
])
