import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';
import { pluginTailwindcss } from '@rsbuild/plugin-tailwindcss';
import { tanstackRouter } from '@tanstack/router-plugin/rspack';

// Docs: https://rsbuild.rs/config/
export default defineConfig({
  plugins: [
    pluginReact({
      // TanStack Table v9 的 useTable 使用 render-phase store（get/markCommitted 配对），
      // 与 React Compiler 自动 memo 化冲突。已在 VirtualTable.tsx / users.tsx 顶部加 "use no memo" 豁免。
      reactCompiler: true,
    }),
    pluginTailwindcss(),
  ],
  resolve: {
    alias: {
      // 与 tsconfig paths 保持一致（shadcn 组件使用 @/ 导入）
      '@': './src',
    },
  },
  server: {
    proxy: {
      // 后端无 CORS，dev 用同源代理转发 /api
      '/api': {
        target: 'http://127.0.0.1:8081',
        changeOrigin: true,
      },
    },
  },
  tools: {
    rspack: {
      plugins: [
        tanstackRouter({
          target: 'react',
          autoCodeSplitting: true,
        }),
      ],
    },
  },
});
