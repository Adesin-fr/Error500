import vue from '@vitejs/plugin-vue'
import fs from 'node:fs'
import { defineConfig } from 'vitest/config'

// Le HTML racine est rendu par le serveur Rust (Inertia) : pas d'index.html,
// l'entrée est src/main.ts et le manifeste est lu par le binaire.
export default defineConfig({
  // `fs` explicite : permet de résoudre les types importés dans defineProps (aussi sous vitest/happy-dom).
  plugins: [
    vue({
      script: {
        fs: { fileExists: fs.existsSync, readFile: (file) => fs.readFileSync(file, 'utf-8'), realpath: fs.realpathSync },
      },
    }),
  ],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    manifest: 'manifest.json',
    rollupOptions: { input: 'src/main.ts' },
  },
  server: { port: 5173, strictPort: true, cors: true, origin: 'http://localhost:5173' },
  test: { environment: 'happy-dom' },
})
