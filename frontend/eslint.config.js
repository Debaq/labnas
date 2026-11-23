import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // Convencion del proyecto: `catch {}` para operaciones best-effort (la UI sigue igual si fallan)
      'no-empty': ['error', { allowEmptyCatch: true }],
      // eslint-plugin-react-hooks 7.1 endurecio estas reglas del React Compiler; el codigo
      // existente (cargar datos en useEffect) las dispara. Advertencia hasta refactorizarlo.
      'react-hooks/set-state-in-effect': 'warn',
      'react-hooks/immutability': 'warn',
      // Contextos que exportan su hook junto al Provider
      'react-refresh/only-export-components': ['error', { allowConstantExport: true, allowExportNames: ['useAuth', 'useToast', 'useTheme'] }],
    },
  },
])
